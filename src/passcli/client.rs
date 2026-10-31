//! Running pass-cli and reporting how it failed.

use std::error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::executable;
use crate::keys;

/// The upstream executable protui drives.
pub const BINARY: &str = "pass-cli";

/// Bounds any single invocation. Network-backed calls occasionally stall;
/// without this the UI would wait on them indefinitely.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// The variables pass-cli reads a new key's passphrase from, in the order it
/// checks them. See `docs/schema.md` §6.1.
pub(super) const PASSPHRASE_VARIABLE: &str = "PROTON_PASS_SSH_KEY_PASSWORD";
const INHERITED_SECRETS: [&str; 2] = [PASSPHRASE_VARIABLE, "PROTON_PASS_SSH_KEY_PASSWORD_FILE"];

/// A reason protui cannot start at all, reported before the terminal UI takes
/// over.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupError {
    /// The pass-cli binary was not found on `PATH`.
    NotInstalled,
    /// pass-cli is installed but has no usable session.
    NoSession,
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(f, "{BINARY} is not installed or not on PATH"),
            Self::NoSession => {
                write!(
                    f,
                    "no valid {BINARY} session: run `{BINARY} login` to authenticate"
                )
            }
        }
    }
}

impl error::Error for StartupError {}

/// A failed pass-cli invocation.
///
/// It always names the subcommand, so the UI can say which call failed rather
/// than showing an empty list or a bare exit status.
#[must_use]
#[derive(Debug)]
pub struct CommandError {
    /// The subcommand, e.g. `item list`. Never contains user data.
    command: &'static str,
    /// Upstream's message, cleaned for display. Unstructured prose: display
    /// it, never match on it.
    stderr: String,
    failure: Failure,
}

/// What went wrong with an invocation.
#[must_use]
#[derive(Debug)]
pub(super) enum Failure {
    /// The process could not be started.
    Spawn(io::Error),
    /// The process outlived [`COMMAND_TIMEOUT`] and was killed.
    TimedOut,
    /// The process exited unsuccessfully, with its exit code if it had one.
    Exit(Option<i32>),
    /// The process succeeded but its output did not decode.
    Decode(serde_json::Error),
    /// The output decoded but does not match the documented shape. Almost
    /// always an upstream change; see `docs/schema.md`.
    Schema(String),
    /// The request was refused before anything ran.
    Invalid(&'static str),
}

impl CommandError {
    /// Wraps a failure of `command` that carries no stderr of its own.
    pub(super) fn new(command: &'static str, failure: Failure) -> Self {
        Self {
            command,
            stderr: String::new(),
            failure,
        }
    }

    /// Builds a failure with upstream's stderr, as the UI tests need one.
    #[cfg(test)]
    pub fn with_stderr(command: &'static str, stderr: &str) -> Self {
        Self {
            command,
            stderr: stderr.to_owned(),
            failure: Failure::Exit(Some(1)),
        }
    }

    /// What went wrong, for tests that tell failures apart.
    #[cfg(test)]
    pub(super) fn failure(&self) -> &Failure {
        &self.failure
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.stderr.is_empty() {
            write!(f, "{BINARY} {}: {}", self.command, self.failure)
        } else {
            write!(f, "{BINARY} {}: {}", self.command, self.stderr)
        }
    }
}

impl error::Error for CommandError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.failure)
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "could not run: {error}"),
            Self::TimedOut => write!(f, "timed out after {}s", COMMAND_TIMEOUT.as_secs()),
            Self::Exit(Some(code)) => write!(f, "exit status {code}"),
            Self::Exit(None) => f.write_str("terminated by a signal"),
            Self::Decode(error) => write!(f, "decode output: {error}"),
            Self::Schema(detail) => write!(f, "{detail}: unexpected {BINARY} output"),
            Self::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl error::Error for Failure {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Spawn(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::TimedOut | Self::Exit(_) | Self::Schema(_) | Self::Invalid(_) => None,
        }
    }
}

/// Runs pass-cli subcommands.
#[derive(Debug)]
pub struct Client {
    runner: Runner,
}

#[derive(Debug)]
enum Runner {
    Process(PathBuf),
    /// Records invocations instead of running them, so argument construction
    /// can be asserted without a real binary.
    #[cfg(test)]
    Fake(std::sync::Mutex<Fake>),
}

impl Client {
    /// Returns a client bound to the pass-cli binary on `PATH`.
    ///
    /// It does not verify that a session exists; call [`Client::preflight`]
    /// for that.
    pub fn try_new() -> Result<Self, StartupError> {
        let binary = executable::find(BINARY).ok_or(StartupError::NotInstalled)?;

        Ok(Self {
            runner: Runner::Process(binary),
        })
    }

