# Documentation

- [Manifest file docs](manifest.md)

## Dry-run output

`sync-upstream --dry-run --all` lists pending file additions and deletions,
including empty files whose line counts are both zero. It reports `Up to date`
when the selected capsules have no pending file or content changes.
