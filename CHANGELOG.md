# Changelog

All notable changes to protui are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing has been released yet. Everything below is the initial feature set,
pending a first tagged version.

### Added

- List SSH key items across every vault, with title, vault, algorithm and
  fingerprint. `pass-cli` cannot list across vaults, so protui queries each one
  and reports failures per vault rather than blanking the list.
- Detail pane showing the public key, fingerprint, comment, vault, timestamps
  and item ID. Long values wrap rather than truncate, and the pane scrolls with
  `Ctrl-e` / `Ctrl-y` when it does not fit.
- Generate a key: `ed25519` (default), `rsa2048` or `rsa4096`, with a title,
  vault, optional comment and optional passphrase.
- Copy the selected public key to the clipboard with `c` (or `y`).
- Move a key to the trash with `d`, recoverable via `pass-cli item untrash`.
- Permanently delete with `D`, guarded by typing the item's exact title.
- SSH agent daemon panel: status, start and stop.
- Fuzzy filter over title, vault, comment and fingerprint.
- vim navigation — `j`/`k`, `gg`/`G`, `Ctrl-f`/`Ctrl-b`, `Ctrl-d`/`Ctrl-u` —
  with lazygit-style single-letter actions and a help bar.
- Startup preflight that reports a missing or unauthenticated `pass-cli` as
  plain text, before the terminal is taken over.

### Changed

- Rewritten in Rust, on [ratatui](https://ratatui.rs) and tokio, replacing the
  Go and Bubble Tea implementation. Behaviour, keys and layout are unchanged
  except where noted here. See
  [the decision record](docs/adr/0014-rewrite-in-rust-with-ratatui.md).
- `rust-toolchain.toml` pins the latest stable Rust, 1.99.1, for development,
  CI and releases, and Renovate keeps it current. The minimum supported Rust
  stays 1.88 and is tested separately.
- `cargo xtask coverage` reports line coverage with cargo-llvm-cov, replacing
  the Go version's `make cover`.
- Project automation moved from the Makefile to `cargo xtask`, a std-only
  crate in the workspace. CI, the pre-commit hook and a local run all call the
  same tasks; `cargo xtask help` lists them.
- `Ctrl-c` quits from every screen, including the dialogs, so none can trap
  the user. Quitting while a create, trash, delete or agent action is still
  running waits for it to finish, so its outcome is never left unknown; asking
  twice quits at once. `SIGTERM` quits cleanly, restoring the terminal.
- A paste arrives as one piece of text and goes only into the focused input,
  so pasting into the list can no longer trigger actions letter by letter.
- Status messages show inside dialogs too, so a mistyped delete confirmation
  is reported where it happened.
- The delete confirmation accepts titles longer than 128 characters.
- The vault list is fetched alongside the startup session check rather than
  after it, taking one `pass-cli` round trip off every launch. A missing
  session is still reported before the terminal is taken over.
- Keys typed while protui is still starting are kept rather than discarded,
  so a `q` pressed during the session check quits as soon as the UI appears.
- A clipboard tool that fails — `xclip` with no display, say — is reported by
  its exit status rather than as a broken pipe.
- Results from a load that a refresh has superseded are dropped, so pressing
  `r` twice in quick succession no longer lists keys twice.
- Colours fall back to the 256-colour palette on terminals without 24-bit
  colour, and `NO_COLOR` is honoured.

### Security

- Private key material never reaches protui. `pass-cli item view --output json`
  returns it unredacted and is never called; public keys are read with
  `item view --field public_key`.
- Passphrases are passed through the child environment, never as command-line
  arguments, which are world-readable via `ps`. The passphrase lives in a
  single pre-allocated buffer that is zeroed once the child has it.
- Every value is passed as `--flag=value`, so a title or comment that begins
  with `-` cannot be read by `pass-cli` as a flag.
- Share ids and item ids are distinct types, so the one can never be passed
  where the other is expected — on the delete path, that would destroy the
  wrong item.
- `PROTON_PASS_SSH_KEY_PASSWORD` and `PROTON_PASS_SSH_KEY_PASSWORD_FILE` are
  removed from the environment `pass-cli` inherits, so a variable exported in
  the user's shell can never silently encrypt a key they left without a
  passphrase.
- `PATH` entries that are not absolute are ignored when finding `pass-cli` and
  the clipboard tools, so a binary in the working directory cannot stand in
  for them.
- The crate forbids `unsafe` code, and CI checks dependencies against the
  RustSec advisory database with `cargo-deny`.
- `--show-secrets` is never passed.
- Text from Proton Pass — item titles, vault names, key comments — is sanitised
  before being drawn. Shared vaults mean these can be authored by someone else,
  and terminal escape sequences in them would otherwise be executed rather than
  displayed, including OSC 52, which writes the system clipboard.

### Documentation

- [`docs/schema.md`](docs/schema.md) records the `pass-cli` output protui
  parses, verified against both the upstream source and a live binary.
- [`docs/adr/`](docs/adr/README.md) records the architecture decisions, with the
  alternatives considered and what each choice cost.

[unreleased]: https://github.com/nxplain-sh/protui/commits/main
