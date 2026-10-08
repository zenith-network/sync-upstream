use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;

use crate::config::Config;

/// Controls how artifacts are stored during run
#[derive(Debug)]
pub enum Artifacts {
    /// Artifacts are cached in target directory
    Cached(CachedArtifacts),

    /// Artifacts are cleaned on drop
    Temp(TempArtifacts),
}

impl Artifacts {
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        if config.no_cache {
            Ok(Self::Temp(TempArtifacts::new()?))
        } else {
            Ok(Self::Cached(CachedArtifacts::open(&config.target_dir)?))
        }
    }

    /// Path to directory where to store artifacts
    pub fn dir(&self) -> &Path {
        match self {
            Artifacts::Cached(cached_artifacts) => cached_artifacts.dir(),
            Artifacts::Temp(temp_artifacts) => temp_artifacts.dir(),
        }
    }
}

const ARTIFACTS_DIR_NAME: &str = "sync-upstream";

#[derive(Clone, Debug)]
pub struct CachedArtifacts {
    path: PathBuf,
}

impl CachedArtifacts {
    pub fn open(target_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = target_dir.as_ref().join(ARTIFACTS_DIR_NAME);
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create cached directory '{}'", path.display()))?;
        Ok(Self { path })
    }

    pub fn dir(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
pub struct TempArtifacts {
    temp: tempfile::TempDir,
}

impl TempArtifacts {
    pub fn new() -> anyhow::Result<Self> {
        let temp = tempfile::TempDir::new().context("failed to create temp directory")?;
        Ok(Self { temp })
    }

    pub fn dir(&self) -> &Path {
        self.temp.path()
    }
}
