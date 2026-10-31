//! Project automation for protui, run as `cargo xtask <task>`.
//!
//! The checks CI runs, the pre-commit hook and a contributor's local run all
//! go through these tasks, so the three cannot drift apart. It needs nothing
//! beyond cargo itself; prettier is fetched through `npx` and cargo-deny is
//! optional locally.

use std::env;
use std::fmt;
use std::fs;
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
  site <dir> [--tag <tag> --date <yyyy-mm-dd>]
                the website from site/, with the release written in if given
  formula <tag> <checksums.txt>
                the Homebrew formula for a release, on stdout
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
        Some("site") => site(&env::args().skip(2).collect::<Vec<_>>()),
        Some("formula") => formula(&env::args().skip(2).collect::<Vec<_>>()),
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

/// The targets `release.yaml` builds, and the Homebrew blocks each one is
/// installed under.
const FORMULA_TARGETS: [(&str, &str, &str); 4] = [
    ("aarch64-apple-darwin", "macos", "arm"),
    ("x86_64-apple-darwin", "macos", "intel"),
    ("aarch64-unknown-linux-musl", "linux", "arm"),
    ("x86_64-unknown-linux-musl", "linux", "intel"),
];

/// Prints the Homebrew formula for a release, from its tag and the
/// `checksums.txt` published beside its archives.
fn formula(args: &[String]) -> Result<(), Failure> {
    let [tag, checksums] = args else {
        return Err(Failure(
            "usage: cargo xtask formula <tag> <checksums.txt>".to_owned(),
        ));
    };
    let checksums = fs::read_to_string(checksums)
        .map_err(|error| Failure(format!("could not read {checksums}: {error}")))?;

    print!("{}", render_formula(tag, &checksums)?);
    Ok(())
}

/// Writes the formula: one prebuilt archive per platform, each with its
/// SHA-256. A formula rather than a cask, so it installs on Linux too, and
/// so the unsigned binary is not quarantined on macOS.
fn render_formula(tag: &str, checksums: &str) -> Result<String, Failure> {
    // The version lands inside Ruby strings, so it is held to what a
    // version can contain.
    let version = tag
        .strip_prefix('v')
        .filter(|version| {
            !version.is_empty()
                && version
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        })
        .ok_or_else(|| Failure(format!("{tag:?} is not a release tag like v1.2.3")))?;

    let mut platforms = Vec::new();
    for os in ["macos", "linux"] {
        platforms.push(format!("  on_{os} do"));
        for &(target, _, arch) in FORMULA_TARGETS.iter().filter(|(_, o, _)| *o == os) {
            let archive = format!("protui_{version}_{target}.tar.gz");
            let sha256 = archive_checksum(checksums, &archive)?;
            platforms.push(format!(
                "    on_{arch} do\n      url \"https://github.com/nxplain-sh/protui/releases/download/{tag}/{archive}\"\n      sha256 \"{sha256}\"\n    end"
            ));
        }
        platforms.push("  end\n".to_owned());
    }
    let platforms = platforms.join("\n");

    Ok(format!(
        r##"# Written by `cargo xtask formula` in nxplain-sh/protui for each release.
# Edits here are overwritten by the next one.
class Protui < Formula
  desc "Unofficial terminal UI for Proton Pass SSH keys"
  homepage "https://nxplain-sh.github.io/protui/"
  license "MIT"

{platforms}
  def install
    bin.install "protui"
  end

  def caveats
    <<~EOS
      protui drives the official Proton Pass CLI, which must be installed
      and logged in:
        brew install protonpass/tap/pass-cli
        pass-cli login
    EOS
  end

  test do
    assert_match "protui #{{version}}", shell_output("#{{bin}}/protui --version")
  end
end
"##
    ))
}

