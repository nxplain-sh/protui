# Sanitize text before drawing it

- **Status:** accepted
- **Date:** 2026-08-31

## Context

protui draws item titles, vault names and key comments. All three are
user-authored, and Proton Pass supports sharing both items and vaults — so any
of them can have been written by somebody other than the person running protui.
That makes them untrusted input.

Drawn unfiltered into a terminal, such a string is not data. It is instructions:

- `ESC [ 2 J` clears the screen and `ESC [ H` homes the cursor, so a title can
  erase and redraw the interface — enough to fake a confirmation prompt.
- `ESC ] 0 ; … BEL` rewrites the terminal window title.
- `ESC ] 5 2 ; c ; <base64> BEL` — OSC 52 — **writes the system clipboard**, and
  is honoured by xterm, kitty, iTerm2, WezTerm and foot.
- Bidirectional overrides (U+202A–U+202E, U+2066–U+2069) reorder rendered text
  so a name displays as something other than what it is: the Trojan Source
  class of attack.

The OSC 52 case is the sharpest, because protui's headline action is putting a
public key on the clipboard. A hostile title could silently replace what the
user believes they just copied, and the UI would look entirely normal.

This was verified rather than assumed. Before the fix, a title containing those
sequences reached `View()` output intact.

The obvious place to fix it — at render time, on the way to the terminal — does
not work. By then lipgloss has wrapped the text in its own SGR escapes for
colour, so stripping escapes from rendered output would remove protui's styling
along with the attack. Sanitising has to happen on raw values, before styling.

## Decision

Text from outside protui is sanitised where it enters, in `internal/passcli` and
in `keys.Describe`, and nowhere else.

`keys.Sanitize` removes complete terminal control sequences (CSI, OSC, and the
two-character escapes), all control runes, explicit bidirectional overrides, and
the Unicode line and paragraph separators. Tabs and newlines become spaces
rather than vanishing, since these are single-line display fields.

It is applied to: vault names and item titles as they are decoded; the public
key returned by `item view --field public_key`; the comment `keys.Describe`
extracts from a key blob; the agent daemon's parsed status; and the `pass-cli`
stderr shown in errors, which upstream sometimes echoes item titles into.

## Consequences

Nothing downstream has to remember. The domain values held by `internal/keys`
are safe to render by construction, which is the same shape as the package's
other guarantee — that no private key material can reach it.

Sanitising at the boundary rather than at render also keeps comparisons honest.
The permanent-delete prompt asks the user to retype the item's title; because
the stored title is the sanitised one, what they are asked to match is what they
can actually see.

Agent status is classified on the raw value and displayed sanitised, so a
control character in upstream's output cannot change `running` into something
unrecognised.

The trade is that a title containing legitimate control characters is displayed
without them. No real SSH key title does, and the alternative is executing them.

Real right-to-left text still renders correctly: only the _explicit_ override
and isolate characters are dropped, and Arabic and Hebrew rely on implicit bidi,
which is untouched.

Because the guarantee lives at the parse boundary, a future code path that
builds a `keys.Key` from somewhere else would bypass it. The tests are written
against the parsers for that reason — they assert the boundary holds, rather
than asserting a property of an already-constructed value that nothing enforces.

`gosec` now runs in the lint step and `govulncheck` in CI, so the class of
problem this record describes is not the only one being watched for.

## Amended 2026-10-30

protui is now Rust on ratatui ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). ratatui keeps styling separate
from text rather than embedding SGR escapes in strings, so the render-time
objection above no longer holds as stated. The decision stands anyway, for the
other reasons given: sanitising at the boundary means nothing downstream has to
remember, and the title the delete prompt asks for is the one on screen. The
module is `src/keys/sanitize.rs`, a hand-written scanner equivalent to the
earlier regular expression, and `cargo-deny` replaces `gosec` and `govulncheck`
in CI.
