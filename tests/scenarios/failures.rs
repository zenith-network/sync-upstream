use std::fs;

use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::fixtures::{Fixture, assert_files, file_tree, write};

/// A missing local remote fails before changing existing destination files.
#[test]
fn unavailable_local_remote() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "upstream\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    write(&fixture.workspace, "vendor/message.proto", "local\n");
    fs::rename(
        &fixture.upstream,
        fixture.root.path().join("unavailable-upstream"),
    )
    .unwrap();

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("failed to resolve capsule 'schemas'");

    assert_files(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "local\n")],
    );
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Missing branch, tag, and revision references fail without modifying destinations.
#[rstest]
#[case::branch("branch = 'missing'")]
#[case::tag("tag = 'missing'")]
#[case::revision("rev = '0000000000000000000000000000000000000001'")]
fn missing_git_reference(#[case] reference: &str) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "upstream\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(reference, "schemas = { src = 'schemas', dst = 'vendor' }");
    write(&fixture.workspace, "vendor/message.proto", "local\n");

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("failed to resolve capsule 'schemas'");

    assert_files(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "local\n")],
    );
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// A disappearing source directory fails for path and Git sources instead of deleting synced files.
#[rstest]
#[case::path(false)]
#[case::git(true)]
fn missing_source_directory(#[case] git_source: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "existing/file.txt", "upstream\n");
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    let capsules = "schemas = { src = 'schemas', dst = 'vendor' }";
    if git_source {
        fixture.commit(&fixture.upstream);
        fixture.git_manifest("branch = 'main'", capsules);
    } else {
        fixture.path_manifest(capsules);
    }
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    let before = file_tree(&fixture.workspace.join("vendor"));
    fs::remove_dir_all(fixture.upstream.join("schemas")).unwrap();
    if git_source {
        fixture.commit(&fixture.upstream);
    }

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("failed to resolve capsule 'schemas'");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
}

/// A later capsule's resolution failure prevents an earlier capsule from being written.
#[test]
fn resolution_failure_preserves_all_destinations() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/file.txt", "new\n");
    write(&fixture.workspace, "vendor/first/file.txt", "old\n");
    write(&fixture.workspace, "vendor/second/file.txt", "second\n");
    fixture.path_manifest(
        "first = { src = 'schemas', dst = 'vendor/first' }\n\
         second = { src = 'missing', dst = 'vendor/second' }",
    );
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("failed to resolve capsule 'second'");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
}

/// Two capsules changing the same destination file must fail before either writes it.
#[test]
fn conflicting_writes() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "first/file.txt", "first\n");
    write(&fixture.upstream, "second/file.txt", "second\n");
    write(&fixture.workspace, "vendor/file.txt", "local\n");
    fixture.path_manifest(
        "first = { src = 'first', dst = 'vendor' }\n\
         second = { src = 'second', dst = 'vendor' }",
    );
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("belongs to capsules 'first' and 'second'");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Shared ownership conflicts even if one capsule's copy matches the existing file.
#[test]
fn conflict_with_unchanged_file() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "first/file.txt", "local\n");
    write(&fixture.upstream, "second/file.txt", "second\n");
    write(&fixture.workspace, "vendor/file.txt", "local\n");
    fixture.path_manifest(
        "first = { src = 'first', dst = 'vendor' }\n\
         second = { src = 'second', dst = 'vendor' }",
    );
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("belongs to capsules 'first' and 'second'");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
}

/// A regular file blocking the destination's parent produces a write error.
#[test]
fn obstructed_destination() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/file.txt", "upstream\n");
    write(&fixture.workspace, "vendor", "blocking file\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor/schemas' }");

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("failed to create");

    assert_eq!(
        fs::read_to_string(fixture.workspace.join("vendor")).unwrap(),
        "blocking file\n"
    );
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Controlled non-UTF-8 files are rejected before synchronization, including stale destination files.
#[rstest]
#[case::source(false)]
#[case::destination(true)]
fn controlled_non_utf8_is_rejected(#[case] binary_destination: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.workspace, "vendor/message.proto", "local\n");
    let binary_root = if binary_destination {
        &fixture.workspace
    } else {
        &fixture.upstream
    };
    let binary_path = if binary_destination {
        "vendor/image.bin"
    } else {
        "schemas/image.bin"
    };
    write(binary_root, binary_path, [0xff, 0x00, 0xfe]);
    let filter = if binary_destination {
        "**/*.proto"
    } else {
        "**/*"
    };
    fixture.path_manifest(&format!(
        "schemas = {{ src = 'schemas', dst = 'vendor', filter = '{filter}' }}"
    ));
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .failure("did not contain valid UTF-8");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert!(!fixture.workspace.join("upstream.lock").exists());
}
