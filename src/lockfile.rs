use std::{
    collections::HashSet,
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use globset::Glob;
use url::Url;

use crate::{
    lockfile_table::{LockedCapsuleTable, LockfileTable},
    manifest::Capsule,
    resolve::ResolvedSource,
};

pub const LOCKFILE_NAME: &str = "upstream.lock";

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    UrlError(#[from] url::ParseError),

    #[error(transparent)]
    TomlDeError(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerError(#[from] toml::ser::Error),

    #[error(transparent)]
    GlobError(#[from] globset::Error),

    #[error("invalid source: no prefix")]
    NoPrefix,

    #[error("invalid source: bad prefix")]
    BadPrefix,

    #[error("capsule name is duplicated")]
    DuplicatingCapsules,
}

#[derive(Clone, Debug)]
pub enum LockedSource {
    Git(Url),
    Path(PathBuf),
}

impl LockedSource {
    pub(crate) fn from_resolved(resolved: ResolvedSource) -> Self {
        match resolved {
            ResolvedSource::Git {
                mut url,
                git_ref,
                commit,
            } => {
                url.query_pairs_mut()
                    .append_pair("ref", git_ref.as_str())
                    .append_pair("rev", &commit);
                Self::Git(url)
            }
            ResolvedSource::Path(path) => Self::Path(path),
        }
    }
}

impl fmt::Display for LockedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockedSource::Git(url) => write!(f, "git+{}", url),
            LockedSource::Path(path) => write!(f, "path+{}", path.display()),
        }
    }
}

impl FromStr for LockedSource {
    type Err = LockfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((prefix, body)) = s.split_once('+') {
            match prefix {
                "git" => Ok(Self::Git(body.parse()?)),
                "path" => Ok(Self::Path(PathBuf::from(body))),
                _ => Err(LockfileError::BadPrefix),
            }
        } else {
            Err(LockfileError::NoPrefix)
        }
    }
}

#[derive(Clone, Debug)]
pub struct LockedCapsule {
    pub name: String,
    pub source: LockedSource,
    pub src: PathBuf,
    pub dst: PathBuf,
    pub filter: Option<Glob>,
}

impl LockedCapsule {
    pub(crate) fn from_resolved(resolved: ResolvedSource, spec: &Capsule) -> Self {
        Self {
            name: spec.name().to_owned(),
            source: LockedSource::from_resolved(resolved),
            src: spec.src().to_path_buf(),
            dst: spec.dst().to_path_buf(),
            filter: spec.filter().cloned(),
        }
    }

    fn from_table(table: LockedCapsuleTable) -> Result<Self, LockfileError> {
        let LockedCapsuleTable {
            name,
            source,
            src,
            dst,
            filter,
        } = table;

        let source = source.parse()?;
        let filter = filter.map(|f| Glob::new(&f)).transpose()?;

        Ok(Self {
            name,
            source,
            src,
            dst,
            filter,
        })
    }

