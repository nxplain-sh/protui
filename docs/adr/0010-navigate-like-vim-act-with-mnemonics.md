# Navigate like vim, act with mnemonics

- **Status:** accepted
- **Date:** 2026-08-31

## Context

The brief asked for "keybindings in vim style with a help bar". The first
implementation took that loosely, and an audit found two bindings that were
wrong on any reading, plus a question about how far the vim analogy should go.

**`g` acted alone.** Top-of-list was bound to a bare `g`, inherited from
bubbles' default keymap. In vim `g` is a prefix — `gg`, `gU`, `gv`, `gi`, `g;`
— and never does anything by itself. Binding it to an action fires on the first
keystroke of what a vim user intends as a two-key sequence.

**Undocumented paging.** bubbles binds paging to bare `f`, `b`, `u`, `h` and
`l`. protui never rebound or documented them, so they were live and
undiscoverable. They are also not vim, which pages with the modifier held:
`Ctrl-f`, `Ctrl-b`, `Ctrl-d`, `Ctrl-u`.

Those two are defects. The open question was the action keys, where there are
three positions:

1. **Full vim fidelity** — `dd` to trash, `yy` to copy. This misreads why those
   sequences exist. `d` and `y` are _operators_ that take a motion; the doubled
   form is the special case meaning "apply to this line". Doubling is
   disambiguation. A list has one object under the cursor and no motion
   namespace, so `dd` would be `d` plus a keystroke that resolves nothing.
2. **Vim-first collision avoidance** — move "new" off `n`, on the grounds that
   a vim user pressing `n` after `/` expects the next match and would instead
   get a modal.
3. **Mnemonic actions** — keep `n` for new, as lazygit does.

Position 2 was tried and reverted. The flaw is that **`/` here filters rather
than searches**: non-matching rows are removed from the list, so there is no
hidden next match to jump to. Binding `n`/`N` to movement would have duplicated
`j`/`k` exactly — muscle-memory placation buying no capability — while giving
up the single most established convention in the closest comparable tool.

## Decision

Navigation is vim's, with vim's semantics and vim's modifiers:

- `j` / `k` to move
- `gg` / `G` for top and bottom, with `gg` a real two-key sequence
- `Ctrl-f` / `Ctrl-b` to page, `Ctrl-d` / `Ctrl-u` to half-page
- `/` to filter

Actions follow the lazygit and k9s idiom — one mnemonic letter each, and no
operators: `c` (or `y`) copy, `n` new, `d` trash, `D` delete permanently, `a`
agent, `r` refresh, `?` help, `q` quit.

bubbles' bare-letter paging bindings are unbound rather than left live.

## Consequences

Movement behaves the way a vim user's fingers expect, and actions behave the
way a lazygit or k9s user's do. Those two populations overlap heavily, which is
what makes the split viable.

`gg` needs a prefix state in the model, since Bubble Tea delivers one keypress
per message and `key.WithKeys("gg")` would never match. A bare `g` arms the
prefix; the second `g` completes it; any other key cancels it and is handled
normally. The prefix is also cleared when entering the filter, so an abandoned
`g` cannot swallow the first character typed there.

Half-page movement is implemented on top of the list's cursor rather than its
pager, because bubbles exposes `CursorUp`/`CursorDown` and whole pages but no
half-page jump. The step is derived from the delegate's row height, so it stays
correct if the row layout changes.

A vim user who presses `n` after filtering gets the create form rather than
nothing. This is the accepted cost of keeping `n` for new. It is a mode change
rather than a destructive act, `esc` backs out of it, and the alternative
binding would have been a synonym for `j`.

`D` means "the same as `d`, but irreversible", which is a TUI convention rather
than a vim one — in vim `D` is `d$`. Kept because it reads correctly here and
nothing competes for it; see
[separate-trashing-from-permanent-deletion](0008-separate-trashing-from-permanent-deletion.md).

`a`, `r`, `q` and `?` also mean other things in vim (append, replace character,
record macro, search backward). They are kept because a read-only list offers
no insert mode, no character to replace and no macro recording, so there is no
expectation to violate.

If `/` ever becomes a true search that leaves non-matching rows visible, this
decision should be revisited: at that point `n`/`N` would have real work to do
and the argument above no longer holds.

The bindings are covered by tests rather than left to review, including the
cases that are easy to regress: that a lone `g` does nothing, that an unrelated
key cancels the prefix, and that the bare-letter paging keys stay unbound.

## Amended 2026-10-30

protui is now Rust on ratatui ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). ratatui has no list widget with
default key bindings, so there are no bare-letter paging bindings to unbind:
the list moves only on the keys in `src/ui/keymap.rs`. The `gg` prefix state
and the half-page arithmetic carry over unchanged, and `Ctrl-c` now quits from
every screen, not only the list.