    /// Verifies that pass-cli has a usable session.
    ///
    /// `info` exits 0 with a live session and 1 otherwise; its human-readable
    /// output is deliberately not parsed, since only the exit status is
    /// contractual.
    pub async fn preflight(&self) -> Result<(), StartupError> {
        match self.call("info", &["info"], &[]).await {
            Ok(_) => Ok(()),
            Err(_) => Err(StartupError::NoSession),
        }
    }

    /// Runs a subcommand and wraps any failure with its name.
    ///
    /// `command` is used only for error messages and must not contain user
    /// data. Only stdout is returned: upstream writes advisory lines to stderr
    /// even on success, so the two streams are kept apart and only stdout is
    /// ever decoded.
    pub(super) async fn call(
        &self,
        command: &'static str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<Vec<u8>, CommandError> {
        match &self.runner {
            Runner::Process(binary) => run(binary, command, args, env).await,
            #[cfg(test)]
            Runner::Fake(fake) => fake
                .lock()
                .expect("fake runner lock")
                .run(command, args, env),
        }
    }
}

/// Executes the binary with `args` as separate argv elements.
///
/// No shell is involved, so there is nothing to quote or inject, and the
/// binary is the one [`executable::find`] resolved rather than anything
/// supplied.
async fn run(
    binary: &Path,
    command: &'static str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<Vec<u8>, CommandError> {
    let mut process = build(binary, args, env);
    let child = process
        .spawn()
        .map_err(|error| CommandError::new(command, Failure::Spawn(error)))?;
    // The command holds the copy of the child's environment that protui
    // cannot wipe — the passphrase, on create. Free it as soon as the child
    // has its own, rather than when the child exits.
    drop(process);

    let output = tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| CommandError::new(command, Failure::TimedOut))?
        .map_err(|error| CommandError::new(command, Failure::Spawn(error)))?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(CommandError {
            command,
            stderr: clean_stderr(&output.stderr),
            failure: Failure::Exit(output.status.code()),
        })
    }
}

/// Prepares an invocation of the binary.
fn build(binary: &Path, args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut process = Command::new(binary);
    process
        .args(args)
        // Suppress the auto-update probe on every invocation: it adds latency
        // and can emit unexpected lines.
        .env("PROTON_PASS_NO_UPDATE_CHECK", "1");

    // A passphrase is protui's to give, never the user's shell's. Inherited,
    // either variable would silently encrypt a key the user asked to leave
    // unprotected.
    for variable in INHERITED_SECRETS {
        process.env_remove(variable);
    }

    process
        .envs(env.iter().copied())
        // The child must never read the terminal. If it tried to prompt (which
        // protui's argument choices avoid) it would otherwise fight the UI for
        // the TTY; with no stdin it fails fast instead of hanging.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A timed-out call drops the future, and with it the child.
        .kill_on_drop(true);

    process
}

/// Trims upstream's stderr for display, and drops the advisory line emitted
/// when a passphrase is read from the environment, which is informational
/// rather than a failure. Never used for control flow.
fn clean_stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("Reading password from"))
        // Upstream echoes item titles into some errors, so this is display
        // input from outside protui like any other.
        .map(|line| keys::sanitize(line.strip_prefix("Error: ").unwrap_or(line)))
        .collect::<Vec<String>>()
        .join("; ")
}

/// What a faked invocation returns, and what it was asked to run.
#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct Fake {
    pub stdout: Vec<u8>,
    pub fails: bool,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[cfg(test)]
impl Fake {
    fn run(
        &mut self,
        command: &'static str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<Vec<u8>, CommandError> {
        self.args = args.iter().map(|&arg| arg.to_owned()).collect();
        self.env = env
            .iter()
            .map(|&(name, value)| (name.to_owned(), value.to_owned()))
            .collect();

        if self.fails {
            Err(CommandError::new(command, Failure::Exit(Some(1))))
        } else {
            Ok(self.stdout.clone())
        }
    }
}

#[cfg(test)]
impl Client {
    /// A client whose every invocation succeeds and prints `stdout`.
    #[must_use]
    pub(super) fn fake(stdout: &str) -> Self {
        Self::from_fake(Fake {
            stdout: stdout.as_bytes().to_vec(),
            ..Fake::default()
        })
    }

    /// A client whose every invocation exits unsuccessfully.
    #[must_use]
    pub(super) fn failing() -> Self {
        Self::from_fake(Fake {
            fails: true,
            ..Fake::default()
        })
    }

    fn from_fake(fake: Fake) -> Self {
        Self {
            runner: Runner::Fake(std::sync::Mutex::new(fake)),
        }
    }

