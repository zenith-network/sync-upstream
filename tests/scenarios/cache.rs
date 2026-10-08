use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::{
    cache_support::{Checkout, cache_state, checkout_paths},
    fixtures::{
        Fixture, RunOutput, assert_capsule, assert_files, file_tree, repository_url, write,
    },
};

/// Cached checkouts live under the default or custom target and contain only the requested subtree and root files.
#[rstest]
#[case::default_target(false)]
#[case::custom_target(true)]
fn cache_location_and_checkout_contents(#[case] custom_target: bool) {
    let fixture = Fixture::new();
    write(
        &fixture.upstream,
        "schemas/nested/message.proto",
        "schema\n",
    );
    write(&fixture.upstream, "README.md", "root file\n");
    write(
        &fixture.upstream,
        "unselected/file.txt",
        "excluded subtree\n",
    );
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let target = if custom_target {
        fixture.cache.clone()
    } else {
        fixture.workspace.join("target")
    };
    let mut arguments = vec!["--all", "--overwrite"];
    if custom_target {
        arguments.extend(["--target-dir", target.to_str().unwrap()]);
    }

    fixture.run(&arguments).success();

    let paths = checkout_paths(&target);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].starts_with(target.join("sync-upstream")));
    assert_eq!(
        cache_state(&fixture, &target),
        vec![Checkout {
            path: paths[0].clone(),
            remote: repository_url(&fixture.upstream),
            commit,
            files: BTreeMap::from([
                (PathBuf::from("README.md"), b"root file\n".to_vec()),
                (
                    PathBuf::from("schemas/nested/message.proto"),
                    b"schema\n".to_vec()
                ),
            ]),
        }]
    );
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("nested/message.proto", "schema\n")],
    );
    if custom_target {
        assert!(!fixture.workspace.join("target").exists());
    }
}

/// Default cache placement follows an explicit manifest even when invoked elsewhere.
#[test]
fn default_cache_follows_manifest() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/file.txt", "schema\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let manifest = fixture.workspace.join("upstream.toml");

    RunOutput::capture(
        fixture
            .tool(&[
                "--all",
                "--overwrite",
                "--manifest-path",
                manifest.to_str().unwrap(),
            ])
            .dir(fixture.root.path()),
    )
    .success();

    assert_eq!(checkout_paths(&fixture.workspace.join("target")).len(), 1);
    assert!(!fixture.root.path().join("target").exists());
}

/// Repeated cached runs keep the same checkout instead of recreating or duplicating it.
#[test]
fn repeated_runs_reuse_checkout() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let arguments = [
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    fixture.run(&arguments).success();
    let before = cache_state(&fixture, &fixture.cache);
    assert_eq!(before.len(), 1);
    write(&before[0].path, ".git/integration-reuse", "same checkout\n");

    fixture.run(&arguments).success();

    assert_eq!(cache_state(&fixture, &fixture.cache), before);
    assert_eq!(
        fs::read_to_string(before[0].path.join(".git/integration-reuse")).unwrap(),
        "same checkout\n"
    );
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// Capsules sharing a repository and commit share a checkout containing both requested subtrees.
#[test]
fn capsules_share_checkout_and_expand_sparse_paths() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    write(&fixture.upstream, "docs/guide.txt", "guide\n");
    write(&fixture.upstream, "unselected/other.txt", "excluded\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor/schemas' }\n\
         docs = { src = 'docs', dst = 'vendor/docs' }",
    );
    let target = fixture.cache.to_str().unwrap();
    fixture
        .run(&["-c", "schemas", "--overwrite", "--target-dir", target])
        .success();
    let initial_paths = checkout_paths(&fixture.cache);
    assert_eq!(initial_paths.len(), 1);
    assert_files(&initial_paths[0], &[("schemas/message.proto", "schema\n")]);

    fixture
        .run(&["--all", "--overwrite", "--target-dir", target])
        .success();

    assert_eq!(checkout_paths(&fixture.cache), initial_paths);
    assert_files(
        &initial_paths[0],
        &[
            ("schemas/message.proto", "schema\n"),
            ("docs/guide.txt", "guide\n"),
        ],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/schemas"),
        &[("message.proto", "schema\n")],
    );
    assert_capsule(
        &fixture.workspace.join("vendor/docs"),
        &[("guide.txt", "guide\n")],
    );
}

/// Advancing a branch creates a new cached revision while preserving the previous checkout.
#[test]
fn branch_advancement_preserves_old_checkout() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "old\n");
    let old_commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let arguments = [
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    fixture.run(&arguments).success();
    let old_state = cache_state(&fixture, &fixture.cache);
    assert_eq!(old_state.len(), 1);
    write(&fixture.upstream, "schemas/message.proto", "new\n");
    let new_commit = fixture.commit(&fixture.upstream);

    fixture.run(&arguments).success();

    let state = cache_state(&fixture, &fixture.cache);
    assert_eq!(state.len(), 2);
    assert!(state.contains(&old_state[0]), "{state:#?}");
    let commits = state
        .iter()
        .map(|checkout| checkout.commit.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(commits, BTreeSet::from([old_commit, new_commit]));
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "new\n")],
    );
}

/// A cached pinned revision remains usable when its local upstream is unavailable.
#[test]
fn pinned_cache_survives_unavailable_upstream() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "cached\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let arguments = [
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    fixture.run(&arguments).success();
    let before = cache_state(&fixture, &fixture.cache);
    fs::rename(
        &fixture.upstream,
        fixture.root.path().join("unavailable-upstream"),
    )
    .unwrap();
    write(&fixture.workspace, "vendor/message.proto", "local edit\n");

    fixture.run(&arguments).success();

    assert_eq!(cache_state(&fixture, &fixture.cache), before);
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "cached\n")],
    );
}

