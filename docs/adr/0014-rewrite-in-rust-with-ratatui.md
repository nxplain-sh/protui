# Rewrite in Rust with ratatui

- **Status:** accepted
- **Date:** 2026-10-30

## Context

protui was first written in Go on Bubble Tea, bubbles and lipgloss. The
decision here is to replace that implementation with a Rust one built on
[ratatui](https://ratatui.rs), keeping the behaviour, the key bindings, the
layout and every security property the earlier records describe.

What Rust offers this project in particular, as of this date:

- **Secrets can be scoped by ownership.** Go strings are immutable, so the
  passphrase environment entry could not be wiped (see
  [pass passphrases through the environment](0005-pass-passphrases-through-the-environment.md)).
  In Rust the passphrase can live in one owned, zeroing buffer that is _moved_
  from the form into the request and on into the client, so no second copy is
  made along the way.
- **Mistakes on the delete path become type errors.** Share ids and item ids
  are both base64 strings in Go; as distinct Rust types, passing one where the
  other belongs does not compile.
- **Memory safety is a compiler guarantee.** The crate can `forbid(unsafe_code)`
  and still parse untrusted public keys and JSON.
- **Upstream is Rust.** `pass-cli` serialises with serde; decoding with serde
  makes field and casing questions in [`../schema.md`](../schema.md) a
  question about one library.

The alternatives considered were to stay on Go, which keeps a working program
and loses the above, and to write a Rust UI on a lower-level library such as
crossterm alone, which would mean reimplementing layout, styling and diffing
that ratatui already provides.

ratatui is immediate-mode: it redraws from state each frame and diffs the
result, rather than threading a model through `Update`/`View` the way Bubble
Tea does. It has no built-in list-with-filter, text input or help bar, so those
are small modules of protui's own.

## Decision

We will implement protui in Rust, on ratatui with the crossterm backend and a
single-threaded tokio runtime, and remove the Go implementation.

The architecture carries over, translated:

- `src/passcli` is the only module that runs `pass-cli`, as `internal/passcli`
  was, on `tokio::process` with a timeout that kills the child.
- The update loop stays free of I/O. `Model::update` returns `Command` values —
  plain data, not closures — and the runtime runs each on its own task and
  posts the result back as a `Message`.
- Sanitising stays at the parse boundary in `src/keys/sanitize.rs`.

Several gaps the Go implementation had are closed along the way: every value
passed to `pass-cli` is attached to its flag as `--flag=value`; the passphrase
variables are stripped from the environment the child inherits; results from a
load that a refresh has superseded are dropped by a generation counter; and
quitting waits for a change that is still in flight rather than killing
`pass-cli` halfway through it.

Bubble Tea enabled bracketed paste and handled `SIGTERM` for free; crossterm
does neither by default, so protui turns on bracketed paste itself — a paste
otherwise arrives as key presses, each one a possible action — and quits
cleanly on `SIGTERM`.

## Consequences

The security properties are stronger, not merely equal: one zeroing buffer for
the passphrase, distinct id types, no `unsafe`, and argument values that cannot
be read as flags. The one remaining copy of the passphrase that protui cannot
wipe is the environment block the standard library builds while spawning.

Commands being data makes the UI tests sharper. A Go test could only assert
that _some_ command came back; a Rust test asserts it was a delete of item
`item-1` in share `share-1`.

Contributors need a Rust toolchain instead of a Go one, and the minimum
supported Rust — `rust-version` in `Cargo.toml` — is now the version that
decides who can build protui. CI tests against it.

protui now owns code it used to borrow from bubbles: a text input, a filtered
list and a help bar. They are small, and tested, but they are protui's to
maintain.

`cargo-deny` replaces `govulncheck` and `gosec`; clippy with its pedantic lints
replaces golangci-lint.

The earlier records that name Go APIs carry a dated note pointing at their Rust
equivalents rather than being rewritten, so the reasoning that led to them is
kept.
