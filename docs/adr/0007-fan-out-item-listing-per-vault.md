# Fan out item listing per vault

- **Status:** accepted
- **Date:** 2026-08-31

## Context

protui presents one list of every SSH key the user has, across all vaults. The
vault is a column, not a navigation level.

`pass-cli item list` cannot do that. `ListItemsQuery::new`
(`pass-cli/src/commands/item/list.rs`, upstream `51a4c9b1`) requires exactly one
of `--share-id` or `--vault-name`, and errors without one:

```console
$ pass-cli item list --output json
Error: Please provide either --share-id, --vault-name, or set a default vault
with 'pass-cli settings set default-vault'
$ echo $?
1
```

So a unified list means N+1 calls: one `vault list`, then one `item list` per
vault. With N independent network-backed calls, some will fail while others
succeed — a vault the session cannot reach, a permission change, a timeout.

The tempting shape is to gather all of them and fail the load if any fails. That
makes one unreachable vault present as "you have no SSH keys", which is both
wrong and alarming.

## Consequences of getting this wrong are asymmetric

Showing a partial list with a visible warning is recoverable: the user sees most
of their keys and knows something is missing. Showing an empty list is not: it
looks like data loss.

## Decision

We will fetch the vault list once, then issue one `item list --share-id <id>
--filter-type ssh-key` per vault, and report each vault's result independently.

A vault that fails produces a status message naming that vault. Keys from
vaults that succeeded are listed regardless. There is no all-or-nothing barrier.

The `--filter-type ssh-key` flag is passed as a bandwidth optimisation, not as a
guarantee: upstream applies it client-side after fetching, so the item type is
re-checked when decoding.

## Consequences

Startup latency is bounded by the slowest vault rather than their sum, since the
calls are issued concurrently as Bubble Tea commands.

Partial failure is a first-class state. The model tracks how many vaults are
still outstanding rather than a single loading boolean, and a failure decrements
that counter like a success does — otherwise the UI would report "loading"
forever after any error.

Vault names are only available from the `vault list` call, so they are carried
into each key rather than read from the item output, which does not include
them.

`share_id` is the handle every item subcommand accepts. `vault_id` is returned
alongside it but is accepted nowhere in the item surface; it is carried for
display and never passed.

There is a test covering the case that matters: one vault succeeding and one
failing must leave the successful vault's keys listed and name the failing vault
in the status line.

## Amended 2026-10-30

protui is now Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). The per-vault calls are issued concurrently as
tokio tasks rather than Bubble Tea commands, and each result carries the load it
belongs to, so a refresh while a fan-out is in flight cannot merge two loads
into one list.
