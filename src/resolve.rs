//! This module defines how capsules are resolved:
//!
//! - git resolving
//! - paths resolving
//! - changeset building

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use url::Url;

use crate::{
    changeset::ChangeSet,
    config::Config,
    files::collect_files,
    git_manager::GitManager,
    lockfile::LockedCapsule,
    manifest::{Capsule, GitRef, Source},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolvedSource {
    Git {
        url: Url,
        git_ref: GitRef,
        commit: String,
    },
    Path(PathBuf),
}

impl ResolvedSource {
    /// Resolve source from source specification.
    ///
    /// This doesn't ensure that source files are present (see [`Self::ensure_sources`]).
    fn resolve(source: &Source, git_manager: &GitManager) -> anyhow::Result<Self> {
        match source {
            Source::Git(git_source) => {
                // TODO: Reference resolution could be cached, so that we won't re-run
                //       git ls-remote for the same tags/branches multiple time.
                let commit = git_manager.resolve_ref(git_source)?;
                Ok(ResolvedSource::Git {
                    url: git_source.url().clone(),
                    git_ref: git_source.git_ref().clone(),
                    commit,
                })
            }
            Source::Path(path) => Ok(ResolvedSource::Path(path.clone())),
        }
    }

    /// Ensure source files are present.
    ///
    /// On success returns path to the source directory.
    /// Capsule files will be under `SOURCE_DIR/src`.
    fn ensure_sources(&self, src: &Path, git_manager: &GitManager) -> anyhow::Result<PathBuf> {
        let source_dir = match self {
            ResolvedSource::Git { url, commit, .. } => {
                let checked_path = git_manager.ensure_checkout(url, commit)?;
                git_manager.ensure_sparse_path(url, commit, src)?;
                checked_path
            }
            ResolvedSource::Path(path) => path.clone(),
        };

        let src_path = source_dir.join(src);
        if !src_path.exists() {
            bail!("source path '{}' doesn't exist", src_path.display());
        }

        Ok(source_dir)
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedCapsule<'a> {
    /// Reference to original capsule specification
    pub spec: &'a Capsule,

    /// Directory where the source files are stored: `ARTIFACTS_DIR/CAPSULE_SRC`
    pub source_dir: PathBuf,

    /// Changes in the capsule
    pub changeset: ChangeSet,

    /// Locked capsule
    pub locked: LockedCapsule,
}

impl<'a> ResolvedCapsule<'a> {
    pub fn resolve(
        spec: &'a Capsule,
        git_manager: &GitManager,
        config: &Config,
    ) -> anyhow::Result<Self> {
        let resolved_source = ResolvedSource::resolve(spec.source(), git_manager)
            .context("failed to resolve source")?;
        let source_dir = resolved_source
            .ensure_sources(spec.src(), git_manager)
            .context("failed to find sources")?;

        let matcher = spec.filter().map(|filter| filter.compile_matcher());

        let capsule_source_dir = source_dir.join(spec.src());
        let source_files = collect_files(&capsule_source_dir, matcher.as_ref())?;

        let capsule_dir = config.workspace_root.join(spec.dst());
        // Note: we control the whole dst directory, so no filter is applied here
        let current_files = collect_files(&capsule_dir, None)?;

        let changeset = ChangeSet::collect(
            &config.workspace_root,
            spec,
            &current_files,
            &source_files,
            &source_dir,
        )
        .context("failed to collect changes")?;

        let locked = LockedCapsule::from_resolved(resolved_source, spec);

        Ok(Self {
            spec,
            source_dir,
            changeset,
            locked,
        })
    }
}

/// Detect overlapping of dst paths in capsules, ensuring that we won't write same file twice
pub fn check_overlapping(capsules: &[ResolvedCapsule<'_>], config: &Config) -> anyhow::Result<()> {
    let mut path_owners = HashMap::new();

    for capsule in capsules {
        let destination = config.workspace_root.join(capsule.spec.dst());

        for path in capsule.changeset.changes().keys() {
            let full_path = destination.join(path);
            if let Some(other_capsule) = path_owners.get(&full_path) {
                bail!(
                    "path '{}' belongs to capsules '{}' and '{}' at the same time",
                    full_path.display(),
                    other_capsule,
                    capsule.spec.name(),
                );
            }
            path_owners.insert(full_path, capsule.spec.name());
        }
    }

    Ok(())
}
