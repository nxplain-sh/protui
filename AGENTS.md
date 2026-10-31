# AGENTS.md

protui is a Rust TUI for Proton Pass SSH keys. It drives the official
`pass-cli` binary and never talks to Proton directly.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before your first change. It holds the
security guarantees a change must keep, the style, the test conventions, the
scope and the commit format. This file adds only what an agent needs on top.

## The local `pass-cli` is the user's real account

An authenticated `pass-cli` on this machine is the user's own Proton Pass
vault. Run read-only commands only (`info`, `vault list`, `item list`,
`item view --field public_key`). Never run `item view --output json`: it prints
private keys into your context.

## Done means `cargo xtask check` passes

Run `cargo xtask fmt`, then `cargo xtask check`. That is the same set CI and
the pre-commit hook run. `fmt` also runs prettier over Markdown, JSON and YAML,
so it covers doc edits too.

## Seeing the UI

`cargo run` takes over the terminal and needs a live Proton session. To look at
a screen, run `cargo xtask preview` (add `PROTUI_PREVIEW_LIGHT=1` for the light
theme). It prints every screen with no session.

## UI snapshots

`screens_match_their_snapshots` in `src/ui/tests.rs` pins every screen, in both
themes, at 120x38 and 80x24. An intended layout change fails it. Accept with
`INSTA_UPDATE=always cargo test`, then read the `.snap` diff in
`src/ui/snapshots/` before committing. A plain failing run leaves `.snap.new`
files there; `.gitignore` does not cover them, so delete any left over.

## Where the why lives

- `docs/adr/`: read the matching record before reversing a design choice.
- `docs/schema.md`: the upstream JSON shape protui parses.
- `docs/design/protui-design.md`: the visual spec; read it before changing
  layout, colour or glyphs.
