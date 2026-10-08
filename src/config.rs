use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, anyhow, ensure};

use crate::cli::Cli;

const DEFAULT_TARGET_DIR: &str = "target";

/// Configuration of the run
#[derive(Clone, Debug)]
pub struct Config {
    /// If `None`, then all capsules must be updated.
    /// If `Some(_)`, then guaranteed to be non-empty.
    /// There is nothing conceptually wrong with passing the same capsule twice in the arguments,
    /// so this collection omits duplicates by collecting them into a set.
    pub selected_capsules: Option<BTreeSet<String>>,

    pub manifest_path: PathBuf,

    /// Directory which contains `upstream.toml`. Derived from `manifest_path`.
    pub workspace_root: PathBuf,

    /// Collect change sets, but don't write files or lockfile.
    pub dry_run: bool,

    /// Allow overwriting files, when it's unsafe (e.g. in dirty git tree)
    pub overwrite: bool,

    /// Directory to store artifacts, defaults to `WORKSPACE_ROOT/target`.
    pub target_dir: PathBuf,

    /// No artifacts need to be preserved.
    pub no_cache: bool,

    pub verbose: bool,
}

impl Config {
    pub fn from_args(args: Cli) -> anyhow::Result<Self> {
        let workspace_root = Self::workspace_root_from_manifest(&args.manifest_path)?;

        let target_dir = args
            .target_dir
            .unwrap_or_else(|| workspace_root.join(DEFAULT_TARGET_DIR));

        let selected_capsules = if args.workspace {
            None
        } else {
            ensure!(
                !args.capsules.is_empty(),
                "No capsules specified, use -c or --workspace"
            );
            Some(args.capsules.into_iter().collect())
        };

        Ok(Self {
            selected_capsules,
            manifest_path: args.manifest_path,
            workspace_root,
            dry_run: args.dry_run,
            overwrite: args.overwrite,
            target_dir,
            no_cache: args.no_cache,
            verbose: args.verbose,
        })
    }

    /// Returns path to workspace root - a directory containing `upstream.toml`.
    ///
    /// If manifest path is a single-component path (e.g. `"upstream.toml"`), this returns empty path.
    /// If manifest file doesn't exist, this returns error.
    fn workspace_root_from_manifest(manifest_path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
        let manifest_path = manifest_path.as_ref();
        Ok(manifest_path
            .canonicalize()
            .with_context(|| {
                format!(
                    "failed to canonicalize manifest path '{}'",
                    manifest_path.display()
                )
            })?
            .parent()
            .ok_or_else(|| anyhow!("workspace root not found"))?
            .to_path_buf())
    }
}
