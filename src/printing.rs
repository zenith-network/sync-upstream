//! Output printing functions

use colored::Colorize as _;

use crate::{changeset::Change, config::Config, resolve::ResolvedCapsule};

/// Print diff stats per capsule
pub fn print_stats(capsules: &[ResolvedCapsule<'_>], config: &Config) {
    let mut is_up_to_date = true;
    for capsule in capsules {
        let capsule_name = capsule.spec.name();
        let capsule_dst = capsule.spec.dst().display();

        let summary = capsule.changeset.summary();
        // Skip capsules with no changes
        if summary.is_unchanged() && !config.verbose {
            continue;
        }

        let total_adds = summary.additions();
        let total_deletes = summary.deletions();
        let hdr = "Capsule".green().bold();
        let source = &capsule.locked.source;

        println!(
            "{hdr} {capsule_name} ('{capsule_dst}') -> {source} {total_adds:#} {total_deletes:#}"
        );

        for line in capsule.changeset.to_strings_pretty(config.verbose) {
            println!("  {}", line);
        }

        is_up_to_date = false;
    }

    if is_up_to_date {
        println!("{}", "Up to date".green().bold());
    }
}

/// Print total diff stats
pub fn print_final_stats(capsules: &[ResolvedCapsule<'_>]) {
    let total_change = capsules.iter().fold(Change::Unchanged, |acc, capsule| {
        acc + capsule.changeset.summary()
    });
    let added = total_change.additions();
    let deleted = total_change.deletions();
    println!(
        "{} ({added:#} {deleted:#})",
        "Changes written".green().bold()
    );
}
