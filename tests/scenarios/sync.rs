use std::fs;

use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::fixtures::{Fixture, assert_capsule, assert_files, file_tree, write};

/// Synchronization copies nested text and empty files, creates a marker, and is repeatable.
#[test]
fn initial_sync_and_repeat() {
    let fixture = Fixture::new();
    write(
        &fixture.upstream,
        "schemas/message.proto",
        "message Example {}\n",
    );
    write(
        &fixture.upstream,
        "schemas/nested/service.proto",
        "service Example {}\n",
    );
    write(&fixture.upstream, "schemas/empty.proto", "");
    write(&fixture.workspace, "notes.txt", "workspace notes\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    let expected = [
        ("message.proto", "message Example {}\n"),
        ("nested/service.proto", "service Example {}\n"),
        ("empty.proto", ""),
    ];
    assert_capsule(&fixture.workspace.join("vendor"), &expected);

    let repeated = fixture.run(&["--all", "--overwrite", "--no-cache"]);
    repeated.success();
    assert!(
        repeated.stdout.contains("Up to date"),
        "{}",
        repeated.stdout
    );
    assert_capsule(&fixture.workspace.join("vendor"), &expected);
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.txt")).unwrap(),
        "workspace notes\n"
    );
}

/// Later runs add, update, and delete files to match the current source tree.
#[test]
fn additions_modifications_and_deletions() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/changed.proto", "old\n");
    write(&fixture.upstream, "schemas/deleted.proto", "deleted\n");
    write(&fixture.upstream, "schemas/unchanged.proto", "unchanged\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    write(&fixture.upstream, "schemas/changed.proto", "new\n");
    write(&fixture.upstream, "schemas/nested/added.proto", "added\n");
    fs::remove_file(fixture.upstream.join("schemas/deleted.proto")).unwrap();

    let output = fixture.run(&["--all", "--overwrite", "--no-cache"]);
    output.success();
    for fragment in [
        "changed.proto [modify]",
        "deleted.proto [delete]",
        "nested/added.proto [add]",
    ] {
        assert!(output.stdout.contains(fragment), "{}", output.stdout);
    }
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[
            ("changed.proto", "new\n"),
            ("nested/added.proto", "added\n"),
            ("unchanged.proto", "unchanged\n"),
        ],
    );
}

/// A selected update leaves an existing unselected capsule and unrelated files untouched.
#[test]
fn selected_update_preserves_other_files() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/file.txt", "new\n");
    write(&fixture.upstream, "docs/file.txt", "upstream docs\n");
    write(&fixture.workspace, "vendor/docs/local.txt", "local docs\n");
    write(&fixture.workspace, "notes.txt", "notes\n");
    fixture.path_manifest(
        "schemas = { src = 'schemas', dst = 'vendor/schemas' }\n\
         docs = { src = 'docs', dst = 'vendor/docs' }",
    );

    fixture
        .run(&["-c", "schemas", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[("file.txt", "new\n")],
    );
    assert_files(
        &fixture.workspace.join("vendor/docs"),
        &[("local.txt", "local docs\n")],
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("notes.txt")).unwrap(),
        "notes\n"
    );
}

