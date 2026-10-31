# Isolate pass-cli behind one package

- **Status:** accepted
- **Date:** 2026-08-31

## Context

Having chosen to
[wrap pass-cli](0001-wrap-pass-cli-instead-of-the-proton-api.md), protui's
correctness now depends on an output format that upstream does not treat as a
contract. Two things follow from that:

- When upstream changes, we want a single place to fix.
- The security rules protui must honour — no secret in argv, never call the
  command that returns private keys, never pass `--show-secrets` — are all
  properties of _how the subprocess is invoked_. A rule that must hold at every
  call site holds nowhere.

The natural drift is for `os/exec` calls to spread: the UI needs a vault list,
so it runs one; the detail pane needs a public key, so it runs another. Each
call site then has to independently remember every security rule.

## Decision

We will confine all `os/exec` use to `internal/passcli`, which exposes one typed
function per operation. No other package imports `os/exec` or knows that a
subprocess exists.

`internal/keys` holds domain types that are deliberately _not_ shaped like the
upstream JSON, so translation happens at the boundary rather than leaking
outward.

The security invariants are stated in the package doc comment and enforced by
tests that inspect the constructed argument list.

## Consequences

The security rules become testable properties of one package rather than review
comments. There are tests asserting that no wrapper passes `--show-secrets`,
that the passphrase never appears in argv, and that no argument names the
private key.

Upstream changes are contained. A field rename touches the wire structs; a
command rename touches one function.

The UI cannot take shortcuts. Anything it needs must be a named operation with
a signature, which makes the full surface protui uses visible in one place.

The indirection costs something for one-off needs: adding a command means adding
a wrapper, a return type, and a translation, rather than a two-line exec call.
That cost is the point.

A test seam exists so the wrappers can be exercised without a real binary. It
caught a real bug: `Preflight` passed its subcommand as an error label but never
as an argument, so it ran bare `pass-cli`, exited non-zero, and reported "no
valid session" on every launch. There is now a test asserting every wrapper's
subcommand reaches argv.

## Amended 2026-10-30

protui is now Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). The package is the module `src/passcli`, and
`os/exec` is `tokio::process`; the rule is unchanged — nothing outside
`src/passcli` runs `pass-cli`. The one other subprocess protui starts is the
platform clipboard tool in `src/clipboard.rs`, which the Go version also ran,
inside its clipboard library.

The test seam is a fake runner compiled only into tests, rather than a function
field, and every wrapper now passes values as `--flag=value` so that none can
be read as a flag.