    /// Returns the argv and extra environment of the last invocation.
    #[must_use]
    pub(super) fn recorded(&self) -> (Vec<String>, Vec<(String, String)>) {
        match &self.runner {
            Runner::Fake(fake) => {
                let fake = fake.lock().expect("fake runner lock");
                (fake.args.clone(), fake.env.clone())
            }
            // A real process records nothing.
            Runner::Process(_) => (Vec::new(), Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_stderr_table() {
        let cases = [
            (
                "strips the Error prefix",
                "Error: Please provide either --share-id, --vault-name\n",
                "Please provide either --share-id, --vault-name",
            ),
            (
                // Upstream writes this to stderr on the success path when a
                // passphrase comes from the environment; it is not a failure.
                "drops the passphrase advisory",
                "Reading password from environment variable PROTON_PASS_SSH_KEY_PASSWORD\n",
                "",
            ),
            (
                "keeps a real error alongside the advisory",
                "Reading password from environment variable X\nError: vault not found\n",
                "vault not found",
            ),
            ("empty", "", ""),
        ];

        for (name, input, want) in cases {
            assert_eq!(clean_stderr(input.as_bytes()), want, "{name}");
        }
    }

    /// Upstream echoes item titles into some of its errors, so its stderr is
    /// outside text like any other.
    #[test]
    fn clean_stderr_sanitizes() {
        let cleaned = clean_stderr(b"Error: no such item \x1b[2J\x1b]52;c;cHduZWQ=\x07");

        assert!(
            !cleaned.chars().any(char::is_control),
            "escapes survived: {cleaned:?}"
        );
    }

    #[tokio::test]
    async fn preflight_runs_info() {
        let client = Client::fake("");

        client.preflight().await.expect("preflight succeeds");

        // The error label and the argument list are separate parameters, and
        // passing only the label runs the bare binary, which exits non-zero
        // and looks exactly like a missing session.
        assert_eq!(client.recorded().0, ["info"]);
    }

    #[tokio::test]
    async fn preflight_reports_no_session() {
        let error = Client::failing().preflight().await.expect_err("no session");

        assert_eq!(error, StartupError::NoSession);
        assert!(
            error.to_string().contains("login"),
            "{error} should say to log in"
        );
    }

    #[tokio::test]
    async fn command_errors_name_the_command() {
        let error = Client::failing()
            .vaults()
            .await
            .expect_err("the call fails");

        assert_eq!(error.command, "vault list");
        assert!(
            error.to_string().contains("vault list"),
            "{error} does not name the command"
        );
    }

    /// Upstream returning success with output protui cannot parse still needs
    /// to say which call produced it.
    #[tokio::test]
    async fn decode_failures_are_command_errors() {
        let error = Client::fake("this is not json")
            .vaults()
            .await
            .expect_err("bad output");

        assert_eq!(error.command, "vault list");
        assert!(matches!(error.failure, Failure::Decode(_)), "{error:?}");
    }

    /// An exported passphrase variable must not reach a key the user asked
    /// to leave unprotected.
    #[test]
    fn inherited_passphrases_are_removed() {
        let process = build(Path::new("/bin/sh"), &[], &[("EXTRA", "value")]);
        let env: Vec<(String, Option<String>)> = process
            .as_std()
            .get_envs()
            .map(|(name, value)| {
                let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
                (text(name), value.map(text))
            })
            .collect();

        for variable in INHERITED_SECRETS {
            assert!(
                env.contains(&(variable.to_owned(), None)),
                "{variable} is inherited: {env:?}"
            );
        }
        assert!(env.contains(&("EXTRA".to_owned(), Some("value".to_owned()))));
        assert!(env.contains(&(
            "PROTON_PASS_NO_UPDATE_CHECK".to_owned(),
            Some("1".to_owned())
        )));
    }

    #[tokio::test]
    async fn a_real_process_reports_its_exit_status_and_stderr() {
        let client = Client {
            runner: Runner::Process(PathBuf::from("/bin/sh")),
        };

        let error = client
            .call("probe", &["-c", "echo 'Error: it broke' >&2; exit 3"], &[])
            .await
            .expect_err("the script exits non-zero");

        assert!(matches!(error.failure, Failure::Exit(Some(3))), "{error:?}");
        assert_eq!(error.to_string(), "pass-cli probe: it broke");
    }

    #[tokio::test]
    async fn a_real_process_sees_its_environment_and_no_stdin() {
        let client = Client {
            runner: Runner::Process(PathBuf::from("/bin/sh")),
        };

        let stdout = client
            .call(
                "probe",
                &[
                    "-c",
                    r#"printf '%s|%s|' "$PROTON_PASS_NO_UPDATE_CHECK" "$EXTRA"; cat"#,
                ],
                &[("EXTRA", "value")],
            )
            .await
            .expect("the script succeeds");

        // `cat` returns at once because stdin is /dev/null, not the terminal.
        assert_eq!(stdout, b"1|value|");
    }
}