/// Uncached synchronization leaves persistent storage untouched and removes temporary checkouts.
#[rstest]
#[case::empty_cache(false)]
#[case::existing_cache(true)]
fn no_cache_preserves_persistent_state_and_cleans_temporary_files(#[case] existing_cache: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let target = fixture.cache.to_str().unwrap();
    if existing_cache {
        fixture
            .run(&["--all", "--overwrite", "--target-dir", target])
            .success();
    }
    write(&fixture.cache, "unrelated.txt", "preserve\n");
    let before = cache_state(&fixture, &fixture.cache);
    let payload_before = file_tree(&fixture.cache);
    let entries_before = fs::read_dir(&fixture.cache)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<BTreeSet<_>>();

    fixture
        .run(&["--all", "--overwrite", "--no-cache", "--target-dir", target])
        .success();

    assert_eq!(cache_state(&fixture, &fixture.cache), before);
    assert_eq!(file_tree(&fixture.cache), payload_before);
    let entries_after = fs::read_dir(&fixture.cache)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<BTreeSet<_>>();
    assert_eq!(entries_after, entries_before);
    assert_eq!(
        fs::read_dir(&fixture.temporary_artifacts).unwrap().count(),
        0
    );
    assert!(!fixture.workspace.join("target").exists());
    assert_capsule(
        &fixture.workspace.join("vendor"),
        &[("message.proto", "schema\n")],
    );
}

/// Failed uncached runs also remove temporary artifacts and leave persistent cache state untouched.
#[rstest]
#[case::clone_failure(false)]
#[case::resolution_after_clone_failure(true)]
fn no_cache_cleans_temporary_files_on_failure(#[case] invalid_text: bool) {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    if invalid_text {
        write(&fixture.upstream, "schemas/binary.bin", [0xff, 0xfe]);
    }
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    if !invalid_text {
        fs::rename(
            &fixture.upstream,
            fixture.root.path().join("unavailable-upstream"),
        )
        .unwrap();
    }
    write(&fixture.cache, "unrelated.txt", "preserve\n");
    let before = file_tree(&fixture.cache);

    fixture
        .run(&[
            "--all",
            "--overwrite",
            "--no-cache",
            "--target-dir",
            fixture.cache.to_str().unwrap(),
        ])
        .failure("failed to resolve capsule 'schemas'");

    assert_eq!(file_tree(&fixture.cache), before);
    assert_eq!(
        fs::read_dir(&fixture.temporary_artifacts).unwrap().count(),
        0
    );
    assert!(!fixture.cache.join("sync-upstream").exists());
    assert!(!fixture.workspace.join("vendor").exists());
}

/// Dry runs populate persistent cache without creating destination files or a lockfile.
#[test]
fn dry_run_populates_cache_only() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "schema\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );

    fixture
        .run(&[
            "--all",
            "--dry-run",
            "--target-dir",
            fixture.cache.to_str().unwrap(),
        ])
        .success();

    let state = cache_state(&fixture, &fixture.cache);
    assert_eq!(state.len(), 1);
    assert_eq!(
        (&state[0].remote, &state[0].commit),
        (&repository_url(&fixture.upstream), &commit)
    );
    assert_files(&state[0].path, &[("schemas/message.proto", "schema\n")]);
    assert!(!fixture.workspace.join("vendor").exists());
    assert!(!fixture.workspace.join("upstream.lock").exists());
}

/// Broken cached Git metadata fails instead of overwriting existing destination files.
#[test]
fn corrupted_cached_checkout() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "upstream\n");
    let commit = fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        &format!("rev = '{commit}'"),
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let arguments = [
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    fixture.run(&arguments).success();
    let paths = checkout_paths(&fixture.cache);
    assert_eq!(paths.len(), 1);
    write(&paths[0], ".git/HEAD", "broken metadata\n");
    write(
        &fixture.workspace,
        "vendor/message.proto",
        "preserve local\n",
    );
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture.run(&arguments).failure("failed to find sources");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert_eq!(checkout_paths(&fixture.cache), paths);
    assert_eq!(
        fs::read_to_string(paths[0].join(".git/HEAD")).unwrap(),
        "broken metadata\n"
    );
}

/// A cache directory becoming a regular file produces an error without changing destinations.
#[test]
fn cache_directory_becomes_obstructed() {
    let fixture = Fixture::new();
    write(&fixture.upstream, "schemas/message.proto", "upstream\n");
    fixture.commit(&fixture.upstream);
    fixture.git_manifest(
        "branch = 'main'",
        "schemas = { src = 'schemas', dst = 'vendor' }",
    );
    let arguments = [
        "--all",
        "--overwrite",
        "--target-dir",
        fixture.cache.to_str().unwrap(),
    ];
    fixture.run(&arguments).success();
    fs::rename(&fixture.cache, fixture.root.path().join("old-cache")).unwrap();
    fs::write(&fixture.cache, "obstruction\n").unwrap();
    write(&fixture.upstream, "schemas/message.proto", "new\n");
    fixture.commit(&fixture.upstream);
    let before = file_tree(&fixture.workspace.join("vendor"));

    fixture
        .run(&arguments)
        .failure("failed to create cached directory");

    assert_eq!(file_tree(&fixture.workspace.join("vendor")), before);
    assert_eq!(fs::read_to_string(&fixture.cache).unwrap(), "obstruction\n");
}