/// Finds `archive`'s SHA-256 in `sha256sum` output.
fn archive_checksum<'a>(checksums: &'a str, archive: &str) -> Result<&'a str, Failure> {
    checksums
        .lines()
        .find_map(|line| {
            let (sum, name) = line.split_once(char::is_whitespace)?;
            (name.trim_start_matches([' ', '*']) == archive).then_some(sum)
        })
        .filter(|sum| sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| Failure(format!("checksums.txt has no SHA-256 for {archive}")))
}

/// Text the page held before release injection existed. Neither may ever be
/// published: a page without a release hides its version chips instead.
const RELEASE_PLACEHOLDERS: [&str; 2] = ["vX.Y.Z", "YYYY-MM-DD"];

/// The release a built site names.
#[derive(Debug, PartialEq, Eq)]
struct Release {
    tag: String,
    date: String,
}

/// Builds the website into a new directory: a copy of `site/`, with the
/// release's tag and date written into the page when one is given. Without a
/// release the page keeps its version chips hidden.
fn site(args: &[String]) -> Result<(), Failure> {
    let (out, release) = site_args(args)?;
    let out = PathBuf::from(out);

    // A new directory only, so a mistyped path never overwrites anything.
    fs::create_dir(&out).map_err(|error| {
        Failure(format!(
            "could not create {}: {error} (it must not exist yet)",
            out.display()
        ))
    })?;
    copy_dir(&workspace_root().join("site"), &out)?;

    let index = out.join("index.html");
    let page = fs::read_to_string(&index)
        .map_err(|error| Failure(format!("could not read {}: {error}", index.display())))?;
    let page = match &release {
        Some(release) => stamp(&page, release)?,
        None => page,
    };
    if let Some(placeholder) = RELEASE_PLACEHOLDERS.iter().find(|p| page.contains(**p)) {
        return Err(Failure(format!(
            "site/index.html shows the placeholder {placeholder:?}"
        )));
    }
    fs::write(&index, page)
        .map_err(|error| Failure(format!("could not write {}: {error}", index.display())))?;

    match release {
        Some(release) => eprintln!("built {} for {}", out.display(), release.tag),
        None => eprintln!("built {} with no release", out.display()),
    }
    Ok(())
}

/// Reads `<dir> [--tag <tag> --date <yyyy-mm-dd>]`.
fn site_args(args: &[String]) -> Result<(&str, Option<Release>), Failure> {
    let usage =
        || Failure("usage: cargo xtask site <dir> [--tag <tag> --date <yyyy-mm-dd>]".to_owned());

    let (out, mut rest) = args.split_first().ok_or_else(usage)?;
    let (mut tag, mut date) = (None, None);
    while let [flag, value, more @ ..] = rest {
        match flag.as_str() {
            "--tag" => tag = Some(value.clone()),
            "--date" => date = Some(value.clone()),
            _ => return Err(usage()),
        }
        rest = more;
    }
    if !rest.is_empty() || out.starts_with('-') {
        return Err(usage());
    }

    let release = match (tag, date) {
        (None, None) => None,
        (Some(tag), Some(date)) if !tag.is_empty() && is_date(&date) => Some(Release { tag, date }),
        _ => {
            return Err(Failure(
                "--tag and --date go together, the date as yyyy-mm-dd".to_owned(),
            ));
        }
    };
    Ok((out, release))
}

fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.char_indices().all(|(index, c)| match index {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        })
}

/// Copies every file under `from` into `to`, which exists.
fn copy_dir(from: &Path, to: &Path) -> Result<(), Failure> {
    let fail = |path: &Path, error: std::io::Error| Failure(format!("{}: {error}", path.display()));

    for entry in fs::read_dir(from).map_err(|error| fail(from, error))? {
        let path = entry.map_err(|error| fail(from, error))?.path();
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            fs::create_dir(&target).map_err(|error| fail(&target, error))?;
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|error| fail(&path, error))?;
        }
    }
    Ok(())
}