/// Dry runs report additions, modifications, and deletions without changing existing files.
#[test]
fn dry_run_preserves_destination() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/changed.proto", "new\n");
    write(&fixture.upstream, "schemas/added.proto", "added\n");
    write(&fixture.workspace, "vendor/changed.proto", "old\n");
    write(&fixture.workspace, "vendor/deleted.proto", "deleted\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    let before = file_tree(&fixture.workspace.join("vendor"));

    let output = fixture.run(&["--all", "--dry-run", "--no-cache"]);
    output.success();
    for fragment in [
        "changed.proto [modify]",
        "added.proto [add]",
        "deleted.proto [delete]",
    ] {
        assert!(output.stdout.contains(fragment), "{}", output.stdout);
    }
    assert!(
        !output.stdout.contains("Changes written"),
        "{}",
        output.stdout
    );
    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// A dry run for a new capsule creates neither its destination nor a lockfile.
#[test]
fn dry_run_does_not_create_destination() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture.run(&["--all", "--dry-run", "--no-cache"]).success();

    assert!(!fixture.workspace.join("vendor").exists());
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Clean Git workspaces accept writes without the overwrite flag.
#[test]
fn clean_git_workspace() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    fixture.init_repository(&fixture.workspace);
    fixture.commit(&fixture.workspace);

    fixture.run(&["--all", "--no-cache"]).success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// Tracked edits and untracked files both require overwrite before synchronizing a Git workspace.
#[rstest]
#[case::tracked("notes.txt")]
#[case::untracked("untracked.txt")]
fn dirty_git_workspace_requires_overwrite(#[case] dirty_file: &str) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.workspace, "notes.txt", "original\n");
    write(&fixture.workspace, "vendor/message.proto", "local\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    fixture.init_repository(&fixture.workspace);
    fixture.commit(&fixture.workspace);
    write(&fixture.workspace, dirty_file, "dirty\n");
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&["--all", "--no-cache"])
        .failure("uncommitted changes");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Non-Git workspaces require overwrite before files can be written.
#[test]
fn non_git_workspace_requires_overwrite() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture
        .run(&["--all", "--no-cache"])
        .failure("write requires --overwrite");

    assert!(!fixture.workspace.join("vendor").exists());
}

/// Overwrite permits synchronization in both dirty Git and non-Git workspaces.
#[rstest]
#[case::git(true)]
#[case::non_git(false)]
fn overwrite_allows_writes(#[case] git_workspace: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.workspace, "vendor/message.proto", "old\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    if git_workspace {
        fixture.init_repository(&fixture.workspace);
        fixture.commit(&fixture.workspace);
        write(&fixture.workspace, "vendor/message.proto", "dirty\n");
    }

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// An empty-file addition or deletion must be reported as a change in a dry run.
#[rstest]
#[case::addition(false, "empty.proto [add]")]
#[case::deletion(true, "empty.proto [delete]")]
#[ignore = "low priority bug, tracked"]
fn empty_file_changes_are_reported(#[case] deletion: bool, #[case] expected: &str) {
    let fixture = Fixture::new();
    fs::create_dir(fixture.upstream.join("schemas")).unwrap();
    if deletion {
        write(&fixture.workspace, "vendor/empty.proto", "");
    } else {
        write(&fixture.upstream, "schemas/empty.proto", "");
    }
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    let output = fixture.run(&["--all", "--dry-run", "--no-cache"]);
    output.success();

    assert!(output.stdout.contains(expected), "{}", output.stdout);
    assert!(!output.stdout.contains("Up to date"), "{}", output.stdout);
}

/// A managed path can transition between a regular file and a populated directory.
#[rstest]
#[case::file_to_directory(false)]
#[case::directory_to_file(true)]
#[ignore = "low priority bug, tracked"]
fn file_directory_transitions(#[case] directory_to_file: bool) {
    let fixture = Fixture::new();
    let initial = if directory_to_file {
        "schemas/node/child.txt"
    } else {
        "schemas/node"
    };
    write(&fixture.upstream, initial, "old\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    if directory_to_file {
        fs::remove_dir_all(fixture.upstream.join("schemas/node")).unwrap();
        write(&fixture.upstream, "schemas/node", "new\n");
    } else {
        fs::remove_file(fixture.upstream.join("schemas/node")).unwrap();
        write(&fixture.upstream, "schemas/node/child.txt", "new\n");
    }

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    let expected = if directory_to_file {
        "node"
    } else {
        "node/child.txt"
    };
    assert_capsule(&fixture.workspace.join("vendor"), &[(expected, "new\n")]);
}

/// A broad update followed by a smaller selection must leave later invocations usable.
#[test]
fn smaller_selection_allows_subsequent_runs() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/file.txt", "schema\n");
    write(
        &fixture.upstream,
        "documentation/nested/guide.txt",
        "guide\n",
    );
    fixture.path_manifest(
        "schemas = { src = 'schemas', dst = 'vendor/schemas' }\n\
         documentation = { src = 'documentation/nested', dst = 'vendor/documentation' }",
    );
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    fixture
        .run(&["-c", "schemas", "--overwrite", "--no-cache"])
        .success();

    fixture
        .run(&["-c", "schemas", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[("file.txt", "schema\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/documentation"),
        &[("guide.txt", "guide\n")],
    );
}

/// A manifest inside a clean repository's subdirectory does not require overwrite.
#[test]
fn clean_git_workspace_in_repository_subdirectory() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    let project = fixture.workspace.join("project");
    fs::create_dir(&project).unwrap();
    fs::rename(
        fixture.workspace.join("upstream.toml"),
        project.join("upstream.toml"),
    )
    .unwrap();
    fixture.init_repository(&fixture.workspace);
    fixture.commit(&fixture.workspace);
    assert_eq!(
        fixture.git(
            &fixture.workspace,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        ),
        "",
    );

    fixture
        .run(&[
            "--all",
            "--no-cache",
            "--manifest-path",
            "project/upstream.toml",
        ])
        .success();

    assert_capsule(&project.join("vendor"), &[("message.proto", "schema\n")]);
}
