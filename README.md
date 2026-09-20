# protui

A terminal UI for managing your Proton Pass SSH keys.

> **protui is not affiliated with, endorsed by, or supported by Proton AG.**
> It is an unofficial front-end that drives the official `pass-cli` binary.
> Proton Pass is a trademark of Proton AG.

## Requires `pass-cli`

protui does not talk to Proton's servers. It shells out to the official
[Proton Pass CLI](https://protonpass.github.io/pass-cli/) for everything, so
that binary must be installed **and authenticated** before protui will start.

```sh
# macOS
brew install protonpass/tap/pass-cli

# then, once
pass-cli login
```

If either step is missing, protui exits immediately with a message telling you
which one — it will not open a UI you cannot use.

Verify with:

```sh
pass-cli info
```

## Install

```sh
go install github.com/nxplain-sh/protui/cmd/protui@latest
```

Prebuilt binaries for macOS and Linux are attached to each
[release](https://github.com/nxplain-sh/protui/releases).

Or from a clone:

```sh
make build   # → bin/protui
make install # → $GOBIN/protui
```

Requires Go 1.27 or newer. macOS and Linux; Windows is not supported.

## Use

```sh
protui
```

Navigation is vim's, with vim's semantics and vim's modifiers:

| Key               | Action                                               |
| ----------------- | ---------------------------------------------------- |
| `j` `k`           | move down / up                                       |
| `gg` `G`          | jump to top / bottom                                 |
| `Ctrl-f` `Ctrl-b` | page down / up                                       |
| `Ctrl-d` `Ctrl-u` | half page down / up                                  |
| `/`               | fuzzy filter by title, vault, comment or fingerprint |

Actions follow lazygit and k9s — one mnemonic letter each:

| Key       | Action                                                   |
| --------- | -------------------------------------------------------- |
| `c` / `y` | copy the selected public key to the clipboard            |
| `n`       | generate a new key                                       |
| `d`       | move to trash — recoverable with `pass-cli item untrash` |
| `D`       | delete permanently — no undo, requires typing the title  |
| `a`       | SSH agent daemon: status, start, stop                    |
| `r`       | reload from `pass-cli`                                   |
| `?`       | expand the help bar                                      |
| `q`       | quit                                                     |

There is no `dd` or `yy`: vim's doubling exists to disambiguate an operator
from a motion, and a list has one object under the cursor and no motions. The
reasoning is in
[the keymap decision record](docs/adr/0010-navigate-like-vim-act-with-mnemonics.md).

Keys from every vault are listed together, with the vault shown per row.

`Ctrl-e` and `Ctrl-y` scroll the detail pane when a key is too long to fit,
without moving the cursor — as in vim.

### Copying needs a clipboard

On macOS this works out of the box. On Linux, copying shells out to `xclip`,
`xsel` or `wl-copy`, so one of them has to be installed:

```sh
sudo apt install xclip      # X11
sudo apt install wl-clipboard  # Wayland
```

Copying will not work over a plain SSH session with no display, since there is
no clipboard to write to. protui reports the failure rather than pretending it
worked; the public key is on screen and can be selected with the mouse.

### Trashing and deleting

`d` moves a key to the trash. protui lists only active keys, so it disappears
from the list — restore it from the CLI:

```sh
pass-cli item list --vault-name <vault> --filter-state trashed
pass-cli item untrash --share-id <id> --item-id <id>
```

`D` destroys the key outright. There is no undo and no trash copy, which is why
it asks you to type the title.

### Generating a key

`n` opens a form for the title, vault, algorithm (`ed25519` by default, or
`rsa2048` / `rsa4096`), an optional comment, and an optional passphrase.

The comment is stored **inside the public key line itself** rather than as a
separate field, because that is the only place upstream keeps it.

## Security

- **No private key ever reaches protui.** The one upstream call that returns
  private key material — `pass-cli item view --output json` — is never used.
  Public keys are read with `item view --field public_key`, which returns
  exactly one field. There is no code path in protui that names the private key.
- **Nothing secret is passed as a command-line argument.** Arguments are visible
  to any user on the machine via `ps`. A passphrase, when you set one, is
  handed to `pass-cli` through the `PROTON_PASS_SSH_KEY_PASSWORD` environment
  variable instead, and the buffer holding it is wiped once the child process
  has it. (The environment string itself is an immutable Go string and cannot be
  explicitly zeroed; it lives until garbage collected. It is never written to
  argv, disk, or logs.)
- **`--show-secrets` is never passed.** protui reads only the redacted item
  summary, which upstream guarantees carries no user secret material.
- **The UI displays public keys and fingerprints only.**
- Fingerprints and algorithms are computed locally from the public key, so
  deriving them requires no additional secret access.
- **Text from Proton Pass is sanitised before it is drawn.** Item titles, vault
  names and key comments are user-authored, and Proton Pass supports sharing
  items and vaults, so they can have been written by somebody else. Drawn
  unfiltered into a terminal such a string is not data but instructions: it can
  clear the screen, rewrite the window title, or — via OSC 52, which xterm,
  kitty, iTerm2, WezTerm and foot all honour — **write to your clipboard**,
  which matters here because putting a key on the clipboard is what protui is
  for. Terminal escape sequences and bidirectional overrides are stripped at
  the point the text enters, in `internal/passcli`.
- **Dependencies are scanned in CI** with
  [govulncheck](https://go.dev/blog/govulncheck), and `gosec` runs as part of
  the lint step.

## How it fits together

```
cmd/protui        entry point; preflight checks, then hands off to the UI
internal/passcli  the only package that executes pass-cli
internal/keys     domain types, independent of the upstream JSON shape
internal/ui       Bubble Tea models; the update loop does no I/O
```

`internal/passcli` is the single seam against upstream. Everything else depends
on it rather than on `os/exec`, so an upstream change is contained to one
package.

The reasoning behind these choices — and the ones that are not visible in the
code, like why `pass-cli item view --output json` is never called — is recorded
in [`docs/adr/`](docs/adr/README.md).

## The upstream schema

`pass-cli` serialises its internal Rust structs directly, and its JSON output is
**not a documented stable contract**. [`docs/schema.md`](docs/schema.md) records
the exact shape protui parses — verified against both the upstream source and a
live binary, with the version and commit it was captured from — so it can be
diffed when upstream changes.

Some things that document will save you from guessing:

- SSH key items store only `private_key`, `public_key` and `sections`. There is
  no algorithm, fingerprint, or comment field.
- `item list` cannot span vaults; protui fans out per vault.
- Timestamps have no timezone, so `time.RFC3339` will not parse them.
- Casing is inconsistent within a single object: `item_type` is snake_case,
  `state` and `flags` are PascalCase, and the CLI flag is kebab-case.

Unknown JSON fields are ignored rather than fatal, so an upstream addition will
not break the list. A removed or renamed field is caught by validating that
required fields are present, and reported as an error naming the command that
produced it.

## Development

```sh
make check   # tidy, lint, test — mirrors CI
make test    # tests only
make fmt     # gofmt the Go sources
make format  # prettier the Markdown, JSON and YAML
make run     # build and launch
```

`make help` lists every target.

CI runs the same checks on push and pull request, across Linux and macOS, with
`-race` — see [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

Go code is formatted by `gofmt`; Markdown, JSON and YAML by
[prettier](https://prettier.io). `golangci-lint` is used when installed and
skipped with a note when not. The code follows
[Effective Go](https://go.dev/doc/effective_go) and the
[Google Go Style Guide](https://google.github.io/styleguide/go/); `make lint`
covers `go vet` and, where available, `staticcheck` via golangci-lint.

Dependency updates come from [Renovate](https://docs.renovatebot.com),
configured in [`renovate.json`](renovate.json). The charmbracelet packages
update together in one pull request, a bump to the minimum Go version waits for
approval on the dependency dashboard, and nothing merges itself — every update
is reviewed.

Architectural changes should come with a record in
[`docs/adr/`](docs/adr/README.md) — see its README for what earns one.

Tests for the parsing layer are table-driven over fixture JSON in
`internal/passcli/testdata/`. Those fixtures are recorded `pass-cli` output and
are excluded from prettier so they stay byte-for-byte as captured.

To eyeball the UI without a Proton session, render every screen with colour
forced on:

```sh
PROTUI_PREVIEW=1 go test ./internal/ui -run TestPreview -v
PROTUI_PREVIEW=1 PROTUI_PREVIEW_LIGHT=1 go test ./internal/ui -run TestPreview -v
```

It asserts nothing and is skipped by default — it exists so layout changes can
be checked against a light and a dark terminal before they ship.

## Scope

v1 handles SSH keys only: listing, generating, viewing, copying, trashing,
deleting, and the SSH agent daemon. Logins, notes, TOTP, aliases, secret
injection and vault management are out of scope — use `pass-cli` directly.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). It covers the setup, the checks, and the
handful of security properties a change must not break.

Vulnerabilities go through [SECURITY.md](SECURITY.md), not the issue tracker.

Changes are recorded in [CHANGELOG.md](CHANGELOG.md).

## Licence

MIT. See [LICENSE](LICENSE).
