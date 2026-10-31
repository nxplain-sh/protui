//! Project automation for protui, run as `cargo xtask <task>`.
//!
//! The checks CI runs, the pre-commit hook and a contributor's local run all
//! go through these tasks, so the three cannot drift apart. It needs nothing
//! beyond cargo itself; prettier is fetched through `npx` and cargo-deny is
//! optional locally.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  check         everything CI runs: lint, format-check, test, audit
  lint          rustfmt --check, then clippy with warnings as errors
  test          the test suite, for every package in the workspace
  fmt           rustfmt the Rust sources, prettier the Markdown, JSON, YAML
  format-check  verify prettier formatting without writing
  audit         cargo-deny: advisories, licences, bans, sources
  coverage      test coverage with cargo-llvm-cov; HTML in target/llvm-cov/html
  preview       render every screen with colour (PROTUI_PREVIEW_LIGHT=1 for light)
  build         optimised binary in target/release, stamped with git describe
  install       install into ~/.cargo/bin, stamped with git describe
  pre-commit    the checks a staged change calls for; run by .githooks/pre-commit
  hooks         point git at .githooks so pre-commit runs on every commit";

fn main() -> ExitCode {
    let task = env::args().nth(1);
    let result = match task.as_deref() {
        Some("check") => check(),
        Some("lint") => lint(),
        Some("test") => test(),
        Some("fmt") => fmt(),
        Some("format-check") => format_check(),
        Some("audit") => audit(),
        Some("coverage") => coverage(),
        Some("preview") => cargo(&[
            "test",
            "--locked",
            "preview",
            "--",
            "--ignored",
            "--nocapture",
        ]),
        Some("build") => build(),
        Some("install") => install(),
        Some("pre-commit") => pre_commit(),
        Some("hooks") => run("git", &["config", "core.hooksPath", ".githooks"]),
        None | Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprintln!("xtask: unknown task {other:?}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("\nxtask: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// A task step that did not succeed.
#[derive(Debug)]
struct Failure(String);

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn check() -> Result<(), Failure> {
    lint()?;
    format_check()?;
    test()?;
    audit()
}

fn lint() -> Result<(), Failure> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])
}

fn test() -> Result<(), Failure> {
    cargo(&["test", "--workspace", "--locked"])
}

fn fmt() -> Result<(), Failure> {
    cargo(&["fmt", "--all"])?;
    run("npx", &["--yes", "prettier", "--write", "."])
}

fn format_check() -> Result<(), Failure> {
    run("npx", &["--yes", "prettier", "--check", "."])
}

/// Runs cargo-deny. It is optional for a contributor, so its absence is noted
/// and skipped, but never in CI, where a skipped audit would read as a pass.
fn audit() -> Result<(), Failure> {
    if subcommand_installed("deny") {
        cargo(&["deny", "check"])
    } else if env::var_os("CI").is_some() {
        Err(Failure("cargo-deny is not installed".to_owned()))
    } else {
        eprintln!("\ncargo-deny not installed; skipping (cargo install --locked cargo-deny)");
        Ok(())
    }
}

/// Measures test coverage across the workspace: a summary in the terminal,
/// and a browsable report in `target/llvm-cov/html`.
fn coverage() -> Result<(), Failure> {
    if !subcommand_installed("llvm-cov") {
        return Err(Failure(
            "cargo-llvm-cov is not installed (cargo install --locked cargo-llvm-cov)".to_owned(),
        ));
    }

    // The instrumentation needs LLVM's tools for the toolchain in use; adding
    // a component that is already there is a no-op.
    run("rustup", &["component", "add", "llvm-tools-preview"])?;
    cargo(&["llvm-cov", "--workspace", "--locked"])?;
    cargo(&["llvm-cov", "report", "--html"])
}

fn build() -> Result<(), Failure> {
    cargo_with_version(&["build", "--release", "--locked"])
}

fn install() -> Result<(), Failure> {
    cargo_with_version(&["install", "--path", ".", "--locked"])
}

/// Runs the checks the staged change calls for, fastest first, so an obvious
/// failure does not wait behind the test suite. Every check is workspace-wide;
/// the staged paths decide only whether a check runs.
fn pre_commit() -> Result<(), Failure> {
    let staged = output(
        "git",
        &["diff", "--cached", "--name-only", "--diff-filter=ACMRT"],
    )?;
    let touched = Touched::of(staged.lines());

    if touched.rust {
        cargo(&["fmt", "--all", "--check"])?;
    }
    if touched.docs || touched.prettier_config {
        // Only files git is tracking, so an untracked draft cannot block a
        // commit that does not include it. A prettier config change can
        // reformat any of them, so then every tracked document is checked.
        let candidates = if touched.prettier_config {
            output("git", &["ls-files"])?
        } else {
            staged.clone()
        };
        let mut args = vec!["--yes", "prettier", "--check", "--ignore-unknown"];
        args.extend(candidates.lines().filter(|path| is_doc(path)));
        run("npx", &args)?;
    }
    if touched.rust {
        cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ])?;
        test()?;
    }
    if touched.dependencies {
        audit()?;
    }
    Ok(())
}

