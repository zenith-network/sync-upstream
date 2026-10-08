use std::fs;

use pretty_assertions::assert_eq;

use crate::fixtures::{Fixture, assert_capsule, write};

/// Capsules inherit the default source and recursive filter unless they override the filter.
#[test]
fn inherited_filter_and_capsule_override() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(
        &fixture.upstream,
        "schemas/nested/service.proto",
        "service\n",
    );
    write(&fixture.upstream, "schemas/readme.txt", "excluded\n");
    write(&fixture.upstream, "docs/guide.txt", "guide\n");
    write(&fixture.upstream, "docs/nested/details.txt", "details\n");
    write(&fixture.upstream, "docs/ignored.proto", "excluded\n");
    fixture.manifest(&format!(
        "[default]\npath = '{}'\nfilter = '**/*.proto'\n\n[capsules]\n\
         schemas = {{ src = 'schemas', dst = 'vendor/schemas' }}\n\
         docs = {{ src = 'docs', dst = 'vendor/docs', filter = '**/*.txt' }}\n",
        fixture.upstream.display()
    ));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[
            ("message.proto", "schema\n"),
            ("nested/service.proto", "service\n"),
        ],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/docs"),
        &[
            ("guide.txt", "guide\n"),
            ("nested/details.txt", "details\n"),
        ],
    );
}

/// A capsule's explicit path source overrides the default source.
#[test]
fn capsule_path_source_override() {
    let fixture = Fixture::new();
    let other_source = fixture.root.path().join("other-source");
    write(&fixture.upstream, "schemas/file.txt", "default\n");
    write(&other_source, "schemas/file.txt", "override\n");
    fixture.path_manifest(&format!(
        "inherited = {{ src = 'schemas', dst = 'vendor/inherited' }}\n\
         explicit = {{ src = 'schemas', dst = 'vendor/explicit', path = '{}' }}",
        other_source.display()
    ));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/inherited"),
        &[("file.txt", "default\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/explicit"),
        &[("file.txt", "override\n")],
    );
}

/// Narrowing a filter removes files that were copied by a previous broader filter.
#[test]
fn narrowing_filter_removes_old_files() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.upstream, "schemas/guide.txt", "guide\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor' }");
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor', filter = '**/*.proto' }");

    let output = fixture.run(&["--all", "--overwrite", "--no-cache"]);
    output.success();

    assert!(
        output.stdout.contains("guide.txt [delete]"),
        "{}",
        output.stdout
    );
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// A filter matching no files produces an empty managed capsule and removes stale payloads.
#[test]
fn filter_with_no_matches() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/readme.txt", "excluded\n");
    write(&fixture.workspace, "vendor/stale.proto", "stale\n");
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor', filter = '**/*.proto' }");

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(&fixture.workspace.join("vendor"), &[]);
}

/// Binary source files excluded by a filter do not prevent text files from synchronizing.
#[test]
fn excluded_binary_source_is_ignored() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.upstream, "schemas/image.bin", [0xff, 0x00, 0xfe]);
    fixture.path_manifest("schemas = { src = 'schemas', dst = 'vendor', filter = '**/*.proto' }");

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
    assert_eq!(
        fs::read(fixture.upstream.join("schemas/image.bin")).unwrap(),
        vec![0xff, 0x00, 0xfe]
    );
}
