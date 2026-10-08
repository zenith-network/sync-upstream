use std::fs;

use rstest::rstest;

use crate::fixtures::{Fixture, assert_capsule, repository_url, write};

#[derive(Clone, Copy)]
enum Reference {
    Revision,
    Branch,
    LightweightTag,
    AnnotatedTag,
}

/// Local Git revisions, branches, and both tag types synchronize correctly across repeated runs.
#[rstest]
#[case::revision(Reference::Revision)]
#[case::branch(Reference::Branch)]
#[case::lightweight_tag(Reference::LightweightTag)]
#[case::annotated_tag(Reference::AnnotatedTag)]
fn git_reference_types(#[case] reference: Reference, #[values(false, true)] no_cache: bool) {
    let fixture = Fixture::new();
    write(
        &fixture.upstream,
        "schemas/nested/message.proto",
        "schema\n",
    );
    let commit = fixture.commit(&fixture.upstream);
    let reference = match reference {
        Reference::Revision => format!("rev = '{commit}'"),
        Reference::Branch => {
            fixture.git(&fixture.upstream, &["branch", "feature/schemas"]);
            "branch = 'feature/schemas'".to_owned()
        }
        Reference::LightweightTag => {
            fixture.git(&fixture.upstream, &["tag", "release/lightweight"]);
            "tag = 'release/lightweight'".to_owned()
        }
        Reference::AnnotatedTag => {
            fixture.git(
                &fixture.upstream,
                &["tag", "-a", "release/annotated", "-m", "release"],
            );
            "tag = 'release/annotated'".to_owned()
        }
    };
    fixture.git_manifest(&reference, "schemas = { src = 'schemas', dst = 'vendor' }");
    let mut arguments = vec![
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    if no_cache {
        arguments.push("--no-cache");
    }

    fixture.run(&arguments).success();
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("nested/message.proto", "schema\n")],
    );
    let repeated = fixture.run(&arguments);
    repeated.success();
    assert!(
        repeated.stdout.contains("Up to date"),
        "{}",
        repeated.stdout
    );
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("nested/message.proto", "schema\n")],
    );
}

/// A branch update brings new, modified, and deleted files into the destination.
#[rstest]
#[case::cached(false)]
#[case::uncached(true)]
fn branch_advancement(#[case] no_cache: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/changed.proto", "old\n");
    write(&fixture.upstream, "schemas/deleted.proto", "deleted\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let mut arguments = vec![
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    if no_cache {
        arguments.push("--no-cache");
    }
    fixture.run(&arguments).success();
    write(&fixture.upstream, "schemas/changed.proto", "new\n");
    write(&fixture.upstream, "schemas/added.proto", "added\n");
    fs::remove_file(fixture.upstream.join("schemas/deleted.proto")).unwrap();
    fixture.commit(&fixture.upstream);

    fixture.run(&arguments).success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("changed.proto", "new\n"), ("added.proto", "added\n")],
    );
}

/// A pinned revision retains its original files even after the upstream branch advances.
#[test]
fn pinned_revision_does_not_advance() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "pinned\n");
    let pinned = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{pinned}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();
    write(&fixture.upstream, "schemas/message.proto", "latest\n");
    fixture.commit(&fixture.upstream);

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "pinned\n")],
    );
}

/// A capsule can inherit a Git URL while overriding the default Git reference.
#[test]
fn capsule_reference_override() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "release\n");
    fixture.commit(&fixture.upstream);
    fixture.git(&fixture.upstream, &["tag", "release"]);
    write(&fixture.upstream, "schemas/message.proto", "main\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "latest = { src = 'schemas', dst = 'vendor/latest' }\n\
         released = { src = 'schemas', dst = 'vendor/released', git = { tag = 'release' } }",
    );

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/latest"),
        &[("message.proto", "main\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/released"),
        &[("message.proto", "release\n")],
    );
}

/// Capsules can synchronize independent local Git repositories in one invocation.
#[test]
fn independent_upstreams() {
    let fixture = Fixture::new();
    let other = fixture.root.path().join("other-upstream");
    fixture.init_repository(&other);
    write(&fixture.upstream, "schemas/message.proto", "first\n");
    write(&other, "schemas/message.proto", "second\n");
    fixture.commit(&fixture.upstream);
    fixture.commit(&other);
    fixture.git_manifest("branch = 'main'", &format!(
        "first = {{ src = 'schemas', dst = 'vendor/first' }}\n\
         second = {{ src = 'schemas', dst = 'vendor/second', git = {{ url = '{}', branch = 'main' }} }}",
        repository_url(&other)
    ));

    fixture
        .run(&["--all", "--overwrite", "--no-cache"])
        .success();

    assert_capsule(
        &fixture.workspace.join("vendor/first"),
        &[("message.proto", "first\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/second"),
        &[("message.proto", "second\n")],
    );
}
