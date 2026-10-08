use std::{
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use globset::Glob;
use url::Url;

use crate::manifest_table::{
    CapsuleTable, DefaultTable, GitRefValues, GitSourceTable, ManifestTable,
};

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    TomlDeError(#[from] toml::de::Error),

    #[error(transparent)]
    GlobError(#[from] globset::Error),

    #[error(transparent)]
    UrlError(#[from] url::ParseError),

    #[error("\"git\" and \"path\" cannot be specified at the same time")]
    GitAndPath,

    #[error("only one of \"rev\", \"tag\" and \"branch\" can be specified")]
    RevBranchTag,

    #[error("src path must be relative")]
    RelativeSrc,

    #[error("dst path must be relative")]
    RelativeDst,

    #[error("unresolved source")]
    UnresolvedSource,
}

fn filter_from_table_with_defaults(
    filter: Option<String>,
    defaults: &Defaults,
) -> Result<Option<Glob>, ManifestError> {
    if let Some(filter) = filter {
        Ok(Some(Glob::new(&filter)?))
    } else {
        Ok(defaults.filter.clone())
    }
}

/// Type of git reference
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitRef {
    Rev(String),
    Tag(String),
    Branch(String),
}

impl GitRef {
    fn from_table(table: GitRefValues) -> Result<Option<Self>, ManifestError> {
        let GitRefValues { rev, tag, branch } = table;
        match (rev, tag, branch) {
            (None, None, None) => Ok(None),
            (None, None, Some(branch)) => Ok(Some(Self::Branch(branch))),
            (None, Some(tag), None) => Ok(Some(Self::Tag(tag))),
            (Some(rev), None, None) => Ok(Some(Self::Rev(rev))),
            _ => Err(ManifestError::RevBranchTag),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            GitRef::Rev(commit) => commit.as_str(),
            GitRef::Tag(tag_name) => tag_name.as_str(),
            GitRef::Branch(branch_name) => branch_name.as_str(),
        }
    }
}

impl fmt::Display for GitRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitRef::Rev(commit_hash) => commit_hash.fmt(f),
            GitRef::Tag(tag_name) => tag_name.fmt(f),
            GitRef::Branch(branch_name) => branch_name.fmt(f),
        }
    }
}

/// Git source specification
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitSource {
    url: Url,
    git_ref: GitRef,
}

impl GitSource {
    /// Git URL specification
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Git reference specification
    pub fn git_ref(&self) -> &GitRef {
        &self.git_ref
    }

    fn from_table(table: GitSourceTable, defaults: &Defaults) -> Result<Self, ManifestError> {
        let GitSourceTable { url, ref_ } = table;

        let git_ref = GitRef::from_table(ref_)?;

        if let Some(url) = url {
            let url = Url::parse(&url)?;
            Ok(Self {
                url,
                git_ref: git_ref.ok_or(ManifestError::UnresolvedSource)?,
            })
        } else {
            // If this source doesn't overwrite the URL, then it must come from default
            let url = defaults
                .git_url
                .clone()
                .ok_or(ManifestError::UnresolvedSource)?;

            let git_ref = if let Some(git_ref) = git_ref {
                git_ref
            } else {
                defaults
                    .git_ref
                    .clone()
                    .ok_or(ManifestError::UnresolvedSource)?
            };

            Ok(Self { url, git_ref })
        }
    }
}

impl fmt::Display for GitSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}?ref={}", self.url, self.git_ref)
    }
}

/// Source specification
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    Git(GitSource),
    Path(PathBuf),
}

impl Source {
    fn from_table_values(
        git: Option<GitSourceTable>,
        path: Option<PathBuf>,
        defaults: &Defaults,
    ) -> Result<Self, ManifestError> {
        match (git, path) {
            (None, None) => defaults.source().ok_or(ManifestError::UnresolvedSource),
            (None, Some(path)) => Ok(Self::Path(path)),
            (Some(table), None) => Ok(Self::Git(GitSource::from_table(table, defaults)?)),
            (Some(_), Some(_)) => Err(ManifestError::GitAndPath),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Git(git) => git.fmt(f),
            Source::Path(path) => path.display().fmt(f),
        }
    }
}

