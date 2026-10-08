use std::path::PathBuf;

use clap::Parser;

#[derive(Clone, Debug, Parser)]
#[command(
    about = "Synchronize vendored source trees from Git repositories",
    long_about = None,
    version,
    styles = clap_cargo::style::CLAP_STYLING
)]
pub struct Cli {
    /// Path to upstream.toml.
    #[arg(
        short = 'm',
        long,
        default_value = "upstream.toml",
        value_name = "PATH"
    )]
    pub manifest_path: PathBuf,

    /// Capsule to update.
    #[arg(short = 'c', long = "capsule", value_name = "NAME")]
    pub capsules: Vec<String>,

    /// Update all capsules.
    #[arg(long, visible_alias = "all", conflicts_with = "capsules")]
    pub workspace: bool,

    /// Report expected diff without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Allow overwriting content.
    #[arg(long)]
    pub overwrite: bool,

    /// Verbose output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Directory for cache artifacts (default: WORKSPACE_ROOT/target).
    #[arg(long, value_name = "DIRECTORY", help_heading = "Caching")]
    pub target_dir: Option<PathBuf>,

    /// Disable caching remote repositories into target directory.
    #[arg(long, help_heading = "Caching")]
    pub no_cache: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn command_debug_assert() {
        Cli::command().debug_assert();
    }
}
