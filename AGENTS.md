# AGENTS.md

This file provides guidance to AI agents when working with code in this repository.

## Rust Coding Guidelines

- Prioritize code correctness and clarity. Speed and efficiency are secondary priorities unless otherwise specified.
- Write doc comments which explains the idea of a type/trait/module/function, where the the idea is not clear enough from the code itself.
- Do not write organizational or comments that summarize the code. Comments should only be written in order to explain "why" the code is written in some way in the case there is a reason that is tricky / non-obvious.
- Prefer implementing functionality in existing files unless it is a new logical component. Avoid creating many small files.
- Prefer exhaustive `match` statements; avoid wildcard `_` arms so that new variants force a compile-time review.
- Prefer private modules with an explicitly exported public crate API over making everything `pub`.
- Do not create small helper methods that are referenced only once — inline them at the call site instead.
- Never create files with `mod.rs` paths - prefer `src/some_module.rs` instead of `src/some_module/mod.rs`.
- Use full words for variable names (no abbreviations like "q" for "queue")

## Module Size & Structure

- Prefer adding new modules instead of growing existing ones.
- Target Rust modules under ~500 LoC (excluding tests).
- If a file exceeds roughly 800 LoC, add new functionality in a new module instead of extending the existing file, unless there is a strong documented reason not to.
- When extracting code from a large module, move the related tests and module/type docs along with the implementation so invariants stay close to the code that owns them.
- Resist bloating any single "core"/catch-all crate. Before adding code there, consider whether an existing crate fits better, or whether it is time to introduce a new crate to the workspace and refactor accordingly.

## Build & Formatting Commands

- Always run `cargo fmt` after editing code
- Always run `cargo check` after editing code and completing all tasks
- Always run `cargo clippy` after  editing code and completing all tasks
- Always run `taplo fmt` and `taplo check` after editing TOML files

## Testing

- Prefer deep equality comparisons: `assert_eq!` on entire objects rather than field-by-field checks.
- Use `pretty_assertions::assert_eq` in tests for clearer diffs. Import it at the top of the test module if it isn't already.
- Avoid mutating process environment (`std::env::set_var`, etc.) inside tests; prefer passing environment-derived flags or dependencies in from above to keep tests isolated and parallel-safe.

## Documentation

- Update the docs docs under `docs/` when adding or changing user-facing features, configuration options, CLI behavior, etc. Do not over-explain internal implementation details there.