/// Capsule specification in the manifest
#[derive(Clone, Debug)]
pub struct Capsule {
    name: String,
    src: PathBuf,
    dst: PathBuf,
    source: Source,
    filter: Option<Glob>,
}

impl Capsule {
    /// Capsule name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Source path in the source repository
    pub fn src(&self) -> &Path {
        &self.src
    }

    /// Destination path in the workspace
    pub fn dst(&self) -> &Path {
        &self.dst
    }

    /// Source repository specification
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// Filter specification
    pub fn filter(&self) -> Option<&Glob> {
        self.filter.as_ref()
    }

    fn from_table(
        name: String,
        table: CapsuleTable,
        defaults: &Defaults,
    ) -> Result<Self, ManifestError> {
        let CapsuleTable {
            src,
            dst,
            git,
            path,
            filter,
        } = table;

        if !src.is_relative() {
            return Err(ManifestError::RelativeSrc);
        }
        if !dst.is_relative() {
            return Err(ManifestError::RelativeDst);
        }

        let source = Source::from_table_values(git, path, defaults)?;
        let filter = filter_from_table_with_defaults(filter, defaults)?;

        Ok(Self {
            name,
            src,
            dst,
            source,
            filter,
        })
    }
}

/// Manifest
#[derive(Clone, Debug)]
pub struct Manifest {
    capsules: Vec<Capsule>,
}

impl Manifest {
    pub fn capsules(&self) -> &[Capsule] {
        &self.capsules
    }

    /// Parse manifest from given path
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let mut file = fs::File::open(path)?;
        Self::from_reader(&mut file)
    }

    /// Parse manifest from reader
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, ManifestError> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Self::parse(&buf)
    }

    /// Parse manifest from string
    pub fn parse(input: &str) -> Result<Self, ManifestError> {
        Self::from_table(toml::from_str(input)?)
    }

    fn from_table(table: ManifestTable) -> Result<Self, ManifestError> {
        let ManifestTable { default, capsules } = table;

        let defaults = Defaults::from_table(default)?;

        let capsules = capsules
            .into_iter()
            .map(|(name, table)| Capsule::from_table(name, table, &defaults))
            .collect::<Result<_, _>>()?;

        Ok(Self { capsules })
    }
}

#[derive(Debug, Default)]
struct Defaults {
    git_url: Option<Url>,
    git_ref: Option<GitRef>,
    path: Option<PathBuf>,
    filter: Option<Glob>,
}

impl Defaults {
    fn source(&self) -> Option<Source> {
        match (&self.git_url, &self.git_ref, &self.path) {
            (None, None, Some(path)) => Some(Source::Path(path.clone())),
            (Some(url), Some(ref_), None) => Some(Source::Git(GitSource {
                url: url.clone(),
                git_ref: ref_.clone(),
            })),
            _ => None,
        }
    }

    fn from_table(table: DefaultTable) -> Result<Self, ManifestError> {
        let DefaultTable { git, path, filter } = table;

        let filter = filter.map(|f| Glob::new(&f)).transpose()?;

        match (git, path) {
            (None, None) => Ok(Self {
                filter,
                ..Default::default()
            }),
            (None, Some(path)) => Ok(Self {
                path: Some(path),
                filter,
                ..Default::default()
            }),
            (Some(git), None) => {
                let GitSourceTable { url, ref_ } = git;
                Ok(Self {
                    git_url: url.map(|url| Url::parse(&url)).transpose()?,
                    git_ref: GitRef::from_table(ref_)?,
                    filter,
                    ..Default::default()
                })
            }
            (Some(_), Some(_)) => Err(ManifestError::GitAndPath),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::Manifest;

    const MAN_EMPTY: &str = "";
    const MAN_CORRUPTED: &str = "[defa";

    #[rstest]
    #[case(MAN_EMPTY)]
    #[should_panic]
    #[case(MAN_CORRUPTED)]
    fn test_parse(#[case] input: &str) {
        let _ = Manifest::parse(input).unwrap();
    }
}
