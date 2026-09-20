# Security policy

protui is an unofficial front-end for the official Proton Pass CLI. It handles
SSH keys, so the security properties below are the point of the project rather
than a footnote.

## Reporting a vulnerability

Please report privately, not in a public issue.

Use GitHub's **[private vulnerability reporting](https://github.com/nxplain-sh/protui/security/advisories/new)**
— the _Report a vulnerability_ button under the repository's Security tab. It
opens a channel visible only to the maintainers.

Useful things to include: what you did, what happened, what you expected, the
`protui` and `pass-cli` versions (`protui --version`, `pass-cli --version`),
your OS and terminal emulator. A failing test or a reproduction against a
throwaway vault is ideal, but a clear description is enough.

This is a personal project maintained in spare time, so there is no guaranteed
response window. Reports are taken seriously and acknowledged as soon as
practical.

## What is in scope

Anything in this repository:

- Private key material reaching protui, being displayed, logged or written to
  disk.
- Secrets appearing in process arguments, which any local user can read.
- Untrusted text from Proton Pass reaching the terminal as instructions rather
  than as data.
- Argument construction that lets a value be interpreted as a flag.
- Anything that causes protui to act on the wrong item, particularly the
  permanent-delete path.

## What is not in scope

protui does not talk to Proton's servers, implement any cryptography, or hold
any credential. It shells out to `pass-cli` for everything.

- **Vulnerabilities in `pass-cli` itself** belong to
  [protonpass/pass-cli](https://github.com/protonpass/pass-cli).
- **Vulnerabilities in Proton Pass, the service or its clients**, belong to
  [Proton's bug bounty programme](https://proton.me/security/bug-bounty).

If a `pass-cli` weakness is only reachable _because of how protui calls it_,
that is in scope here — please report it.

## Security model

protui's guarantees, and where they are enforced:

| Guarantee                                         | Enforced in                 |
| ------------------------------------------------- | --------------------------- |
| No private key material ever enters the process   | `internal/passcli/item.go`  |
| No secret is ever a command-line argument         | `internal/passcli/item.go`  |
| Only one package may execute a subprocess         | `internal/passcli`          |
| Text from Proton Pass is sanitised before display | `internal/keys/sanitize.go` |

The reasoning behind each, including what was rejected, is in
[`docs/adr/`](docs/adr/README.md) — particularly
[read public keys with a field-scoped view](docs/adr/0004-read-public-keys-with-a-field-scoped-view.md),
[pass passphrases through the environment](docs/adr/0005-pass-passphrases-through-the-environment.md)
and
[sanitize text before drawing it](docs/adr/0011-sanitize-text-before-drawing-it.md).

Every guarantee has tests asserting it, including that no constructed argument
list contains the word `private`, and that hostile item titles cannot emit
terminal escape sequences.

`gosec` runs in the lint step and `govulncheck` runs in CI on every push and
pull request.

## Known limitations

These are accepted trade-offs rather than oversights, and are documented so you
can judge them yourself:

- **A passphrase cannot be fully wiped from memory.** It is passed to `pass-cli`
  through the child environment, never argv. The `[]byte` holding it is zeroed
  once the environment entry is built, but that entry is a Go string, which is
  immutable and cannot be explicitly cleared; it lives until garbage collected.
  It is never written to argv, disk, or logs.
- **A child process environment is not private from root or from the same
  user.** On Linux it is readable at `/proc/<pid>/environ`. The environment is
  the strongest mechanism `pass-cli` offers for this, and it is the one upstream
  documents.
- **protui trusts `pass-cli` and the session it holds.** It does not verify the
  binary beyond resolving it on `PATH`, so a `PATH` an attacker controls is a
  `pass-cli` an attacker controls.
- **The clipboard is not protui's to protect.** A copied public key is not
  secret, but it stays on the clipboard until something replaces it.
- **Supported versions:** protui is pre-release and unversioned. Fixes land on
  `main`; there are no backports.
