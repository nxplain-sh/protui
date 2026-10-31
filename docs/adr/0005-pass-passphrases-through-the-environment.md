# Pass passphrases through the environment

- **Status:** accepted
- **Date:** 2026-08-31

## Context

Generating a passphrase-protected SSH key means getting a secret from protui to
`pass-cli`. That secret is the only one protui ever handles.

`pass-cli` accepts it three ways
(`pass-cli/src/commands/item/create/ssh_key.rs:274-313`, upstream `51a4c9b1`),
resolved in this order:

1. `PROTON_PASS_SSH_KEY_PASSWORD` — used if set, **regardless of whether
   `--password` was passed**.
2. `PROTON_PASS_SSH_KEY_PASSWORD_FILE` — a path; contents are trimmed.
3. `--password` with neither variable set — an **interactive TTY prompt**,
   asked twice with confirmation, looping until the two match.

There is no flag that takes the passphrase as an argument value, which is
fortunate, because argv is world-readable: any user on the machine can read a
process's full command line from `ps`. A secret in argv is a secret published to
every local account for the lifetime of the process.

Option 3 is actively dangerous for a TUI. Bubble Tea owns the terminal; a child
process reading from the same TTY contends with it for input. The child would
block on a prompt the user cannot see, and the UI would hang with no way out.

Option 2 means writing the secret to the filesystem, even briefly.

## Decision

We will pass the passphrase to `pass-cli` in the child process environment as
`PROTON_PASS_SSH_KEY_PASSWORD`, and we will never pass `--password`.

Because upstream checks the variable before it consults the flag, setting the
variable alone is sufficient and cannot fall through to the prompt. Omitting the
flag means that even if the variable somehow failed to reach the child, the
result is a key without a passphrase — not a hung terminal.

The passphrase crosses the API as `[]byte` so the caller's buffer can be wiped
once the environment entry is built.

## Consequences

The secret never appears in argv, never touches disk, and never reaches a log.
The TUI cannot be deadlocked by a child prompting for input.

The wipe is partial, and the README says so rather than overclaiming. The
`[]byte` is zeroed, but the environment entry built from it is a Go string,
which is immutable and cannot be explicitly cleared; it survives until garbage
collected. What is achievable is done; what is not is documented.

The child environment is visible to the process itself and to a sufficiently
privileged observer (`/proc/<pid>/environ` on Linux, root or same-user). This is
weaker than a pipe on a dedicated file descriptor, but it is the strongest
mechanism upstream offers, and it is the one upstream documents.

Tests pin all of this: that the passphrase appears in exactly one environment
entry and no argv element, that `--password` is never passed whether or not a
passphrase is set, and that the caller's buffer is zeroed on return.

`PROTON_PASS_NO_UPDATE_CHECK=1` is also set on every invocation, unrelated to
secrecy — it suppresses an auto-update probe that adds latency and can write
unexpected lines to the output being parsed.

## Amended 2026-10-30

protui is now Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)), which changes the wiping story for the better.
The passphrase lives in one buffer, allocated at full size before the first
keystroke so that typing never reallocates and strands a copy, and that buffer
is moved — not copied — from the form to the request to the client, and zeroed
when the request is dropped. The remaining copy protui cannot reach is the
environment block the standard library builds while spawning the child; it is
freed, unzeroed, as soon as the child has started.
