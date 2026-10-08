#![allow(dead_code, reason = "Generic methods, maybe useful in the future")]

use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    string::FromUtf8Error,
    thread,
};

/// Command factory for git executable
///
/// Helps executing git commands through subprocesses.
#[derive(Clone, Debug)]
pub struct Git {
    verbose: bool,
    dir: Option<PathBuf>,
    git_args: Vec<String>,
}

impl Git {
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// New git command factory
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            dir: None,
            git_args: Vec::new(),
        }
    }

    /// Git which runs with `-C dir`
    ///
    /// Note that this won't create `dir`, if it's missing.
    pub fn new_in(dir: PathBuf, verbose: bool) -> Self {
        Self {
            verbose,
            dir: Some(dir),
            git_args: Vec::new(),
        }
    }

    /// Get git command with directory and output streams set
    pub fn command(&self) -> GitCommand {
        let mut command = Command::new("git");

        // We don't want the advice messages to pollute logs,
        // they are not useful for the user in this case anyway.
        command.env("GIT_ADVICE", "0");

        if let Some(path) = &self.dir {
            command.current_dir(path);
        }
        for arg in &self.git_args {
            command.arg(arg);
        }
        GitCommand {
            command,
            verbose: self.verbose,
        }
    }

    /// Get git command with subcommand set
    pub fn with_subcommand(&self, subcommand: &str) -> GitCommand {
        let mut command = self.command();
        command.command.arg(subcommand);
        command
    }

    /// `git checkout`
    pub fn checkout(&self) -> GitCommand {
        self.with_subcommand("checkout")
    }

    /// `git clone`
    ///
    /// Underscore added to avoid confusion with `Clone::clone`.
    pub fn clone_(&self) -> GitCommand {
        self.with_subcommand("clone")
    }

    /// `git fetch`
    pub fn fetch(&self) -> GitCommand {
        self.with_subcommand("fetch")
    }

    /// `git reset`
    pub fn reset(&self) -> GitCommand {
        self.with_subcommand("reset")
    }

    /// `git rev-parse`
    pub fn rev_parse(&self) -> GitCommand {
        self.with_subcommand("rev-parse")
    }

    /// `git sparse-checkout`
    pub fn sparse_checkout(&self) -> GitCommand {
        self.with_subcommand("sparse-checkout")
    }

    /// `git ls-remote`
    pub fn ls_remote(&self) -> GitCommand {
        self.with_subcommand("ls-remote")
    }

    /// `git status`
    pub fn status(&self) -> GitCommand {
        self.with_subcommand("status")
    }

    /// Returns `true` if specified dir is inside a git work tree.
    ///
    /// If dir is not specified, returns `false`.
    pub fn is_inside_work_tree(&self) -> bool {
        if self.dir.is_none() {
            return false;
        }

        let mut command = self.rev_parse().arg("--is-inside-work-tree");
        command.set_verbose(false);
        command
            .run()
            .is_ok_and(|output| output.stdout().trim() == "true")
    }

    /// Returns `true` if specified dir is a root of git repository.
    ///
    /// If dir is not specified, returns `false`.
    pub fn is_repository(&self) -> bool {
        if self.dir.is_none() {
            return false;
        }
        // "." checks for bare repos, where there is no ".git" directory
        [".git", "."].into_iter().any(|path| {
            let mut cmd = self.rev_parse().args(["--resolve-git-dir", path]);
            // This will print ".git" on success - not really a useful log,
            // so suppressing it.
            cmd.set_verbose(false);
            cmd.run().is_ok()
        })
    }
}

#[derive(Debug)]
pub struct GitCommand {
    command: Command,
    verbose: bool,
}

impl GitCommand {
    /// Restore original command as a single string.
    fn command_string(&self) -> String {
        let program = self.command.get_program().to_string_lossy().to_string();
        let args = self
            .command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        format!("{program} {args}")
    }

    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    /// Run command
    pub fn run(mut self) -> Result<GitOutput, GitError> {
        let restored_command = self.command_string();
        let mut child = self
            .command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| GitError {
                command: restored_command.clone(),
                kind: ErrorKind::IOError(error),
            })?;
        let stdout = child
            .stdout
            .take()
            .expect("stdout handle should be present after piped");
        let stderr = child
            .stderr
            .take()
            .expect("stderr handle should be present after piped");

        // stdout/stderr collection
        let output = thread::scope(|scope| {
            let stdout =
                scope.spawn(|| Self::collect_output(stdout, io::stdout().lock(), self.verbose));
            let stderr =
                scope.spawn(|| Self::collect_output(stderr, io::stderr().lock(), self.verbose));
            let stdout_result = stdout.join().map_err(|_| GitError {
                command: restored_command.clone(),
                kind: ErrorKind::JoinError,
            })?;
            let stderr_result = stderr.join().map_err(|_| GitError {
                command: restored_command.clone(),
                kind: ErrorKind::JoinError,
            })?;
            Ok(GitOutput {
                stdout: stdout_result.map_err(|kind| GitError {
                    command: restored_command.clone(),
                    kind,
                })?,
                stderr: stderr_result.map_err(|kind| GitError {
                    command: restored_command.clone(),
                    kind,
                })?,
            })
        });

        let status = child.wait().map_err(|error| GitError {
            command: restored_command.clone(),
            kind: ErrorKind::GitJoinError(error),
        })?;

        if !status.success() {
            return Err(GitError {
                command: restored_command,
                kind: ErrorKind::Failure {
                    stderr: output?.stderr,
                    status,
                },
            });
        }
        output
    }

    fn collect_output(
        mut reader: impl Read,
        mut writer: impl Write,
        verbose: bool,
    ) -> Result<String, ErrorKind> {
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let read_result = reader.read(&mut buffer);
            if matches!(&read_result, Err(error) if error.kind() == std::io::ErrorKind::Interrupted)
            {
                continue;
            }
            let count = read_result.map_err(ErrorKind::OutputError)?;
            if count == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..count]);
            if verbose {
                writer
                    .write_all(&buffer[..count])
                    .map_err(ErrorKind::OutputError)?;
                writer.flush().map_err(ErrorKind::OutputError)?;
            }
        }
        String::from_utf8(output).map_err(ErrorKind::FromUtf8Error)
    }

    /// Add argument
    pub fn arg<S>(mut self, arg: S) -> Self
    where
        S: AsRef<OsStr>,
    {
        self.command.arg(arg);
        self
    }

    /// Add arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Add env var
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, val);
        self
    }
}

#[derive(Clone, Debug)]
pub struct GitOutput {
    stdout: String,
    stderr: String,
}

impl GitOutput {
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

#[derive(Debug, thiserror::Error)]
#[error("git failed: {kind}")]
pub struct GitError {
    command: String,
    kind: ErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum ErrorKind {
    #[error("output is not a valid UTF-8: {0}")]
    FromUtf8Error(#[source] FromUtf8Error),

    #[error("I/O error: {0}")]
    IOError(#[source] io::Error),

    #[error("failed to join git: {0}")]
    GitJoinError(#[source] io::Error),

    #[error("failed to handle output: {0}")]
    OutputError(#[source] io::Error),

    #[error("failed to join collection thread")]
    JoinError,

    #[error("status {status}: {stderr}")]
    Failure { stderr: String, status: ExitStatus },
}
