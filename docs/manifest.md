# Manifest file

Manifest file is `upstream.toml` by default.

```toml
# Placement of upstream.toml defined the directory of sync-upstream-managed directory,
# similar to how Cargo.toml defines the Cargo package.
# Directory with upstream.toml is considered a root of workspace for sync-upstream.
# Destination paths of capsules are relative to the workspace root.
# Any upstream.toml in the subdirectories of the workspace root is ignored by this workspace.

# You can specify default source and filter for your capsules:
[default]
git = {
    # URL of source repository
    url = "https://github.com/org/repo"

    # You can place one of these here:
    rev = "<commit_hash>"
    # or
    tag = "<tag_name>"
    # or
    branch = "<branch_name>"
    # This is similar to how Cargo.toml allows to specify git dependencies
}
# or
path = "/path/to/local/repo"
# Both "git" and "path" at the same time are not allowed.

# Optional default filter for source files
# Unix-style glob syntax is used, applied to path, relative to capsule's src.
filter = "**/*.proto"

# "Capsule" means a bunch of files (directory subtree).
# We deliberately don't want to use word "package" to avoid confusion with Cargo packages.
# Capsule can be named however you want. This name only affects how sync-upstream
# identifies this set of files.
[capsules]

# Capsule name
my-capsule = {
    # Relative path to source repository root.
    src = "source/path",

    # Relative path to the workspace root.
    dst = "this/workspace/path",

    # You can specify the source for each individual capsule.
    git = {
        url = "https://github.com/org/repo"

        # If "url" is used then it's REQUIRED to place one of these here:
        rev = "<commit_hash>"
        # or
        tag = "<tag_name>"
        # or
        branch = "<branch_name>"
        # Note that these options cannot be taken from [default] - if you overwrite the default URL,
        # you have to specify the reference here.
        # This is similar to how Cargo.toml allows to specify git dependencies
    }

    # or
    path = "/path/to/local/repo"
    # Both "git" and "path" at the same time are not allowed.
    # If none of these specified, value from [default] will be used.

    # Optional filter for source files in this capsule.
    # Overwrites default filter.
    filter = "**/*.{proto,yml}"
}

# ... other capsules
```
