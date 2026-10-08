use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::fixtures::{Fixture, file_tree};

#[derive(Debug, PartialEq, Eq)]
/// Comparable cache snapshot containing Git identity and payload files, without Git metadata.
pub struct Checkout {
    pub path: PathBuf,
    pub remote: String,
    pub commit: String,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

/// Find cached working trees by their `.git` directories, without assuming URL-derived names.
/// Results are sorted so filesystem traversal order cannot affect assertions.
pub fn checkout_paths(target: &Path) -> Vec<PathBuf> {
    if !target.exists() {
        return Vec::new();
    }
    let mut paths = WalkDir::new(target)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git")
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_dir() && entry.path().join(".git").is_dir())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Snapshot each checkout, asking Git for its origin URL and current commit.
/// Payload comparisons exclude internal Git files that can change during ordinary commands.
pub fn cache_state(fixture: &Fixture, target: &Path) -> Vec<Checkout> {
    checkout_paths(target)
        .into_iter()
        .map(|path| Checkout {
            remote: fixture.git(&path, &["remote", "get-url", "origin"]),
            commit: fixture.git(&path, &["rev-parse", "HEAD"]),
            files: file_tree(&path),
            path,
        })
        .collect()
}
