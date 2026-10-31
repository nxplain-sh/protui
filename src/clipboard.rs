//! Writing public keys to the system clipboard.
//!
//! This shells out to the platform's clipboard tool — `pbcopy` on macOS,
//! `wl-copy`, `xclip` or `xsel` on Linux — the same tools a terminal user
//! would reach for. The text goes over the child's stdin, never argv, and only
//! public key material is ever copied: protui has no path that reads a private
//! key.

use std::env;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::process::Command;

use crate::executable;

/// Bounds a clipboard write. The tools return as soon as they own the
/// selection, so this only fires if one hangs.
const COPY_TIMEOUT: Duration = Duration::from_secs(5);

/// Why the clipboard could not be written.
#[must_use]
#[derive(Debug)]
pub enum Error {
    /// None of the clipboard tools is installed.
    NoTool,
    Io(io::Error),
    Failed(ExitStatus),
    TimedOut,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTool if cfg!(target_os = "macos") => f.write_str("pbcopy was not found"),
            Self::NoTool => f.write_str("install wl-clipboard, xclip or xsel to copy"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Failed(status) => write!(f, "the clipboard tool failed ({status})"),
            Self::TimedOut => f.write_str("the clipboard tool did not respond"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::NoTool | Self::Failed(_) | Self::TimedOut => None,
        }
    }
}

/// Puts `text` on the clipboard.
pub async fn copy(text: &str) -> Result<(), Error> {
    let (program, args) = tools()
        .into_iter()
        .find_map(|(name, args)| executable::find(name).map(|program| (program, args)))
        .ok_or(Error::NoTool)?;

    pipe(&program, args, text).await
}

/// Runs a clipboard tool and hands it `text` on stdin.
async fn pipe(program: &Path, args: &[&str], text: &str) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        // xclip and wl-copy fork a process that keeps serving the selection;
        // with no pipes held open, waiting on the parent returns at once.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(Error::Io)?;

    let write_and_wait = async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Io(io::Error::from(io::ErrorKind::BrokenPipe)))?;
        let written = stdin.write_all(text.as_bytes()).await;
        // Closing stdin is what tells the tool the text is complete.
        drop(stdin);
        let status = child.wait().await.map_err(Error::Io)?;

        // A tool that gives up before reading — xclip with no display, say —
        // breaks the pipe, but its exit status is what says why.
        if !status.success() {
            return Err(Error::Failed(status));
        }
        written.map_err(Error::Io)
    };

    tokio::time::timeout(COPY_TIMEOUT, write_and_wait)
        .await
        .map_err(|_| Error::TimedOut)?
}

/// The candidate tools for this platform, most specific first.
fn tools() -> Vec<(&'static str, &'static [&'static str])> {
    if cfg!(target_os = "macos") {
        return vec![("pbcopy", &[])];
    }

    let mut tools: Vec<(&'static str, &'static [&'static str])> = Vec::new();
    if env::var_os("WAYLAND_DISPLAY").is_some() {
        tools.push(("wl-copy", &[]));
    }
    tools.push(("xclip", &["-selection", "clipboard"]));
    tools.push(("xsel", &["--clipboard", "--input"]));
    tools
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;

    /// Writes an executable shell script into a fresh directory of its own.
    fn script(name: &str, body: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("protui-clipboard-{name}-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temp directory");
        let path = directory.join("tool");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("chmod");
        path
    }

    #[tokio::test]
    async fn the_text_arrives_whole_on_stdin() {
        let tool = script("stdin", r#"cat > "$1""#);
        let received = tool.with_file_name("received");
        let text =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM+XgkeZS/lofu0u1xq0g4DQUZIOxGcdSqHhQbjKwVEQ me";

        pipe(&tool, &[received.to_str().expect("utf-8 path")], text)
            .await
            .expect("copy succeeds");

        assert_eq!(fs::read_to_string(&received).expect("received"), text);
    }

    #[tokio::test]
    async fn a_failing_tool_is_reported() {
        // It exits without reading, so the write breaks the pipe; the exit
        // status must still be what is reported.
        let tool = script("fails", "exit 3");

        // More than a pipe buffer holds, so the write always breaks.
        let text = "x".repeat(1 << 20);

        let error = pipe(&tool, &[], &text).await.expect_err("the tool fails");

        assert!(
            matches!(error, Error::Failed(status) if status.code() == Some(3)),
            "{error:?}"
        );
    }

    /// Writes the real clipboard and reads it back. It replaces whatever the
    /// clipboard held, so it only runs when asked for — in CI's Linux
    /// container, under Xvfb:
    ///
    /// ```sh
    /// cargo test real_clipboard -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "overwrites the system clipboard"]
    async fn the_real_clipboard_round_trips() {
        let text = format!("protui clipboard test {}", std::process::id());

        copy(&text).await.expect("copy succeeds");

        let paste: &[&str] = if cfg!(target_os = "macos") {
            &["pbpaste"]
        } else if env::var_os("WAYLAND_DISPLAY").is_some() {
            &["wl-paste", "--no-newline"]
        } else {
            &["xclip", "-selection", "clipboard", "-out"]
        };
        let output = std::process::Command::new(paste[0])
            .args(&paste[1..])
            .output()
            .expect("paste tool");
        assert_eq!(String::from_utf8_lossy(&output.stdout), text);
    }
}
