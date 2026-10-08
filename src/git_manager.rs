use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, anyhow, bail};
use url::Url;

use crate::{
    git::Git,
    manifest::{GitRef, GitSource},
};

/// Parse the table produced by `git ls-remote` into vector pairs (OBJECT_ID, REF_NAME)
fn parse_ls_remote_output(stdout: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut res = Vec::new();
    for line in stdout.lines() {
        let (obj_id, ref_name) = line
            .split_once('\t')
            .ok_or_else(|| anyhow!("unexpected format of git ls-remote output: TAB not found"))?;
        res.push((obj_id.to_owned(), ref_name.to_owned()));
    }
    Ok(res)
}

#[derive(Clone, Debug)]
pub struct GitManager {
    workdir: PathBuf,
    verbose: bool,
}

impl GitManager {
    pub fn new(workdir: PathBuf, verbose: bool) -> Self {
        Self { workdir, verbose }
    }

    /// Resolve git reference to a concrete commit.
    ///
    /// Returns commit hash.
    ///
    /// This doesn't check that commit actually exists in the repo.
    pub fn resolve_ref(&self, source: &GitSource) -> anyhow::Result<String> {
        let git = Git::new(self.verbose);

        match source.git_ref() {
            // Explicit commits are returned as they are
            GitRef::Rev(commit_sha) => Ok(commit_sha.clone()),

            GitRef::Tag(tag_name) => {
                let tag_ref = format!("refs/tags/{tag_name}");
                let annotated_tag_ref = format!("refs/tags/{tag_name}^{{}}");
                let output = git
                    .ls_remote()
                    .arg("--exit-code")
                    .arg(source.url().to_string())
                    .args([&tag_ref, &annotated_tag_ref])
                    .run()?;
                let objs = parse_ls_remote_output(output.stdout())?;

                // Check annotated tag ref first, to get the actual commit ID,
                // not the object ID of the tag.
                if let Some(commit) = objs.iter().find_map(|(obj_id, ref_name)| {
                    // Inputs are patterns, and we're looking for exact match.
                    if ref_name == &annotated_tag_ref {
                        Some(obj_id)
                    } else {
                        None
                    }
                }) {
                    Ok(commit.clone())
                } else {
                    // Now check the regular tags
                    objs.into_iter()
                        .find_map(|(obj_id, ref_name)| {
                            // Inputs are patterns, and we're looking for exact match.
                            if ref_name == tag_ref {
                                Some(obj_id)
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| anyhow!("tag '{tag_name}' not found among tags"))
                }
            }

            GitRef::Branch(branch_name) => {
                let branch_ref = format!("refs/heads/{branch_name}");
                let output = git
                    .ls_remote()
                    .arg("--exit-code")
                    .arg(source.url().to_string())
                    .arg(&branch_ref)
                    .run()?;
                parse_ls_remote_output(output.stdout())?
                    .into_iter()
                    .find_map(|(obj_id, ref_name)| {
                        // branch_ref input is a pattern, not an exact match,
                        // and we're looking for exact match
                        if ref_name == branch_ref {
                            Some(obj_id)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| anyhow!("branch '{branch_name}' not found among heads"))
            }
        }
    }

    /// Returns directory with repo set to `commit`
    pub fn ensure_checkout(&self, url: &Url, commit: &str) -> anyhow::Result<PathBuf> {
        let checkout_dir = self.workdir.join(encode_url(url)).join(commit);

        let git = Git::new_in(checkout_dir.clone(), self.verbose);

        if !git.is_repository() {
            // git clone with "-C" requires directory to exist
            fs::create_dir_all(&checkout_dir)
                .with_context(|| format!("failed to create '{}'", checkout_dir.display()))?;

            git.clone_()
                .args(["--depth=1", "--filter=blob:none", "--sparse"])
                .arg(format!("--revision={commit}"))
                .arg(url.to_string())
                .arg(".")
                .run()?;
        }

        // Ensuring given commit is checked out
        git.reset().arg("--hard").arg(commit).run()?;

        Ok(checkout_dir)
    }

    pub fn ensure_sparse_path(&self, url: &Url, commit: &str, path: &Path) -> anyhow::Result<()> {
        let checkout_dir = self.workdir.join(encode_url(url)).join(commit);

        let git = Git::new_in(checkout_dir, self.verbose);
        if !git.is_repository() {
            bail!("No checkout found for {}?rev={}", url, commit);
        }

        git.sparse_checkout().arg("add").arg(path).run()?;
        Ok(())
    }
}

fn encode_url(url: &Url) -> String {
    // this keeps name human-readable
    slug::slugify(url)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{Url, encode_url};

    #[rstest]
    #[case::example("https://example.com", "https-example-com")]
    fn test_url_encoding(#[case] input: &str, #[case] expected: &str) {
        let url = Url::parse(input).unwrap();
        let encoded = encode_url(&url);
        assert_eq!(encoded, expected);
    }
}
