//! This module defines updating files.

use std::{fs, path::Path};

use anyhow::Context as _;

use crate::{
    changeset::Change,
    config::Config,
    lockfile::{LOCKFILE_NAME, Lockfile},
    resolve::ResolvedCapsule,
};

pub fn write_lockfile(capsules: &[ResolvedCapsule<'_>], config: &Config) -> anyhow::Result<()> {
    let lockfile = Lockfile {
        capsules: capsules
            .iter()
            .map(|capsule| capsule.locked.clone())
            .collect(),
    };

    let lockfile_path = config.workspace_root.join(LOCKFILE_NAME);
    lockfile
        .write_to_file(&lockfile_path)
        .context("failed to write upstream.lock")?;
    Ok(())
}

pub fn write_files(capsules: &[ResolvedCapsule<'_>], config: &Config) -> anyhow::Result<()> {
    capsules.iter().try_for_each(|capsule| {
        let source_path = capsule.source_dir.join(capsule.spec.src());
        let dst_path = config.workspace_root.join(capsule.spec.dst());

        // We store marker file in each tracked dst directory for visibility
        write_marker_file(&dst_path, capsule.spec.name())?;

        capsule
            .changeset
            .changes()
            .iter()
            .try_for_each(|(path, change)| write_change(&dst_path, &source_path, path, change))
            .with_context(|| {
                format!(
                    "failed to write changes of capsule '{}'",
                    capsule.spec.name()
                )
            })
    })
}

fn write_change(
    capsule_path: &Path,
    capsule_source_path: &Path,
    path: &Path,
    change: &Change,
) -> anyhow::Result<()> {
    match change {
        Change::Add { .. } | Change::Modify { .. } => {
            let src_path = capsule_source_path.join(path);
            let dst_path = capsule_path.join(path);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create '{}'", parent.display()))?;
            }
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy file '{}' -> '{}'",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
        Change::Delete { .. } => {
            let dst_path = capsule_path.join(path);
            fs::remove_file(&dst_path)
                .with_context(|| format!("failed to remove file '{}'", dst_path.display()))?;
        }
        Change::Unchanged => {
            // do nothing
        }
    }
    Ok(())
}

const MARKER_CONTENT: &str = "This directory is controlled by sync-upstream\n";

fn write_marker_file(capsule_path: &Path, capsule_name: &str) -> anyhow::Result<()> {
    // We store marker file in each tracked dst directory for visibility
    fs::create_dir_all(capsule_path)
        .with_context(|| format!("failed to create '{}'", capsule_path.display()))?;
    let marker_path = capsule_path.join(".sync_upstream");
    fs::write(&marker_path, MARKER_CONTENT)
        .with_context(|| format!("failed to write marker file in {capsule_name}"))?;
    Ok(())
}
