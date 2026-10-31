# Fail before taking over the terminal

- **Status:** accepted
- **Date:** 2026-08-31

## Context

protui is useless without `pass-cli` installed and authenticated. Both are
plausible first-run states: the binary may not be installed, or it may be
installed with no session because the user has never run `pass-cli login`.

A TUI reports errors badly. Once the alternate screen buffer is active, an error
is a line inside a full-screen layout that vanishes on exit, cannot be scrolled
back to, and cannot be copied out of a terminal the program has taken over. For
a message whose whole content is a command to run, that is the wrong medium.

Detecting the session is also not free-form: `pass-cli info` exits 0 with a live
session and 1 without, and prints human-readable text with no JSON option. Only
the exit status is contractual — the printed lines are prose that could be
reworded at any time.

## Decision

We will check both prerequisites in `main`, before starting the Bubble Tea
program, and exit with a plain-text message on failure.

`exec.LookPath` covers the binary. `pass-cli info` under a 20-second timeout
covers the session, and only its exit code is consulted — the output is not
parsed.

The two failures produce different messages: one points at the install
instructions, the other at `pass-cli login`. Both are distinguished by sentinel
errors (`ErrNotInstalled`, `ErrNoSession`) rather than by string matching.

## Consequences

The most likely first-run failure produces a message the user can read, copy,
and act on, in a terminal that still behaves normally.

Startup costs one extra subprocess. That is acceptable for the launch path and
is the same call the user would run to diagnose the problem by hand.

Session state can still lapse mid-session — a token expiring, a session being
locked from elsewhere. The preflight does not prevent that; those surface as
ordinary command failures inside the UI, carrying upstream's own stderr text.
The preflight is about the common startup case, not a guarantee for the
program's lifetime.

`Session has lock: no` in the `info` output indicates a lock state we
deliberately do not parse. Unlocking is interactive and out of scope, so a
locked session presents as a command failure rather than a special startup path.

Not parsing `info` means protui cannot show the logged-in account in its
header. That would require depending on prose formatting, which is exactly what
this decision avoids; the trade is accepted.

## Amended 2026-10-30

protui is now Rust on ratatui ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). The checks run in `main` before
`ratatui::try_init`, and the binary is found by a `PATH` search that skips
relative entries, so a binary in the working directory cannot stand in for
`pass-cli`.

## Amended 2026-10-31

The vault list now starts alongside `pass-cli info` instead of after it. It
needs a session but not the check's verdict, and running the two together
finishes the pair in about a third of the time against a live account. The
decision is unchanged: the check must pass before the terminal is taken over,
and if it fails the vault list is dropped unread.
