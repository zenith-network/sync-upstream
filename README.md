# sync-upstream

Synchronize vendored source trees from Git repositories, recording their sources
and commits in a lockfile for repeatable updates.

If you need to synchronize some files from some remote upstream (e.g. from GitHub),
but you don't want to clone full repository, this tool may help you. Think of it
as a `git sparse-checkout` on steroids.

## Prerequisites

`sync-upstream` runs Git as a subprocess. So Git executable, available in PATH is a runtime
dependency:

- Git >= v2.49

## Installation

Install directly from GitHub:

```sh
cargo install --git https://github.com/zenith-network/sync-upstream --tag v0.1.0 --locked
```

## Usage

Create `upstream.toml` in your project, replacing the example URL and paths with
your upstream repository and directories:

```toml
[default]
git = { url = "https://example.com/project.git", rev = "commit-sha" }

[capsules]
schemas = { src = "api/proto", dst = "vendor/proto" }
```

Capsule is just a bunch of files, controlled under a single name.
`sync-upstream` will manage `dst` directory, controlling its content.

You can see the diff of your local state with the upstream using `--dry-run`:

```sh
sync-upstream --dry-run --all
```

To sync capsules content run:

```sh
sync-upstream --all
```

> NOTE: For safety, by default update can be performed on a clean Git tree.
> To ignore this limitation use `--overwrite` flag.

See [docs](docs/README.md) for more information.

Also see [examples](examples/example-project/README.md).

## Limitations

- Only UTF-8 text files are currently supported.
- Symlinks in sources are currently not supported.
- Lockfile verification is not implemented yet.
