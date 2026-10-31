//! protui is an unofficial terminal UI for managing Proton Pass SSH keys.
//!
//! It drives the official pass-cli binary and is not affiliated with or
//! endorsed by Proton AG.

use std::env;
use std::io;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use crossterm::style::Color;

use crate::passcli::{BINARY, Client, StartupError};

mod clipboard;
mod executable;
mod keys;
mod passcli;
mod ui;

#[cfg(not(unix))]
compile_error!("protui supports macOS and Linux only");

/// The version, overridable at build time through `PROTUI_VERSION` so release
/// builds can carry the tag they were cut from.
const VERSION: &str = match option_env!("PROTUI_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// Bounds the startup session check, so a stalled network call cannot hang the
/// launch indefinitely.
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(20);

const USAGE: &str = "\
protui — a terminal UI for Proton Pass SSH keys.

Usage:
  protui            launch the UI
  protui --version  print the version
  protui --help     print this message

Requires the official pass-cli binary, authenticated with 'pass-cli login'.
Not affiliated with or endorsed by Proton AG.";

/// Whether output may carry colour.
#[derive(Clone, Copy)]
enum Paint {
    Colour,
    Plain,
}

/// Why protui exited early.
enum Failure {
    Startup(StartupError),
    Terminal(io::Error),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if let Some(argument) = env::args_os().nth(1) {
        return match argument.to_str() {
            Some("-v" | "--version" | "version") => {
                println!("protui {VERSION}");
                ExitCode::SUCCESS
            }
            Some("-h" | "--help" | "help") => {
                println!("{USAGE}");
                ExitCode::SUCCESS
            }
            _ => {
                // Debug formatting quotes and escapes the argument, so nothing
                // typed on the command line is echoed raw.
                #[expect(clippy::unnecessary_debug_formatting, reason = "escapes the argument")]
                let quoted = format!("{argument:?}");
                eprintln!("protui: unknown argument {quoted}\n\n{USAGE}");
                ExitCode::from(2)
            }
        };
    }

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprint!("{}", fatal_message(&failure, stderr_paint()));
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Failure> {
    // pass-cli must be present and authenticated before the TUI takes over
    // the terminal: a missing session is the most likely first-run failure,
    // and it is far clearer reported as plain text than inside a full-screen
    // UI.
    let client = Arc::new(Client::try_new().map_err(Failure::Startup)?);

    // The vault list needs a session but not the check's verdict, so it runs
    // alongside the check rather than after it, taking one pass-cli round trip
    // off every launch. If the check fails, the list is dropped unread.
    let vaults = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.vaults().await }
    });

    tokio::time::timeout(PREFLIGHT_TIMEOUT, client.preflight())
        .await
        .unwrap_or(Err(StartupError::NoSession))
        .map_err(Failure::Startup)?;

    ui::run(client, vaults).await.map_err(Failure::Terminal)
}

/// Renders a startup failure as plain text for a terminal the UI never took
/// over.
fn fatal_message(failure: &Failure, paint: Paint) -> String {
    use std::fmt::Write as _;

    use crossterm::style::Stylize;

    let paint = |text: &str, color: Color| match paint {
        Paint::Colour => text.with(color).to_string(),
        Paint::Plain => text.to_owned(),
    };

    let mut message = paint("✗ protui cannot start.", Color::Red);
    message.push_str("\n\n");

    // Writing to a String cannot fail.
    let _ = match failure {
        Failure::Startup(StartupError::NotInstalled) => writeln!(
            message,
            "{BINARY} is not installed, or is not on your PATH.\n\n{}\n{}",
            paint(
                "protui is a front-end for it, so install it first:",
                Color::DarkGrey
            ),
            paint("  https://protonpass.github.io/pass-cli/", Color::Magenta),
        ),
        Failure::Startup(StartupError::NoSession) => writeln!(
            message,
            "{BINARY} is installed but has no valid session.\n\n{}\n{}",
            paint("Authenticate first:", Color::DarkGrey),
            paint(&format!("  {BINARY} login"), Color::Magenta),
        ),
        Failure::Terminal(error) => writeln!(message, "terminal UI failed: {error}"),
    };

    message
}

/// Colours stderr only when it is a terminal that has not opted out.
fn stderr_paint() -> Paint {
    use std::io::IsTerminal;

    if io::stderr().is_terminal() && !ui::no_color() {
        Paint::Colour
    } else {
        Paint::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatal_messages_say_what_to_do() {
        let missing = fatal_message(&Failure::Startup(StartupError::NotInstalled), Paint::Plain);
        assert!(missing.contains("not installed"), "{missing}");
        assert!(
            missing.contains("https://protonpass.github.io/pass-cli/"),
            "{missing}"
        );

        let logged_out = fatal_message(&Failure::Startup(StartupError::NoSession), Paint::Plain);
        assert!(logged_out.contains("pass-cli login"), "{logged_out}");

        let broken = fatal_message(&Failure::Terminal(io::Error::other("no tty")), Paint::Plain);
        assert!(broken.contains("terminal UI failed: no tty"), "{broken}");
    }

    #[test]
    fn plain_output_carries_no_escapes() {
        let message = fatal_message(&Failure::Startup(StartupError::NoSession), Paint::Plain);

        assert!(!message.contains('\u{1b}'), "{message:?}");
    }
}
