# Restyle the TUI as neobrutalist

- **Status:** accepted
- **Date:** 2026-10-31

## Context

The first look was deliberately quiet. Its reasons were recorded in code
comments rather than a record, and are restated here because this decision
reverses each of them:

- **No filled banner.** "A coloured block across the top of a terminal reads
  as chrome, and the list is what deserves the attention." The header was a
  plain wordmark over a faint rule.
- **Faint rules and borders**, so structure receded behind content.
- **Rounded dialog corners**, "which read as softer than square ones and suit
  a prompt"; only the permanent delete was square, heavy and red.
- **One accent against a neutral ramp:** Proton's violet, with every colour
  tuned separately for light and dark terminals.
- **A blank screen behind dialogs.**

In use, the quiet look had costs. The algorithm was a word in a dimmed
summary line, so telling an old `rsa2048` key from an `ed25519` one meant
reading every row. The selected row was a one-column bar in the gutter, easy
to lose on a long list. Trash and permanent delete looked alike apart from
their frames. And protui was about to get a README banner and a website with
no visual identity in common with the TUI.

`docs/design/protui-design.md` specifies a replacement: heavy ink borders,
hard two-column shadows, a filled yellow header, a filled selected row, a
colour chip per algorithm, a filled title band per dialog, hazard stripes on
the permanent delete, and a dark footer key bar. It draws only what a terminal
can draw — truecolor, bold, box-drawing and block elements — and the same
palette is used by the banner and the site.

The alternatives considered were to keep the quiet look and only add algorithm
chips, which fixes scanning but not selection, danger or identity; and to
adopt the new look partially, keeping faint borders. The second was rejected
because the heavy borders and hard shadows are what make the fills read as
deliberate rather than as stray colour.

## Decision

We will draw every screen to the design spec. The spec is the reference for
layout and colour, and wins over its own reference grids where they disagree.

- **Two themes, one set of fills.** Ink, muted, faint, shadow, code, dots,
  danger and bar tokens differ between light and dark terminals — on dark,
  borders and shadows flip to cream. The six fills are the same in both, and
  every fill carries `#111111` text.
- **Red is for the permanent delete alone**: its dialog, its `D` keycap, and
  nothing else. Errors elsewhere use an orange chip and ink text.
- **The terminal's own background is never painted.** The spec's ground
  colours are used only as the text of inverted chips.
- **The theme is `PROTUI_THEME=light|dark` if set**, otherwise the terminal's
  answer to a background query, otherwise dark. Without truecolor the nearest
  xterm-256 colours are sent; under `NO_COLOR`, fills become reverse video.
- **Drawing goes through `src/ui/draw.rs`**: one helper each for the shadow,
  chip, button, outline, backdrop and hazard stripes, all clipped at the
  screen edge.

Behaviour does not change: key bindings, the 92-column cut-off for the detail
pane, scrolling, and every security property stay as they were.

## Consequences

- The list scans by colour, the selection is unmistakable, and the permanent
  delete looks like nothing else on screen.
- protui shares one look with its banner and its site.
- The header and footer take more rows: 13 rows of chrome around the list
  instead of 6. On a 24-row terminal three keys fit on screen rather than
  six, and the new-key form switches to one-row fields below about 32 rows.
- The look leans on colour more than before. Monochrome terminals get reverse
  video for fills, which keeps the structure but loses the distinctions
  between fills — every chip reads the same.
- More code draws straight into the buffer instead of through ratatui
  widgets, so it is checked by snapshot tests of every screen, in both themes,
  at 120×38 and 80×24, and by tests that red appears only on the delete and
  that every fill carries ink text.
- Sanitising is unchanged and still happens where text enters protui. The
  per-character test `TextInput` applies is now the same one `sanitize` uses,
  so typed text cannot carry a bidirectional override into the filter label.
- The header's agent chip reuses the status read at startup and when the
  agent panel opens, so it can be stale if the agent is started or stopped
  outside protui. Fetching it on every reload would add a `pass-cli` call for
  a nicety.

Revisit if the extra chrome proves too costly on small terminals, or if users
report the fills as hard to read on a particular terminal theme.