/// Writes `release` into the page: its tag into every element marked
/// `data-release-version`, its date into every `data-release-date`, and the
/// `hidden` attribute off every `data-release-chip`.
///
/// The page is the project's own hand-written HTML, so a scan of its tags is
/// enough; a marked element holds text only, up to the next tag.
fn stamp(page: &str, release: &Release) -> Result<String, Failure> {
    let unclosed = || Failure("site/index.html: a tag is never closed".to_owned());
    let mut out = String::with_capacity(page.len());
    let mut rest = page;
    let (mut versions, mut dates) = (0, 0);

    while let Some(open) = rest.find('<') {
        let close = open + rest[open..].find('>').ok_or_else(unclosed)?;
        out.push_str(&rest[..open]);
        let tag = &rest[open..=close];
        let mut parts = attributes(&rest[open + 1..close]);
        rest = &rest[close + 1..];

        if has_attribute(&parts, "data-release-chip") {
            parts.retain(|part| *part != "hidden");
            out.push('<');
            out.push_str(&parts.join(" "));
            out.push('>');
        } else {
            out.push_str(tag);
        }

        let content = if has_attribute(&parts, "data-release-version") {
            versions += 1;
            Some(&release.tag)
        } else if has_attribute(&parts, "data-release-date") {
            dates += 1;
            Some(&release.date)
        } else {
            None
        };
        if let Some(content) = content {
            out.push_str(&escape_html(content));
            rest = &rest[rest.find('<').ok_or_else(unclosed)?..];
        }
    }
    out.push_str(rest);

    if versions == 0 || dates == 0 {
        return Err(Failure(
            "site/index.html has nowhere to show the release version or date".to_owned(),
        ));
    }
    Ok(out)
}

/// Splits the inside of a tag, such as `span class="a b" hidden`, into its
/// name and attributes, keeping quoted values whole.
fn attributes(inside: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = None;
    let mut quote = None;

    for (index, c) in inside.char_indices() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if c.is_whitespace() => {
                if let Some(start) = start.take() {
                    parts.push(&inside[start..index]);
                }
            }
            None => {
                if matches!(c, '"' | '\'') {
                    quote = Some(c);
                }
                start.get_or_insert(index);
            }
        }
    }
    if let Some(start) = start {
        parts.push(&inside[start..]);
    }
    parts
}

