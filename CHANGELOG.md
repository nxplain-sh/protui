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

### Security

- Private key material never reaches protui. `pass-cli item view --output json`
  returns it unredacted and is never called; public keys are read with
  `item view --field public_key`.
- Passphrases are passed through the child environment, never as command-line
  arguments, which are world-readable via `ps`.
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