/// Which kinds of file a change touches.
#[derive(Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per kind of check, each set on its own"
)]
struct Touched {
    rust: bool,
    docs: bool,
    prettier_config: bool,
    dependencies: bool,
}

impl Touched {
    fn of<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut touched = Self::default();
        for path in paths {
            let manifest =
                matches!(path, "Cargo.toml" | "Cargo.lock") || path.ends_with("/Cargo.toml");
            touched.dependencies |= manifest || path == "deny.toml";
            touched.rust |= manifest
                || extension_is(path, &["rs"])
                // Fixtures are read by the parser tests.
                || path.starts_with("src/passcli/testdata/");
            touched.docs |= is_doc(path);
            touched.prettier_config |= matches!(path, ".prettierrc" | ".prettierignore");
        }
        touched
    }
}

/// Reports whether prettier formats `path`.
fn is_doc(path: &str) -> bool {
    extension_is(path, &["md", "json", "yml", "yaml"])
}

/// Reports whether `path` has one of `extensions`, ignoring case.
fn extension_is(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.iter().any(|e| extension.eq_ignore_ascii_case(e)))
}

/// Runs cargo with `PROTUI_VERSION` set from `git describe`, so the binary
/// reports the commit it was built from.
fn cargo_with_version(args: &[&str]) -> Result<(), Failure> {
    let version = output("git", &["describe", "--tags", "--always", "--dirty"])
        .map(|described| described.trim().to_owned())
        .unwrap_or_default();

    let mut command = Command::new(cargo_program());
    command.args(args);
    if !version.is_empty() {
        command.env("PROTUI_VERSION", version);
    }
    status(&mut command, &cargo_program(), args)
}

fn cargo(args: &[&str]) -> Result<(), Failure> {
    run(&cargo_program(), args)
}

fn run(program: &str, args: &[&str]) -> Result<(), Failure> {
    status(Command::new(program).args(args), program, args)
}

/// Runs a prepared command from the workspace root, echoing it first.
fn status(command: &mut Command, program: &str, args: &[&str]) -> Result<(), Failure> {
    // Cargo arrives as a full toolchain path; its name is what a reader needs.
    let shown = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    eprintln!("\n> {shown} {}", args.join(" "));

    let status = command
        .current_dir(workspace_root())
        .status()
        .map_err(|error| Failure(format!("could not run {program}: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Failure(format!(
            "`{program} {}` failed ({status})",
            args.join(" ")
        )))
    }
}

/// Runs a command quietly and returns its stdout.
fn output(program: &str, args: &[&str]) -> Result<String, Failure> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workspace_root())
        .output()
        .map_err(|error| Failure(format!("could not run {program}: {error}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(Failure(format!(
            "`{program} {}` failed ({})",
            args.join(" "),
            output.status
        )))
    }
}

/// Reports whether a cargo subcommand such as `deny` is installed.
fn subcommand_installed(name: &str) -> bool {
    Command::new(cargo_program())
        .args([name, "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The cargo that is running this task, so a `cargo +toolchain xtask` keeps
/// its toolchain for every step.
fn cargo_program() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_docs_only_change_runs_no_rust_checks() {
        let touched = Touched::of(["README.md", "docs/adr/0014-rewrite-in-rust-with-ratatui.md"]);

        assert_eq!(
            touched,
            Touched {
                docs: true,
                ..Touched::default()
            }
        );
    }

    #[test]
    fn source_and_fixture_changes_run_the_rust_checks() {
        assert!(Touched::of(["src/ui/view.rs"]).rust);
        assert!(Touched::of(["src/passcli/testdata/vault_list.json"]).rust);
        assert!(Touched::of(["xtask/src/main.rs"]).rust);
    }

    #[test]
    fn manifest_changes_also_run_the_audit() {
        let touched = Touched::of(["Cargo.lock"]);

        assert!(touched.dependencies && touched.rust, "{touched:?}");
        assert!(Touched::of(["xtask/Cargo.toml"]).dependencies);
        let policy = Touched::of(["deny.toml"]);
        assert!(policy.dependencies && !policy.rust, "{policy:?}");
    }

    #[test]
    fn prettier_config_changes_recheck_the_whole_tree() {
        assert!(Touched::of([".prettierrc"]).prettier_config);
        assert!(!Touched::of(["README.md"]).prettier_config);
        assert!(!Touched::of(["LICENSE"]).docs);
    }
}