fn has_attribute(parts: &[&str], name: &str) -> bool {
    parts
        .iter()
        .any(|part| part.split('=').next() == Some(name))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

    fn checksums(version: &str) -> String {
        FORMULA_TARGETS
            .iter()
            .enumerate()
            .map(|(index, (target, ..))| {
                format!(
                    "{}  protui_{version}_{target}.tar.gz\n",
                    index.to_string().repeat(64)
                )
            })
            .chain([format!("{}  something-else.tar.gz\n", "f".repeat(64))])
            .collect()
    }

    #[test]
    fn the_formula_names_every_archive_with_its_checksum() {
        let formula = render_formula("v1.2.3", &checksums("1.2.3")).expect("a formula");

        for (index, (target, _, arch)) in FORMULA_TARGETS.iter().enumerate() {
            let url = format!(
                "url \"https://github.com/nxplain-sh/protui/releases/download/v1.2.3/protui_1.2.3_{target}.tar.gz\"\n      sha256 \"{}\"",
                index.to_string().repeat(64)
            );
            assert!(formula.contains(&url), "{target} is missing:\n{formula}");
            assert!(formula.contains(&format!("on_{arch} do")));
        }
        assert!(formula.contains("on_macos do") && formula.contains("on_linux do"));
        assert!(formula.contains(r##"shell_output("#{bin}/protui --version")"##));
    }

    #[test]
    fn the_formula_refuses_a_missing_checksum_or_an_odd_tag() {
        let partial: String = checksums("1.2.3").split_inclusive('\n').skip(1).collect();
        assert!(render_formula("v1.2.3", &partial).is_err());
        assert!(render_formula("v1.2.3", &checksums("1.2.4")).is_err());
        for tag in ["1.2.3", "v", "v1.2.3\"; system(\"x\")"] {
            assert!(
                render_formula(tag, &checksums("1.2.3")).is_err(),
                "{tag:?} was accepted"
            );
        }
    }

    fn release() -> Release {
        Release {
            tag: "v1.2.3".to_owned(),
            date: "2026-11-02".to_owned(),
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|&arg| arg.to_owned()).collect()
    }

    #[test]
    fn stamping_fills_every_marked_element_and_shows_the_chips() {
        let page = concat!(
            r#"<a data-release-chip hidden href="x"><span data-release-version></span></a>"#,
            r#"<p data-release-chip hidden style="overflow: hidden">on <span data-release-date></span></p>"#,
            r#"<a>Download <span data-release-chip hidden data-release-version></span></a>"#,
            r#"<p hidden>kept</p>"#,
        );

        let stamped = stamp(page, &release()).expect("the page stamps");

        assert_eq!(
            stamped,
            concat!(
                r#"<a data-release-chip href="x"><span data-release-version>v1.2.3</span></a>"#,
                r#"<p data-release-chip style="overflow: hidden">on <span data-release-date>2026-11-02</span></p>"#,
                r#"<a>Download <span data-release-chip data-release-version>v1.2.3</span></a>"#,
                r#"<p hidden>kept</p>"#,
            )
        );
    }

    #[test]
    fn stamping_escapes_the_release() {
        let page = "<b data-release-version></b><i data-release-date></i>";
        let release = Release {
            tag: "<v1>&".to_owned(),
            ..release()
        };

        let stamped = stamp(page, &release).expect("the page stamps");

        assert!(stamped.contains("&lt;v1&gt;&amp;"), "{stamped}");
    }

    #[test]
    fn stamping_a_page_with_nowhere_to_put_the_release_fails() {
        assert!(stamp("<p>no markers</p>", &release()).is_err());
        assert!(stamp("<p data-release-version>", &release()).is_err());
    }

    /// The real page, with and without a release: neither may show a
    /// placeholder, and without one every version chip stays hidden.
    #[test]
    fn the_site_never_shows_a_placeholder() {
        let page =
            fs::read_to_string(workspace_root().join("site/index.html")).expect("site/index.html");
        let stamped = stamp(&page, &release()).expect("the page stamps");

        for placeholder in RELEASE_PLACEHOLDERS {
            assert!(!page.contains(placeholder), "the page shows {placeholder}");
            assert!(
                !stamped.contains(placeholder),
                "the stamped page shows {placeholder}"
            );
        }
        assert!(stamped.contains(">v1.2.3<") && stamped.contains(">2026-11-02<"));
        assert!(!stamped.contains("data-release-chip hidden"));
        for marked in page
            .split('<')
            .filter(|tag| tag.contains("data-release-chip"))
        {
            assert!(
                has_attribute(
                    &attributes(marked.split('>').next().unwrap_or_default()),
                    "hidden"
                ),
                "{marked}"
            );
        }
    }

    #[test]
    fn site_arguments() {
        assert_eq!(site_args(&strings(&["out"])).ok(), Some(("out", None)));
        assert_eq!(
            site_args(&strings(&[
                "out",
                "--tag",
                "v1.2.3",
                "--date",
                "2026-11-02"
            ]))
            .ok(),
            Some(("out", Some(release())))
        );
        for bad in [
            &[][..],
            &["--tag", "v1"],
            &["out", "--tag", "v1.2.3"],
            &["out", "--tag", "v1.2.3", "--date", "2026-11-02T10:00:00Z"],
            &["out", "--date", "2026-11-02"],
            &["out", "extra"],
        ] {
            assert!(site_args(&strings(bad)).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn prettier_config_changes_recheck_the_whole_tree() {
        assert!(Touched::of([".prettierrc"]).prettier_config);
        assert!(!Touched::of(["README.md"]).prettier_config);
        assert!(!Touched::of(["LICENSE"]).docs);
    }
}