    fn to_table(&self) -> LockedCapsuleTable {
        LockedCapsuleTable {
            name: self.name.clone(),
            source: self.source.to_string(),
            src: self.src.clone(),
            dst: self.dst.clone(),
            filter: self.filter.as_ref().map(|filter| filter.glob().to_owned()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Lockfile {
    pub capsules: Vec<LockedCapsule>,
}

impl Lockfile {
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), LockfileError> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        self.write(&mut file)
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> Result<(), LockfileError> {
        let content = self.write_to_string()?;
        writer.write_all(content.as_bytes()).map_err(Into::into)
    }

    pub fn write_to_string(&self) -> Result<String, LockfileError> {
        let table = self.to_table();
        toml::to_string_pretty(&table).map_err(Into::into)
    }

    fn to_table(&self) -> LockfileTable {
        LockfileTable {
            capsules: self
                .capsules
                .iter()
                .map(|capsule| capsule.to_table())
                .collect(),
        }
    }

    /// Parse lockfile from given path
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, LockfileError> {
        let mut file = fs::File::open(path)?;
        Self::from_reader(&mut file)
    }

    /// Parse lockfile from reader
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, LockfileError> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Self::parse(&buf)
    }

    /// Parse lockfile from string
    pub fn parse(input: &str) -> Result<Self, LockfileError> {
        Self::from_table(toml::from_str(input)?)
    }

    fn from_table(table: LockfileTable) -> Result<Self, LockfileError> {
        let capsules = table
            .capsules
            .into_iter()
            .map(LockedCapsule::from_table)
            .collect::<Result<_, _>>()?;
        let lockfile = Self { capsules };

        // Check names are unique
        let mut capsule_names = HashSet::new();
        for name in lockfile.capsules.iter().map(|capsule| &capsule.name) {
            if !capsule_names.insert(name) {
                return Err(LockfileError::DuplicatingCapsules);
            }
        }

        Ok(lockfile)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::Lockfile;

    const LOCK_CAPSULE_GIT_REV: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://github.com/org/repo.git?ref=abc&rev=abc"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_GIT_TAG: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://github.com/org/repo.git?ref=v1.0.0&rev=abc"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_GIT_BRANCH: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://github.com/org/repo.git?ref=main&rev=abc"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_GIT_SSH: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+ssh://git@github.com/org/repo.git?ref=main&rev=abc"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_GIT_WITHOUT_REFERENCE: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://github.com/org/repo.git"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_ABSOLUTE_SOURCE_PATH: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_CAPSULE_RELATIVE_SOURCE_PATH: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+../local/repo"
src = "."
dst = "."
"###;
    const LOCK_CAPSULE_SOURCE_PATH_WITH_PLUS: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo+extra"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_MULTIPLE_CAPSULES: &str = r###"[[capsule]]
name = "first"
source = "git+https://github.com/org/repo.git?ref=main&rev=abc"
src = "source/path"
dst = "first/destination"
filter = "**/*.proto"

[[capsule]]
name = "second"
source = "path+/path/to/local/repo"
src = "other/source"
dst = "second/destination"
"###;
    const LOCK_DISTINCT_NAMES_SAME_SOURCE: &str = r###"[[capsule]]
name = "first"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"

[[capsule]]
name = "second"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_SOURCE_WITHOUT_PREFIX: &str = r###"[[capsule]]
name = "my-capsule"
source = "https://github.com/org/repo.git"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_SOURCE_EMPTY: &str = r###"[[capsule]]
name = "my-capsule"
source = ""
src = "source/path"
dst = "target/path"
"###;
    const LOCK_SOURCE_BAD_PREFIX: &str = r###"[[capsule]]
name = "my-capsule"
source = "https+https://github.com/org/repo.git"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_SOURCE_EMPTY_PREFIX: &str = r###"[[capsule]]
name = "my-capsule"
source = "+/path/to/local/repo"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_GIT_RELATIVE_URL: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+../local/repo"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_GIT_EMPTY_URL: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_GIT_INVALID_HOST: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://[invalid]/org/repo.git"
src = "source/path"
dst = "target/path"
"###;
    const LOCK_DUPLICATE_NAMES: &str = r###"[[capsule]]
name = "my-capsule"
source = "git+https://github.com/org/repo.git?ref=main&rev=abc"
src = "source/path"
dst = "first/destination"

[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "other/source"
dst = "second/destination"
"###;
    const LOCK_NONADJACENT_DUPLICATE_NAMES: &str = r###"[[capsule]]
name = "first"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "first/destination"

[[capsule]]
name = "second"
source = "path+/path/to/local/repo"
src = "other/source"
dst = "second/destination"

[[capsule]]
name = "first"
source = "path+/path/to/other/repo"
src = "third/source"
dst = "third/destination"
"###;
    const LOCK_LATER_CAPSULE_INVALID_SOURCE: &str = r###"[[capsule]]
name = "first"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "first/destination"

[[capsule]]
name = "second"
source = "git+../local/repo"
src = "other/source"
dst = "second/destination"
"###;

    const LOCK_CAPSULE_FILTER_EMPTY: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = ""
"###;
    const LOCK_CAPSULE_FILTER_RECURSIVE: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/*.proto"
"###;
    const LOCK_CAPSULE_FILTER_ALTERNATES: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/*.{proto,yml}"
"###;
    const LOCK_CAPSULE_FILTER_CHARACTER_CLASS: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/[a-z]?.proto"
"###;
    const LOCK_CAPSULE_FILTER_NEGATED_CHARACTER_CLASS: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/[!0-9]*.proto"
"###;
    const LOCK_CAPSULE_FILTER_ESCAPED_LITERALS: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/\\[literal\\].proto"
"###;
    const LOCK_CAPSULE_FILTER_NESTED_ALTERNATES: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/*.{proto,{yml,yaml}}"
"###;
    const LOCK_CAPSULE_FILTER_UNCLOSED_CLASS: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/[abc"
"###;
    const LOCK_CAPSULE_FILTER_REVERSED_RANGE: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/[z-a].proto"
"###;
    const LOCK_CAPSULE_FILTER_UNCLOSED_ALTERNATES: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/*.{proto,yml"
"###;
    const LOCK_CAPSULE_FILTER_UNOPENED_ALTERNATES: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/*.proto}"
"###;
    const LOCK_CAPSULE_FILTER_DANGLING_ESCAPE: &str = r###"[[capsule]]
name = "my-capsule"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "target/path"
filter = "**/file\\"
"###;
    const LOCK_LATER_CAPSULE_INVALID_FILTER: &str = r###"[[capsule]]
name = "first"
source = "path+/path/to/local/repo"
src = "source/path"
dst = "first/destination"
filter = "**/*.proto"

[[capsule]]
name = "second"
source = "path+/path/to/local/repo"
src = "other/source"
dst = "second/destination"
filter = "**/[abc"
"###;

    #[rstest]
    #[case("")]
    #[case::capsule_git_rev(LOCK_CAPSULE_GIT_REV)]
    #[case::capsule_git_tag(LOCK_CAPSULE_GIT_TAG)]
    #[case::capsule_git_branch(LOCK_CAPSULE_GIT_BRANCH)]
    #[case::capsule_git_ssh(LOCK_CAPSULE_GIT_SSH)]
    #[case::capsule_git_without_reference(LOCK_CAPSULE_GIT_WITHOUT_REFERENCE)]
    #[case::capsule_absolute_source_path(LOCK_CAPSULE_ABSOLUTE_SOURCE_PATH)]
    #[case::capsule_relative_source_path(LOCK_CAPSULE_RELATIVE_SOURCE_PATH)]
    #[case::capsule_source_path_with_plus(LOCK_CAPSULE_SOURCE_PATH_WITH_PLUS)]
    #[case::multiple_capsules(LOCK_MULTIPLE_CAPSULES)]
    #[case::distinct_names_same_source(LOCK_DISTINCT_NAMES_SAME_SOURCE)]
    #[case::capsule_filter_empty(LOCK_CAPSULE_FILTER_EMPTY)]
    #[case::capsule_filter_recursive(LOCK_CAPSULE_FILTER_RECURSIVE)]
    #[case::capsule_filter_alternates(LOCK_CAPSULE_FILTER_ALTERNATES)]
    #[case::capsule_filter_character_class(LOCK_CAPSULE_FILTER_CHARACTER_CLASS)]
    #[case::capsule_filter_negated_character_class(LOCK_CAPSULE_FILTER_NEGATED_CHARACTER_CLASS)]
    #[case::capsule_filter_escaped_literals(LOCK_CAPSULE_FILTER_ESCAPED_LITERALS)]
    #[case::capsule_filter_nested_alternates(LOCK_CAPSULE_FILTER_NESTED_ALTERNATES)]
    #[should_panic(expected = "NoPrefix")]
    #[case::source_without_prefix(LOCK_SOURCE_WITHOUT_PREFIX)]
    #[should_panic(expected = "NoPrefix")]
    #[case::source_empty(LOCK_SOURCE_EMPTY)]
    #[should_panic(expected = "BadPrefix")]
    #[case::source_bad_prefix(LOCK_SOURCE_BAD_PREFIX)]
    #[should_panic(expected = "BadPrefix")]
    #[case::source_empty_prefix(LOCK_SOURCE_EMPTY_PREFIX)]
    #[should_panic(expected = "UrlError")]
    #[case::git_relative_url(LOCK_GIT_RELATIVE_URL)]
    #[should_panic(expected = "UrlError")]
    #[case::git_empty_url(LOCK_GIT_EMPTY_URL)]
    #[should_panic(expected = "UrlError")]
    #[case::git_invalid_host(LOCK_GIT_INVALID_HOST)]
    #[should_panic(expected = "DuplicatingCapsules")]
    #[case::duplicate_names(LOCK_DUPLICATE_NAMES)]
    #[should_panic(expected = "DuplicatingCapsules")]
    #[case::nonadjacent_duplicate_names(LOCK_NONADJACENT_DUPLICATE_NAMES)]
    #[should_panic(expected = "UrlError")]
    #[case::later_capsule_invalid_source(LOCK_LATER_CAPSULE_INVALID_SOURCE)]
    #[should_panic(expected = "GlobError")]
    #[case::capsule_filter_unclosed_class(LOCK_CAPSULE_FILTER_UNCLOSED_CLASS)]
    #[should_panic(expected = "GlobError")]
    #[case::capsule_filter_reversed_range(LOCK_CAPSULE_FILTER_REVERSED_RANGE)]
    #[should_panic(expected = "GlobError")]
    #[case::capsule_filter_unclosed_alternates(LOCK_CAPSULE_FILTER_UNCLOSED_ALTERNATES)]
    #[should_panic(expected = "GlobError")]
    #[case::capsule_filter_unopened_alternates(LOCK_CAPSULE_FILTER_UNOPENED_ALTERNATES)]
    #[should_panic(expected = "GlobError")]
    #[case::capsule_filter_dangling_escape(LOCK_CAPSULE_FILTER_DANGLING_ESCAPE)]
    #[should_panic(expected = "GlobError")]
    #[case::later_capsule_invalid_filter(LOCK_LATER_CAPSULE_INVALID_FILTER)]
    fn test_parse(#[case] input: &str) {
        let _ = Lockfile::parse(input).unwrap();
    }
}
