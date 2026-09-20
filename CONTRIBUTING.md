# Contributing

Thanks for taking a look. protui is small and deliberately narrow, so the most
useful thing you can do before writing code is check that a change fits the
scope below.

## Getting set up

You need:

- **Go 1.27 or newer** — the version is read from `go.mod`, so bump it there.
- **`pass-cli`**, installed and authenticated. protui refuses to start without
  it. See the [README](README.md#requires-pass-cli).
- **Node** — only for `prettier`, fetched on demand via `npx`.
- **`golangci-lint`** — optional. The lint target skips it with a note when it
  is absent, but CI runs it, so installing it saves a round trip.

```sh
git clone https://github.com/nxplain-sh/protui
cd protui
make check   # tidy, lint, test — the same checks CI runs
make hooks   # optional: run those checks on every commit
```

`make help` lists every target.

### The pre-commit hook

`make hooks` points git's `core.hooksPath` at
[`.githooks/pre-commit`](.githooks/pre-commit), which runs `gofmt`, `prettier`,
`go vet`, `golangci-lint` and `go test` before each commit. The hook shells out
to Makefile targets, so the hook and `make check` cannot drift apart. Git reads
that setting from repository config, so it is one command per clone:

```sh
git config core.hooksPath .githooks
```

The hook is scoped by what the commit touches — a Markdown-only commit does not
run the Go checks — and `git commit --no-verify` skips it when you need it.

## Before you open a pull request

```sh
make fmt     # gofmt the Go sources
make format  # prettier the Markdown, JSON and YAML
make check   # tidy, lint, test
```

CI runs the equivalent on Linux and macOS, with `-race`, plus `gosec` and
`govulncheck`.

## The parts that are not negotiable

protui manages SSH keys, so a few properties are load-bearing. A change that
breaks one of these will not be merged even if it is otherwise good, and each
has tests that will fail if you do.

- **No private key material enters the process.** The upstream call
  `item view --output json` returns the private key unredacted and must never be
  made. Public keys are read with `item view --field public_key`. No code path
  names the private key; adding one is a design change, not a patch.
- **No secret is ever a command-line argument.** Arguments are readable by any
  local user via `ps`. The only secret protui handles — a passphrase on create —
  goes through the child environment.
- **`--show-secrets` is never passed.**
- **Only `internal/passcli` may execute a subprocess.** Nothing else imports
  `os/exec`. If you need something new from `pass-cli`, add a typed wrapper
  there rather than an exec call at the call site.
- **Text from Proton Pass is sanitised before it is displayed.** Titles, vault
  names and comments can come from a shared vault, so they are untrusted. This
  happens at the parse boundary and cannot be moved to render time — see
  [the decision record](docs/adr/0011-sanitize-text-before-drawing-it.md) for why.

## If you touch the parsing layer

`pass-cli` serialises its internal Rust structs directly, and its JSON output is
not a documented contract. [`docs/schema.md`](docs/schema.md) records what
protui parses, verified against both the upstream source and a live binary.

If you change how output is parsed, or bump the supported `pass-cli` version,
re-verify and update that document. It has a section of commands to re-run and
names the fields most likely to drift. Please do not paste real vault data into
it — redact as the existing examples do.

Parser tests are table-driven over fixtures in `internal/passcli/testdata/`.
Those fixtures are recorded output and are excluded from prettier so they stay
byte-for-byte as captured. Where a fixture needs control characters, write them
as JSON `\u001b`-style escapes rather than as raw bytes — which
is what upstream emits anyway, and keeps invisible characters out of the
repository.

## If you touch the UI

The Bubble Tea update loop does no I/O — every `pass-cli` call is dispatched as
a `tea.Cmd` and comes back as a message. Keep it that way; a blocking call
freezes the interface, including the ability to quit.

Model methods take value receivers and return the updated model, matching how
Bubble Tea threads state.

To see your change without a Proton session:

```sh
PROTUI_PREVIEW=1 go test ./internal/ui -run TestPreview -v
PROTUI_PREVIEW_LIGHT=1 PROTUI_PREVIEW=1 go test ./internal/ui -run TestPreview -v
```

It renders every screen with colour forced on. Check both themes — the palette
is adaptive, and light terminals are easy to break without noticing.

## Architecture decisions

Choices that a future reader would reasonably question, and that the code does
not explain on its own, get a record in [`docs/adr/`](docs/adr/README.md). Its
README covers what earns one and the file naming convention.

You do not need an ADR for a bug fix or a local change. You probably do for a
new dependency, a new `pass-cli` command, a change to one of the guarantees
above, or a reversal of an existing record.

## Style

Go code follows [Effective Go](https://go.dev/doc/effective_go) and the
[Google Go Style Guide](https://google.github.io/styleguide/go/). A few things
that come up in review:

- Receivers are consistent per type — all value or all pointer, never mixed.
- Comments explain _why_, not _what_. The code already says what.
- Prefer the standard library and builtins over hand-rolled helpers.
- Errors are lowercase, wrapped with `%w`, and name the command that failed.

## Tests

New behaviour needs a test. The existing suite is a reasonable guide:

- Parsing is table-driven over fixture JSON.
- The UI is driven with synthetic messages and asserted on rendered output — no
  terminal and no `pass-cli` binary required.
- Security properties are asserted directly, not left to review: that argument
  lists never contain a secret, that hostile titles cannot emit escapes.

Tests must not require a Proton account or write to one.

## Scope

protui v1 does SSH keys, and only SSH keys: listing, generating, viewing,
copying, trashing, deleting, and the agent daemon.

Out of scope: logins, notes, TOTP, aliases, secret injection, vault management
and Windows support. Those are all things `pass-cli` already does well, and
adding them would make protui a worse tool for the one job it has.

If you are unsure whether something fits, open an issue before building it.

## Commits and pull requests

Commit subjects are a present-tense imperative phrase — "add the import
picker", not "added" or "adds" — matching the ADR file naming.

Keep a pull request to one concern. If you find an unrelated problem along the
way, that is a second pull request, or an issue if you would rather not fix it.

## Reporting a vulnerability

Not through an issue or a pull request. See [SECURITY.md](SECURITY.md).
