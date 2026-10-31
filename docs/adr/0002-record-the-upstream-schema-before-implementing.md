# Record the upstream schema before implementing

- **Status:** accepted
- **Date:** 2026-08-31

## Context

`pass-cli` derives `serde::Serialize` directly on its internal Rust domain
structs and prints the result. There is no schema document, no versioning on the
output, and no statement anywhere that the JSON is a supported interface. A
field rename in `pass-domain` silently changes the wire format.

Writing a parser against a guessed shape is the obvious failure mode, and it
fails quietly: a wrong field name decodes to a zero value rather than an error,
so the UI shows an empty list instead of a message.

Guessing would have been wrong in specific, non-obvious ways. Checking the
source and a live binary found, among others:

- SSH key items store no algorithm, fingerprint, or comment field.
- Casing is inconsistent _within a single JSON object_: `item_type` is
  snake_case, `state` and `flags` are PascalCase, and the matching CLI flag is
  kebab-case.
- Timestamps are `jiff::civil::DateTime` — no zone, no offset — so
  `time.RFC3339` does not parse them.
- `item view --output json` returns the private key unredacted.

None of those are guessable. All of them change the implementation.

## Decision

We will maintain `docs/schema.md` as the record of the upstream output contract
protui parses, written _before_ the parsing layer and updated when upstream
changes.

Each claim in it is verified two ways: read from the `pass-cli` Rust source
(authoritative for field names, optionality and serde attributes) and confirmed
against a live binary. Where the two disagree, the binary wins and the
disagreement is noted. The document pins the upstream commit and binary version
it was captured from, and records what could _not_ be verified.

## Consequences

The schema document is a maintenance obligation. It is only worth having if it
is re-verified on upstream bumps, so it carries a section of commands to re-run
and names the fields most likely to drift.

Parser changes now have a written reference to diff against rather than
requiring a re-reading of Rust source each time.

It captures negative knowledge — the things we deliberately do not call, and
why — which the code alone cannot express. `item view --output json` is absent
from the codebase; only the document explains that its absence is deliberate.

The document is honest about its gaps. At capture time the account held no SSH
key items, so the `ssh_key` spelling of `item_type` is read from source rather
than observed. That is recorded rather than glossed over, because a reader who
assumes everything was verified would trust it too much.

## Amended 2026-10-30

The implementation moved from Go to Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). The trap is the same:
an RFC 3339 parser, in any language, rejects the zoneless timestamps.
