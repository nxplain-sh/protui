# Tolerate unknown JSON fields but validate required ones

- **Status:** accepted
- **Date:** 2026-08-31

## Context

protui parses output that upstream does not treat as a contract. Two distinct
kinds of change can arrive:

- **Additive.** Upstream adds a field. Our decoder does not know about it.
- **Subtractive.** Upstream renames or removes a field we read.

These need opposite handling, and Go's default gets one right and one wrong.

`encoding/json` ignores unknown fields by default, so additive changes are
already harmless. Calling `DisallowUnknownFields` would turn every upstream
addition into a hard failure — the listing would break because a field we do not
use appeared.

Subtractive changes are the dangerous ones, and they are silent. A renamed
`title` decodes to `""`. A renamed `id` decodes to `""`. Nothing errors; the
list renders with blank rows, or renders items that cannot be acted on because
their identifier is empty. The failure surfaces later, as a confusing error from
a command called with an empty `--item-id`, or as nothing at all.

## Decision

We will not call `DisallowUnknownFields`, so unknown fields are ignored, and we
will explicitly validate that required fields are non-empty after decoding.

A missing `share_id` on a vault, or a missing `id` or `title` on an item, is
reported as `ErrUnexpectedSchema` naming the offending index or item — not
skipped, and not allowed through as a zero value.

Decode failures are wrapped in a `CommandError` naming the `pass-cli` subcommand
that produced the output, so the user sees which call broke rather than a bare
JSON error.

## Consequences

An upstream field addition cannot break protui. An upstream field removal fails
loudly at the boundary, close to the cause, with the schema document to check
against.

Validation is a judgement call about which fields are load-bearing. `share_id`
and `id` are required because every subsequent command needs them; `vault_id` is
not, because nothing accepts it. That list needs revisiting if the set of
commands changes.

Not every unexpected value is an error. An unrecognised item `state` maps to
active rather than failing, because keeping an item visible and actionable is
better than dropping it over a label we do not recognise. An unparseable public
key leaves the row listed with unknown metadata. The rule is: fail on things
that make an item unusable, degrade on things that only make it less
informative.

Timestamps are strict in one direction on purpose. Upstream emits zoneless civil
datetimes; if one ever arrives with an offset, that is a schema change worth
hearing about, so it is a decode error rather than a silent fallback. There is a
fixture for exactly that case.

Fixtures live in `internal/passcli/testdata/` and are excluded from prettier, so
they stay byte-for-byte as captured. One fixture carries invented future fields
specifically to prove additive changes are survivable.

## Amended 2026-10-30

protui is now Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). serde ignores unknown fields unless
`deny_unknown_fields` is set, and it is deliberately not set, so the additive
case is handled as before; required fields are still validated after decoding.
Fixtures moved to `src/passcli/testdata/`.
