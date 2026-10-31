# Architecture decision records

This is protui's architecture decision log: the "why" behind choices that are
not recoverable from reading the code.

An [architecture decision record](https://github.com/architecture-decision-record/architecture-decision-record)
captures one decision together with its context and its consequences. The code
shows what protui does; these records show what else was on the table and what
the choice cost.

## The log

Numbered roughly from the foundational to the specific rather than
chronologically — 0001 through 0013 were all decided on the same day, during
initial design. Records added later take the next free number.

| Decision                                                                                                 | In short                                                                    |
| -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| [Wrap pass-cli instead of the Proton API](0001-wrap-pass-cli-instead-of-the-proton-api.md)               | Drive the official binary; implement no crypto of our own.                  |
| [Record the upstream schema before implementing](0002-record-the-upstream-schema-before-implementing.md) | `pass-cli` JSON is not a contract, so write down what we verified.          |
| [Isolate pass-cli behind one package](0003-isolate-pass-cli-behind-one-package.md)                       | One package execs; the security rules live where they can be tested.        |
| [Read public keys with a field-scoped view](0004-read-public-keys-with-a-field-scoped-view.md)           | The obvious call returns private keys, so never make it.                    |
| [Pass passphrases through the environment](0005-pass-passphrases-through-the-environment.md)             | argv is world-readable, and `--password` would prompt on the TTY.           |
| [Derive key metadata from public keys](0006-derive-key-metadata-from-public-keys.md)                     | Upstream stores no algorithm, fingerprint, or comment field.                |
| [Fan out item listing per vault](0007-fan-out-item-listing-per-vault.md)                                 | `item list` cannot span vaults; one failure must not blank the list.        |
| [Separate trashing from permanent deletion](0008-separate-trashing-from-permanent-deletion.md)           | `d` is recoverable, `D` is not, and the prompts differ accordingly.         |
| [Keep I/O out of the update loop](0009-keep-io-out-of-the-update-loop.md)                                | Every `pass-cli` call runs off the loop, so the UI never freezes.           |
| [Navigate like vim, act with mnemonics](0010-navigate-like-vim-act-with-mnemonics.md)                    | Real `gg` and `Ctrl-f/b/d/u`; actions stay lazygit-style single letters.    |
| [Sanitize text before drawing it](0011-sanitize-text-before-drawing-it.md)                               | Shared vault titles are untrusted; escapes reach the terminal otherwise.    |
| [Tolerate unknown JSON fields but validate required ones](0012-tolerate-unknown-json-fields.md)          | Additive upstream changes are harmless; subtractive ones must be loud.      |
| [Fail before taking over the terminal](0013-fail-before-taking-over-the-terminal.md)                     | A missing session is reported in a terminal that still works.               |
| [Rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)                                     | Ownership scopes the passphrase; types keep the delete on the right item.   |
| [Restyle the TUI as neobrutalist](0015-restyle-the-tui-as-neobrutalist.md)                               | Algorithm chips, a filled selection, and red kept for the permanent delete. |

## Writing one

Copy [`0000-template.md`](0000-template.md). It follows
[Michael Nygard's format](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions):
context, decision, consequences.

File names are a four-digit number, then a **present-tense imperative verb
phrase**, lowercase with dashes, matching how commit messages read:
`0001-wrap-pass-cli-instead-of-the-proton-api.md`, not `0001-passcli.md`. Take
the next unused number, and never reuse one — a superseded record keeps its
number and stays in place.

## What earns a record

Write one when a future reader would otherwise reasonably ask "why on earth is
it done this way", and the answer is not in the code.

In practice that has meant: a constraint discovered in upstream source that
contradicts the obvious approach; a security property that must hold across many
call sites; a decision where the safe option is not the convenient one.

Skip it when a decision is local, cheap to reverse, or already documented
elsewhere. Package-level choices belong in a doc comment. The shape of the
upstream JSON belongs in [`../schema.md`](../schema.md).

## Keeping them honest

Each record is dated, because a good part of its content is facts about
`pass-cli` that were true when it was written. Where a record cites upstream
behaviour it names the file and the commit, so a reader can check whether it
still holds.

Prefer amending a record with a dated note over silently rewriting it — the
reasoning that led somewhere is the part worth keeping. When a decision is
genuinely replaced, write a new record and mark the old one superseded with a
link, rather than editing it into agreement with the present.
