use std::path::{Path, PathBuf};

use globset::GlobMatcher;
use walkdir::WalkDir;

/// Collect all files from directory matching the filter.
///
/// Returned paths are relative to `path`.
pub fn collect_files(
    path: impl AsRef<Path>,
    matcher: Option<&GlobMatcher>,
) -> anyhow::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let files_iter = WalkDir::new(path);

    for entry_result in files_iter {
        let entry = entry_result?;

        let file_type = entry.file_type();
        // For now we ignore symlinks
        if file_type.is_file() {
            // Always ignore marker files
            let filename = entry.file_name();
            if filename == ".sync_upstream" {
                continue;
            }

            let relpath = entry.path().strip_prefix(path)?;

            if matcher.is_none_or(|m| m.is_match(relpath)) {
                files.push(relpath.to_path_buf());
            }
        }
    }

    Ok(files)
}
