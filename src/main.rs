use anyhow::{Context as _, anyhow, bail};
use clap::Parser;

mod artifacts;
mod changeset;
mod cli;
mod config;
mod files;
mod git;
mod git_manager;
mod lockfile;
mod lockfile_table;
mod manifest;
mod manifest_table;
mod printing;
mod resolve;
mod write;

use crate::{
    artifacts::Artifacts,
    cli::Cli,
    config::Config,
    git::Git,
    git_manager::GitManager,
    lockfile::{LOCKFILE_NAME, Lockfile},
    manifest::{Capsule, Manifest},
    resolve::ResolvedCapsule,
};

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let config = Config::from_args(args)?;

    let manifest =
        Manifest::from_file(&config.manifest_path).context("failed to read manifest file")?;

    let lockfile_path = config.workspace_root.join(LOCKFILE_NAME);
    if lockfile_path.exists() {
        // TODO: for further use
        let _lockfile =
            Lockfile::from_file(lockfile_path).context("failed to read upstream.lock")?;
    }

    let selected_capsules = select_capsules(&config, &manifest)?;

    // Do not drop artifacts till the end, or temp artifacts may be lost
    let artifacts = Artifacts::from_config(&config)?;

    let git_manager = GitManager::new(artifacts.dir().to_path_buf(), config.verbose);

    // Check writable before resolving capsules, because the artifacts created during resolving
    // may make the git tree dirty (e.g. if target/ is not in .gitignore).
    // Note that this only solves the case when the tool has just created files, which made the
    // tree dirty. On repeated runs user has to deal with this himself.
    if !config.dry_run && !is_workspace_writable(&config)? {
        bail!("uncommitted changes in the workspace, write requires --overwrite");
    }

    let capsules = selected_capsules
        .iter()
        .map(|spec| {
            ResolvedCapsule::resolve(spec, &git_manager, &config)
                .with_context(|| format!("failed to resolve capsule '{}'", spec.name()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    resolve::check_overlapping(&capsules, &config)?;

    printing::print_stats(&capsules, &config);

    if config.dry_run {
        return Ok(());
    }

    write::write_files(&capsules, &config)?;
    write::write_lockfile(&capsules, &config)?;

    printing::print_final_stats(&capsules);

    Ok(())
}

/// Resolve capsules selection
fn select_capsules(config: &Config, manifest: &Manifest) -> anyhow::Result<Vec<Capsule>> {
    if let Some(names) = &config.selected_capsules {
        names
            .iter()
            .map(|name| {
                manifest
                    .capsules()
                    .iter()
                    .find_map(|capsule| {
                        if capsule.name() == name {
                            Some(capsule.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| anyhow!("capsule '{name}' not found"))
            })
            .collect::<Result<Vec<_>, _>>()
    } else {
        Ok(manifest.capsules().to_vec())
    }
}

/// Workspace is writable by default, if it's a clean git repo.
/// Otherwise `--overwrite` flag is required.
fn is_workspace_writable(config: &Config) -> anyhow::Result<bool> {
    if config.overwrite {
        Ok(true)
    } else {
        let git = Git::new_in(config.workspace_root.clone(), false);
        if git.is_inside_work_tree() {
            let status = git
                .status()
                .args([
                    "--porcelain=v1",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                ])
                .env("GIT_OPTIONAL_LOCKS", "0")
                .run()
                .context("failed to check Git workspace status")?;
            Ok(status.stdout().is_empty())
        } else {
            Ok(false)
        }
    }
}
