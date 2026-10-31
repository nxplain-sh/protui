# Derive key metadata from public keys

- **Status:** accepted
- **Date:** 2026-08-31

## Context

protui lists SSH keys with their algorithm and fingerprint, and shows the
comment in the detail pane. The natural assumption is that these are fields on
the item.

They are not. `SshKeyItem`
(`pass-domain/src/models/item/mod.rs:1529-1534`, upstream `51a4c9b1`) is, in
full:

```rust
pub struct SshKeyItem {
    pub private_key: String,
    pub public_key: String,
    pub sections: Vec<CustomSection>,
}
```

Three fields. There is no algorithm, no fingerprint, and no comment. The
`--comment` flag on `item create ssh-key generate` writes into the trailing
comment of the OpenSSH public key line itself and round-trips only there.

Worse for listing: `item list` returns an item _summary_ that carries no key
material at all — by design, since the summary must never contain secrets. So
the public key is not available from the call that produces the list.

## Decision

We will derive algorithm, fingerprint, comment and key size locally by parsing
the public key with `golang.org/x/crypto/ssh.ParseAuthorizedKey`, and fetch each
item's public key in a separate call after the list loads.

`internal/keys` owns the derivation. `internal/passcli` returns keys with the
metadata unset, and the UI fills it in as the fetches complete, so rows appear
immediately and gain their fingerprint a moment later.

Those fetches are drained through a bounded window (four at a time) rather than
issued all at once, because each is a process spawn.

A key whose public key is missing or unparseable keeps its row and shows
`unknown`. The parse error is returned but not escalated: one malformed key must
not break the listing.

## Consequences

Fingerprints are computed the same way `ssh-keygen -lf` computes them, so they
can be compared directly against what a user sees elsewhere. Tests pin our
output against real `ssh-keygen` values.

Listing costs one call for the vault plus one per key. This is the main
contributor to load time and the reason for the concurrency bound. It is a
direct consequence of
[read-public-keys-with-a-field-scoped-view](0004-read-public-keys-with-a-field-scoped-view.md):
the cheaper bulk call is the one that returns private keys.

The comment is only as durable as the public key line. Rewriting that line
through another client would lose it, and protui cannot detect that.

The UI must tolerate partially-populated rows, since metadata arrives after the
row does. The detail pane shows `loading…` rather than an empty field.

RSA is the only algorithm whose size varies in practice, so it is the only one
rendered with its bit count (`rsa4096`); `ed25519` is fixed at 256 bits and
showing it would be noise.

## Amended 2026-10-30

protui is now Rust ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). Public keys are parsed with the RustCrypto
[`ssh-key`](https://crates.io/crates/ssh-key) crate instead of
`golang.org/x/crypto/ssh`, and `src/keys` owns the derivation. The fingerprints
are pinned against `ssh-keygen -lf` by the same test vectors as before.
