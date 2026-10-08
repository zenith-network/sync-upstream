use std::{
    collections::{BTreeMap, HashSet},
    fmt, fs,
    ops::{Add, AddAssign},
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use colored::Colorize as _;
use similar::{ChangeTag, TextDiff};

use crate::manifest::Capsule;

#[derive(Clone, Debug, Default)]
pub struct ChangeSet {
    changes: BTreeMap<PathBuf, Change>,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self {
            changes: BTreeMap::new(),
        }
    }

    pub fn changes(&self) -> &BTreeMap<PathBuf, Change> {
        &self.changes
    }

    pub fn collect(
        workspace_root: &Path,
        spec: &Capsule,
        current_files: &[PathBuf],
        source_files: &[PathBuf],
        source_dir: &Path,
    ) -> anyhow::Result<Self> {
        let new_files_set = source_files.iter().collect::<HashSet<_>>();
        let old_files_set = current_files.iter().collect::<HashSet<_>>();

        let added_files = new_files_set.difference(&old_files_set);
        let deleted_files = old_files_set.difference(&new_files_set);
        let modified_files = new_files_set.intersection(&old_files_set);

        let mut changeset = Self::new();

        for path in added_files {
            let full_path = source_dir.join(spec.src()).join(path);
            let content = fs::read_to_string(&full_path)
                .with_context(|| format!("failed to read '{}'", full_path.display()))?;
            changeset
                .changes
                .insert(path.to_path_buf(), Change::from_added_file(&content));
        }

        for path in deleted_files {
            let full_path = workspace_root.join(spec.dst()).join(path);
            let content = fs::read_to_string(&full_path)
                .with_context(|| format!("failed to read '{}'", full_path.display()))?;
            changeset
                .changes
                .insert(path.to_path_buf(), Change::from_deleted_file(&content));
        }

        for path in modified_files {
            let old_file_path = workspace_root.join(spec.dst()).join(path);
            let old_content = fs::read_to_string(&old_file_path)
                .with_context(|| format!("failed to read '{}'", old_file_path.display()))?;

            let new_file_path = source_dir.join(spec.src()).join(path);
            let new_content = fs::read_to_string(&new_file_path)
                .with_context(|| format!("failed to read '{}'", new_file_path.display()))?;

            let change = if old_content == new_content {
                Change::unchanged()
            } else {
                Change::from_modified_file(&old_content, &new_content)
            };

            changeset.changes.insert(path.to_path_buf(), change);
        }

        Ok(changeset)
    }

    /// Accumulated change of the whole changeset
    pub fn summary(&self) -> Change {
        self.changes
            .iter()
            .fold(Change::Unchanged, |acc, (_, change)| acc + *change)
    }

    pub fn to_strings_pretty(&self, verbose: bool) -> Vec<String> {
        let mut lines = Vec::new();
        for (path, change) in &self.changes {
            if change.is_unchanged() && !verbose {
                // skip unchanged files, if verbose is not set
                continue;
            }
            let path = path.display();
            lines.push(format!("{path} {change}"));
        }
        lines
    }
}

/// Change to a file in a capsule
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Change {
    /// Add new file
    Add {
        additions: Additions,
    },

    /// Delete old file
    Delete {
        deletions: Deletions,
    },

    /// Modify existing file
    Modify {
        additions: Additions,
        deletions: Deletions,
    },

    Unchanged,
}

impl Change {
    pub fn unchanged() -> Self {
        Self::Unchanged
    }

    pub fn from_added_file(file: &str) -> Self {
        Self::Add {
            additions: Additions(file.lines().count()),
        }
    }

    pub fn from_deleted_file(file: &str) -> Self {
        Self::Delete {
            deletions: Deletions(file.lines().count()),
        }
    }

    pub fn from_modified_file(old: &str, new: &str) -> Self {
        let diff = TextDiff::from_lines(old, new);

        let mut additions = Additions(0);
        let mut deletions = Deletions(0);

        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => {}
                ChangeTag::Delete => {
                    deletions += 1;
                }
                ChangeTag::Insert => {
                    additions += 1;
                }
            }
        }

        Self::Modify {
            additions,
            deletions,
        }
    }

    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    pub fn additions(&self) -> Additions {
        match self {
            Self::Add { additions } => *additions,
            Self::Delete { .. } | Self::Unchanged => Additions(0),
            Self::Modify { additions, .. } => *additions,
        }
    }

    pub fn deletions(&self) -> Deletions {
        match self {
            Self::Add { .. } | Self::Unchanged => Deletions(0),
            Self::Delete { deletions } => *deletions,
            Self::Modify { deletions, .. } => *deletions,
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let change = match self {
            Change::Add { .. } => "add".green(),
            Change::Delete { .. } => "delete".red(),
            Change::Modify { .. } => "modify".yellow(),
            Change::Unchanged => "unchanged".bright_white(),
        };
        let additions = self.additions();
        let deletions = self.deletions();

        if self.is_unchanged() {
            write!(f, "[{change}]")
        } else {
            write!(f, "[{change}] {additions:#} {deletions:#}")
        }
    }
}

impl Add for Change {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let self_additions = self.additions();
        let self_deletions = self.deletions();

        let rhs_additions = rhs.additions();
        let rhs_deletions = rhs.deletions();

        let additions = self_additions + rhs_additions;
        let deletions = self_deletions + rhs_deletions;

        match (additions, deletions) {
            (Additions(0), Deletions(0)) => Self::Unchanged,
            (Additions(0), _) => Self::Delete { deletions },
            (_, Deletions(0)) => Self::Add { additions },
            (_, _) => Self::Modify {
                additions,
                deletions,
            },
        }
    }
}

impl AddAssign for Change {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Additions(pub usize);

impl Add for Additions {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Add<usize> for Additions {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for Additions {
    fn add_assign(&mut self, rhs: usize) {
        self.0.add_assign(rhs);
    }
}

impl fmt::Display for Additions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}", format!("+{}", self.0).green())
        } else {
            self.0.fmt(f)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Deletions(pub usize);

impl Add for Deletions {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Add<usize> for Deletions {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for Deletions {
    fn add_assign(&mut self, rhs: usize) {
        self.0.add_assign(rhs);
    }
}

impl fmt::Display for Deletions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}", format!("-{}", self.0).red())
        } else {
            self.0.fmt(f)
        }
    }
}
