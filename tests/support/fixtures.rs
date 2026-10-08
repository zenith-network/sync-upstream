use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
};

use duct::{Expression, cmd};
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use url::Url;
use walkdir::WalkDir;

/// Expected user-visible marker in each synchronized capsule directory.
pub const MARKER: &str = "This directory is controlled by sync-upstream\n";

/// Owns one test's temporary workspace, upstream, and artifact directories.
/// Dropping the fixture removes them all; tests never share repositories or caches.
pub struct Fixture {
    pub root: TempDir,
    pub workspace: PathBuf,
    pub upstream: PathBuf,
    pub cache: PathBuf,
    pub temporary_artifacts: PathBuf,
}

impl Fixture {
    /// Create a non-Git workspace and an empty upstream repository on the `main` branch.
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let fixture = Self {
            workspace: root.path().join("workspace"),
            upstream: root.path().join("upstream"),
            cache: root.path().join("cache"),
            temporary_artifacts: root.path().join("temporary-artifacts"),
            root,
        };
        for directory in [
            &fixture.workspace,
            &fixture.cache,
            &fixture.temporary_artifacts,
            &fixture.root.path().join("git-template"),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        write(fixture.root.path(), "git-config", "");
        fixture.init_repository(&fixture.upstream);
        fixture
    }

    /// Configure only the child's environment so parallel tests stay independent.
    /// Git uses fixture-local settings and file transport; temporary artifacts stay in the fixture.
    pub fn isolated(&self, mut expression: Expression) -> Expression {
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                expression = expression.env_remove(name);
            }
        }
        expression
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root.path().join("git-config"))
            .env("GIT_TEMPLATE_DIR", self.root.path().join("git-template"))
            .env("GIT_AUTHOR_NAME", "Integration Tests")
            .env("GIT_AUTHOR_EMAIL", "integration@example.invalid")
            .env("GIT_COMMITTER_NAME", "Integration Tests")
            .env("GIT_COMMITTER_EMAIL", "integration@example.invalid")
            .env("GIT_CONFIG_COUNT", "3")
            .env("GIT_CONFIG_KEY_0", "commit.gpgSign")
            .env("GIT_CONFIG_VALUE_0", "false")
            .env("GIT_CONFIG_KEY_1", "tag.gpgSign")
            .env("GIT_CONFIG_VALUE_1", "false")
            .env("GIT_CONFIG_KEY_2", "core.autocrlf")
            .env("GIT_CONFIG_VALUE_2", "false")
            .env("GIT_ALLOW_PROTOCOL", "file")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("NO_COLOR", "1")
            .env("TMPDIR", &self.temporary_artifacts)
    }

    /// Prepare the Cargo-built CLI in the workspace; callers can adjust it before execution.
    pub fn tool(&self, arguments: &[&str]) -> Expression {
        self.isolated(cmd(env!("CARGO_BIN_EXE_sync-upstream"), arguments))
            .dir(&self.workspace)
    }

    /// Run the CLI and retain its output even when it exits unsuccessfully.
    pub fn run(&self, arguments: &[&str]) -> RunOutput {
        RunOutput::capture(self.tool(arguments))
    }

    /// Run an isolated Git command, require success, and return trimmed stdout.
    pub fn git(&self, repository: &Path, arguments: &[&str]) -> String {
        let output = RunOutput::capture(self.isolated(cmd("git", arguments)).dir(repository));
        output.success();
        output.stdout.trim().to_owned()
    }

    /// Initialize a fixture repository with an explicit branch name, independent of user settings.
    pub fn init_repository(&self, repository: &Path) {
        fs::create_dir_all(repository).unwrap();
        self.git(repository, &["init", "--initial-branch=main"]);
    }

    /// Stage all changes, including deletions, commit them, and return the full commit ID.
    pub fn commit(&self, repository: &Path) -> String {
        self.git(repository, &["add", "--all"]);
        self.git(repository, &["commit", "--allow-empty", "-m", "fixture"]);
        self.git(repository, &["rev-parse", "HEAD"])
    }

    /// Replace `upstream.toml` with raw text so tests control the manifest without parsing it.
    pub fn manifest(&self, content: &str) {
        write(&self.workspace, "upstream.toml", content);
    }

    /// Build a manifest from capsule TOML entries, using the fixture upstream as the default path.
    pub fn path_manifest(&self, capsules: &str) {
        self.manifest(&format!(
            "[default]\npath = '{}'\n\n[capsules]\n{capsules}\n",
            self.upstream.display()
        ));
    }

    /// Use the local upstream URL with a reference fragment such as `branch = 'main'`.
    /// Capsule definitions are supplied as TOML entries.
    pub fn git_manifest(&self, reference: &str, capsules: &str) {
        self.manifest(&format!(
            "[default]\ngit = {{ url = '{}', {reference} }}\n\n[capsules]\n{capsules}\n",
            repository_url(&self.upstream)
        ));
    }
}

/// Captured exit status and text streams for explicit success or failure assertions.
pub struct RunOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    /// Capture both streams without treating a nonzero exit status as a fixture error.
    pub fn capture(expression: Expression) -> Self {
        let output = expression
            .stdout_capture()
            .stderr_capture()
            .unchecked()
            .run()
            .unwrap();
        Self {
            status: output.status,
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
        }
    }

    /// Require a successful exit, showing both streams if the assertion fails.
    pub fn success(&self) {
        assert!(
            self.status.success(),
            "expected success, got {}\nstdout:\n{}\nstderr:\n{}",
            self.status,
            self.stdout,
            self.stderr
        );
    }

    /// Require an unsuccessful exit and a diagnostic substring in stderr.
    pub fn failure(&self, message: &str) {
        assert!(
            !self.status.success(),
            "expected failure containing {message:?}\nstdout:\n{}\nstderr:\n{}",
            self.stdout,
            self.stderr
        );
        assert!(
            self.stderr.contains(message),
            "expected stderr to contain {message:?}, got:\n{}",
            self.stderr
        );
    }
}

/// Encode an absolute repository path as a `file://` URL, including any special characters.
pub fn repository_url(repository: &Path) -> String {
    Url::from_directory_path(repository).unwrap().to_string()
}

/// Write text or binary fixture data at a relative path, creating its parent directories.
pub fn write(root: &Path, relative: &str, content: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Collect relative file paths and bytes for complete, ordered tree comparisons.
/// Missing roots are empty; `.git` directories are skipped because Git inspects their state.
pub fn file_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    if !root.exists() {
        return BTreeMap::new();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git")
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

/// Compare the complete non-Git file tree against relative paths and expected text contents.
pub fn assert_files(root: &Path, expected: &[(&str, &str)]) {
    let expected = expected
        .iter()
        .map(|(path, content)| (PathBuf::from(path), content.as_bytes().to_vec()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(file_tree(root), expected);
}

/// Compare a capsule's complete payload and automatically include its expected marker file.
pub fn assert_capsule(root: &Path, expected: &[(&str, &str)]) {
    let mut expected = expected.to_vec();
    expected.push((".sync_upstream", MARKER));
    assert_files(root, &expected);
}
