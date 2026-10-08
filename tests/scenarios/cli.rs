use std::fs;

use rstest::rstest;

use crate::fixtures::{Fixture, RunOutput, assert_capsule, assert_files, write};

/// Help and version can be requested without a manifest or a workspace.
#[rstest]
#[case::help("--help", "Synchronize vendored source trees")]
#[case::version("--version", concat!("sync-upstream ", env!("CARGO_PKG_VERSION")))]
fn informational_flags(#[case] flag: &str, #[case] expected: &str) {
    let fixture = Fixture::new();
    let output = fixture.run(&[flag]);

    output.success();
    assert!(output.stdout.contains(expected), "{}", output.stdout);
    assert_files(&fixture.workspace, &[]);
}

/// A sync invocation must explicitly select a capsule or the whole workspace.
#[test]
fn selection_is_required() {
    let fixture = Fixture::new();
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture.run(&[]).failure("No capsules specified");
    assert!(!fixture.workspace.join("vendor").exists());
}

/// Selecting a nonexistent capsule produces a diagnostic without writing destinations.
#[test]
fn unknown_capsule() {
    let fixture = Fixture::new();
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture
        .run(&["-c", "missing"])
        .failure("capsule 'missing' not found");
    assert!(!fixture.workspace.join("vendor").exists());
}

/// Whole-workspace selection cannot be combined with an individual capsule selection.
#[test]
fn conflicting_selection_flags() {
    let fixture = Fixture::new();

    fixture
        .run(&["--workspace", "-c", "schemas"])
        .failure("cannot be used with");
    assert_files(&fixture.workspace, &[]);
}

/// Both whole-workspace spellings synchronize every configured capsule.
#[rstest]
#[case::workspace("--workspace")]
#[case::all("--all")]
fn workspace_aliases(#[case] flag: &str) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.upstream, "docs/guide.txt", "guide\n");
    fixture.path_manifest(
        "schemas = { src = 'schemas', dst = 'vendor/schemas' }\n\
         docs = { src = 'docs', dst = 'vendor/docs' }",
    );

    fixture.run(&[flag, "--overwrite", "--no-cache"]).success();

    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[("message.proto", "schema\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/docs"),
        &[("guide.txt", "guide\n")],
    );
}

/// Several capsule flags synchronize the requested capsules and leave others untouched.
#[test]
fn multiple_capsule_selections() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.upstream, "docs/guide.txt", "guide\n");
    write(&fixture.upstream, "other/file.txt", "unselected\n");
    fixture.path_manifest(
        "schemas = { src = 'schemas', dst = 'vendor/schemas' }\n\
         docs = { src = 'docs', dst = 'vendor/docs' }\n\
         other = { src = 'other', dst = 'vendor/other' }",
    );

    fixture
        .run(&["-c", "schemas", "-c", "docs", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[("message.proto", "schema\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/docs"),
        &[("guide.txt", "guide\n")],
    );
    assert!(!fixture.workspace.join("vendor/other").exists());
}

/// Repeating the same capsule selection has the same effect as selecting it once.
#[test]
fn repeated_capsule_selection() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");

    fixture
        .run(&[
            "-c",
            "schemas",
            "-c",
            "schemas",
            "--overwrite",
            "--no-cache",
        ])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// Destination paths are relative to an explicitly selected manifest, not the current directory.
#[test]
fn manifest_path_from_another_directory() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    let manifest = fixture.workspace.join("upstream.toml");
    let elsewhere = fixture.root.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();

    RunOutput::capture(
        fixture
            .tool(&[
                "--all",
                "--overwrite",
                "--no-cache",
                "--manifest-path",
                manifest.to_str().unwrap(),
            ])
            .dir(&elsewhere),
    )
    .success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
    assert_files(&elsewhere, &[]);
    assert!(
        fs::read_to_string(manifest)
            .unwrap()
            .contains("dst = 'vendor'")
    );
}
