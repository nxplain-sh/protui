# protui — neobrutalist TUI design spec

Source of truth for the protui visual redesign. The design canvas has the
rendered artboards; this file is the implementation reference. Where the two
disagree, this file wins.

Every screen is designed on a **120×38 character grid** using only things a
terminal can draw: truecolor foreground/background, bold, box-drawing and
block-element glyphs. No emoji, no Nerd Font glyphs.

## The seven rules

1. Borders are ink and heavy (`┏━┓┃┗┛┣┫`). Never faint grey.
2. Shadows are solid: 2 columns right, half a row down (built from `▄` `▀` and filled cells).
3. Coloured fills always carry ink (`#111111`) text. Never white on colour.
4. One filled band per surface: the header, the selected row, a dialog's title band.
5. The key algorithm is a colour chip, so the list scans without reading.
6. Red belongs to `D` (permanent delete) alone. Trash is orange, because it is recoverable.
7. On a dark terminal, borders and shadows flip to cream; fills stay the same.

## Tokens

Two themes. Do not paint the whole screen background: the "ground" is the
user's own terminal background. The ground values below are only the reference
the colours were tuned against.

| Token        | Light     | Dark      | Use                                              |
| ------------ | --------- | --------- | ------------------------------------------------ |
| `ink`        | `#111111` | `#F5F0E1` | body text, all borders                           |
| `muted`      | `#4F483B` | `#CFC8B6` | labels, secondary text                           |
| `faint`      | `#6B6352` | `#A39C8C` | list summaries, hints, placeholders              |
| `shadow`     | `#111111` | `#F5F0E1` | hard shadows                                     |
| `code`       | `#F1E6C8` | `#262626` | public-key block, command blocks, cancel buttons |
| `dots`       | `#E2D6B6` | `#2C2C2C` | dialog backdrop pattern (decorative)             |
| `danger`     | `#B3121F` | `#FF6B6B` | danger text on ground                            |
| `bar`        | `#111111` | `#F5F0E1` | footer key-bar background                        |
| `bar_text`   | `#FFF8E7` | `#111111` | footer key-bar labels                            |
| `disabled`   | `#E6D9B8` | `#E6D9B8` | disabled button fill (text `#6B6352`)            |
| ground (ref) | `#FFF8E7` | `#141414` | reference only — never painted                   |

Fills are the same in both themes and always carry `#111111` text:

| Fill     | Hex       | Use                                          |
| -------- | --------- | -------------------------------------------- |
| `yellow` | `#FFD93D` | header band, keycaps, focus, primary buttons |
| `violet` | `#8C6BFF` | selected row, detail/new-key title bands     |
| `mint`   | `#5BE3A6` | success, `ed25519`, agent running            |
| `pink`   | `#FF7AB8` | `rsa4096`, stop agent                        |
| `orange` | `#FFA24C` | `rsa2048`, trash, warnings                   |
| `red`    | `#FF5A5A` | permanent delete only                        |

Algorithm chips: `ed25519` → mint, `rsa4096` → pink, `rsa2048` → orange. Chip
text is the label uppercased with one space of padding each side, bold.

If the terminal does not advertise truecolor (`COLORTERM` is not `truecolor` or
`24bit`), degrade to the nearest 256-colour values rather than sending RGB.

## Glyphs

| Purpose                     | Glyphs                        |
| --------------------------- | ----------------------------- |
| Heavy box (default)         | `┏ ━ ┓ ┃ ┗ ┛ ┣ ┫`             |
| Light box (unfocused input) | `┌ ─ ┐ │ └ ┘`                 |
| Shadows / thick rules       | `▀ ▄` and filled cells        |
| Marks                       | `◆ ▶ ● ○ ◐ ✓ ✗ ‹ › • · ↑ ↓ ▸` |

## Components

### Shadow

For a rectangle at `(x, y)` of size `w × h`, with offset `dx` (2 for panes and
dialogs, 1 for buttons, inputs and status chips):

- Columns `x+w … x+w+dx-1`: row `y` gets `▄` in the shadow colour; rows `y+1 … y+h-1` are solid shadow.
- Row `y+h`: columns `x+dx … x+w+dx-1` get `▀` in the shadow colour.
- Clip to the screen. Draw after the backdrop and before anything that sits on top.

One helper, used everywhere. It is the single most recognisable element of the style.

### Header (rows 0–3)

- Rows 0–2: yellow fill, full width.
- Row 1: at column 2, `◆ PROTUI` (yellow on ink, bold); two spaces; `SSH KEYS · PROTON PASS` (ink, bold).
- Right-aligned, 2 columns from the edge: the count chip on violet, bold — `2/9 KEYS`, `1/2 OF 9 KEYS` while filtering, `NO KEYS` when empty. Keep the existing count semantics (position/total).
- Optional, 2 columns left of the count: an agent chip `● AGENT UP` on mint. Only if the agent status is already known. It is a new data need, so leave it out if it would add a call on every load.
- Row 3: `▀` in `ink` across the full width (a thick rule under the band).

### Section label (row 5)

An inverted chip — `KEYS` in ground-coloured text on `ink` — followed by a faint
description (`all vaults · sorted by title`). While filtering: `FILTER` on yellow,
then `matching “git” in 2 of 9 keys`.

### List row (height 2, spacing 1)

- **Unselected:** title bold `ink` at indent 3; next line, the summary in `faint`
  (`Vault  ·  <first 16 of fingerprint>…`); the algorithm chip right-aligned on the
  title line, one column in from the list's edge.
- **Selected:** the whole 2-row block is filled violet; `▶ title` bold ink; the summary
  in ink; the chip inverted (ink fill, ground text) on light terminals and keeps its
  algorithm fill on dark ones; a shadow with `dx = 2` (the spacing row absorbs it).
- **Filter matches:** matched characters get a yellow fill with ink bold text.
- When more rows exist below, show `↓ N more` in `faint` under the list.

### Detail pane

- Heavy box with a shadow (`dx = 2`). Same width split and same 92-column cut-off as today.
- Rows 1–3 inside: violet band. Title bold on the middle row at inner column 2; algorithm chip right-aligned.
- Row 4: tee rule `┣━━┫`.
- Label/value rows from row 6: labels uppercase, bold, `muted`, in a 14-column label column.
  - `VAULT` — value bold.
  - `FINGERPRINT` — `SHA256:…` on a yellow fill, ink bold, wrapping under the value column.
  - `COMMENT` — `—` in `faint` when empty.
  - `CREATED`, `MODIFIED`.
  - `ITEM ID` — `faint`, wrapping.
- Then the `PUBLIC KEY` inverted label chip followed by a thin `─` rule, then the key in a
  `code`-filled block with one cell of padding, hard-wrapped (keep the current mid-token wrap).
- Then a `c  COPY` button (yellow) and, beside it, `public key only — no private material` in `faint`.
- Keep the existing scroll behaviour and `↑ N more` / `↓ N more` markers.

### Status line (row above the footer)

A chip plus a bold message:

- success: `✓ COPIED` on mint, e.g. `github-personal → clipboard`
- warning: `!` on orange
- error: `✗ ERROR` on red, message in `danger`

### Footer key bar (last 3 rows)

- 3 rows filled with `bar`; keys on the middle row.
- Each key: a keycap `k` (yellow fill, ink bold) followed by ` label` in `bar_text`, 3 spaces between pairs. `D` gets a red keycap.
- **Filter mode:** a `/` keycap, the typed text bold, and the cursor as one solid ink cell. Right-hand hints use inverted keycaps (ink fill, yellow text): `enter apply  esc clear  ↑/↓ move`.
- `?` keeps expanding the full help, styled the same way.

### Dialog frame (new key, trash, delete, agent)

- **Backdrop:** instead of a blank screen, fill with `·` in `dots` at every 4th column,
  offset by 2 columns on odd rows.
- Centred heavy box, filled with ground, shadow `dx = 2`.
- Rows 1–3: a title band in the dialog's colour. Middle row: an icon chip (`+`, `d`, `a`, `!`) in
  the band colour on ink, then the title in bold caps.
- Row 4: tee rule. Body starts at row 6, indent 3. Buttons on the third-from-last row.

| Dialog        | Band   | Icon | Title                 |
| ------------- | ------ | ---- | --------------------- |
| New key (`n`) | violet | `+`  | `NEW SSH KEY`         |
| Trash (`d`)   | orange | `d`  | `MOVE TO TRASH?`      |
| Agent (`a`)   | mint   | `a`  | `SSH AGENT DAEMON`    |
| Delete (`D`)  | red    | `!`  | `PERMANENTLY DELETE?` |

**Delete is different:** band rows 1 and 3 are hazard stripes (2-column cells
alternating yellow / ink, row 3 shifted by one column), row 2 is red with the
title. On dark terminals its shadow is red. Body: a `THIS CANNOT BE UNDONE` red
chip, then the explanation in `danger`.

### Buttons

`key  LABEL` — bold ink on a fill, shadow `dx = 1`.

- Primary: yellow. Cancel: `code` fill.
- Trash confirm: orange. Agent stop: pink.
- Disabled: `disabled` fill, `#6B6352` text, no shadow. Use it for `START` while the agent runs,
  `STOP` while it is stopped, and `DELETE` until the typed title matches (then it turns red).

### Inputs (new-key form)

- Label column of 14, labels uppercase bold `muted`. The focused label gets a yellow fill.
- Unfocused text input: light box, value in `ink` or placeholder in `faint`.
- Focused text input: heavy box, shadow `dx = 1`, value bold, cursor as a solid ink cell.
- Vault selector: `‹ value ›` with faint arrows.
- Type selector: a segmented row — each option `  ED25519  ` on `code`, the selected one on mint, bold; `←/→` hint in `faint`.
- Passphrase stays masked with `•`.
- Under the fields, two `▸` notes in `muted` (comment lives in the public key; the passphrase goes via the environment).
- Buttons: `enter  CREATE` (yellow), `esc  CANCEL` (`code`), and a faint `tab  next field` on the right.
- The delete confirmation input: heavy box, **red** shadow `dx = 1`, typed-count `11/15` in `faint` on the right.

### Agent panel

- `STATUS` chip with a shadow `dx = 1`: running `● RUNNING` on mint; stopped `○ STOPPED` on `code`; degraded `◐ <detail>` on orange; unknown on orange.
- `PID` and `SOCKET` rows. When running, a `POINT SSH AT IT` inverted chip and the `export SSH_AUTH_SOCK=…` line in a `code` block.
- Buttons: `s START`, `x STOP` (pink), `r REFRESH` (yellow), `esc back` as plain muted text.

### Empty and error states

- Header and footer stay. Centred box (about 50×11) filled yellow, heavy border, shadow `dx = 2`:
  `NO SSH KEYS YET` bold, a one-line explanation, then `n  CREATE ONE` (violet button) and `r  reload`.
- Error: same box filled with ground, a red `✗ COULDN'T LOAD KEYS` chip, the error text, then `r  RETRY` (yellow) and `q  quit`.
- Loading: the same box shape with the spinner and `Loading keys…`.

### Narrow terminals

Below 92 columns, drop the detail pane (as today). Clamp dialog widths to
`width − 6` and let shadows clip at the edge.

## Reference screens

Character grids of each screen at 120×38, rendered from the design. Shadow
half-cells appear as `▀`/`▄`; fills are not visible in plain text, so read
these together with the component specs above.

### Key list (light)

```text

   ◆ PROTUI   SSH KEYS · PROTON PASS                                                           ● AGENT UP    2/9 KEYS

▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀

   KEYS  all vaults · sorted by title                        ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄
                                                             ┃                                                     ┃
     deploy-ci                                 ED25519       ┃  github-personal                          ED25519   ┃
     Work  ·  UvImZaYMEtKJGF2V…                              ┃                                                     ┃
                                                             ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
   ▶ github-personal                           ED25519  ▄▄   ┃                                                     ┃
     Personal  ·  oJXyD5OVZQz5OAuO…                          ┃  VAULT         Personal                             ┃
    ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ┃  FINGERPRINT   SHA256:oJXyD5OVZQz5OAuO2yJKaySKHpJO  ┃
     gitlab-work                               ED25519       ┃                j9CuLhqUkqMwXxg                      ┃
     Work  ·  jLYQkA+eNH+uiG3G…                              ┃  COMMENT       me@laptop                            ┃
                                                             ┃  CREATED       2026-03-14 09:12                     ┃
     hetzner-vps                               RSA4096       ┃  MODIFIED      2026-10-02 18:40                     ┃
     Infra  ·  unJJm/oSHoNrKsFX…                             ┃  ITEM ID       hH5buwf9B8pHeEIxsZr0WHLO77n8WfT5XRQ  ┃
                                                             ┃                4Gjp4MlY0e5_85pzXAHrop1jMpBXVqR7oY8  ┃
     homelab-nas                               ED25519       ┃                i2wDN64y1vyqJVFs3y                   ┃
     Infra  ·  lMx0EdcX8UV5sqoQ…                             ┃                                                     ┃
                                                             ┃   PUBLIC KEY  ────────────────────────────────────  ┃
     old-macbook                               RSA2048       ┃                                                     ┃
     Personal  ·  K5wdfg83xEkhvT9l…                          ┃   ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI4Zhjw1O4/H   ┃
                                                             ┃   4mSLmepCUL09W35IOgbbuzz4Ej6IbAgZE me@laptop       ┃
     raspberry-pi                              ED25519       ┃                                                     ┃
     Infra  ·  W67iYfU7JhUtJjuo…                             ┃                                                     ┃
                                                             ┃   c  COPY ▄  public key only — no private material  ┃
     work-laptop                               ED25519       ┃   ▀▀▀▀▀▀▀▀▀                                         ┃
     Work  ·  g/Oep629DXTm3sfz…                              ┃                                                     ┃
                                                             ┃                                                     ┃
                                                             ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
  ↓ 1 more                                                     ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
   ✓ COPIED  github-personal → clipboard


   j/k  move    /  filter    c  copy    n  new    d  trash    D  delete    a  agent    r  reload    ?  more    q  quit
```

### Filter (dark)

```text

   ◆ PROTUI   SSH KEYS · PROTON PASS                                                      ● AGENT UP    1/2 OF 9 KEYS

▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀

   FILTER  matching “git” in 2 of 9 keys                     ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄
                                                             ┃                                                     ┃
   ▶ github-personal                           ED25519  ▄▄   ┃  github-personal                          ED25519   ┃
     Personal  ·  oJXyD5OVZQz5OAuO…                          ┃                                                     ┃
    ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
     gitlab-work                               ED25519       ┃                                                     ┃
     Work  ·  jLYQkA+eNH+uiG3G…                              ┃  VAULT         Personal                             ┃
                                                             ┃  FINGERPRINT   SHA256:oJXyD5OVZQz5OAuO2yJKaySKHpJO  ┃
                                                             ┃                j9CuLhqUkqMwXxg                      ┃
  7 keys hidden by the filter                                ┃  COMMENT       me@laptop                            ┃
                                                             ┃  CREATED       2026-03-14 09:12                     ┃
                                                             ┃  MODIFIED      2026-10-02 18:40                     ┃
                                                             ┃  ITEM ID       hH5buwf9B8pHeEIxsZr0WHLO77n8WfT5XRQ  ┃
                                                             ┃                4Gjp4MlY0e5_85pzXAHrop1jMpBXVqR7oY8  ┃
                                                             ┃                i2wDN64y1vyqJVFs3y                   ┃
                                                             ┃                                                     ┃
                                                             ┃   PUBLIC KEY  ────────────────────────────────────  ┃
                                                             ┃                                                     ┃
                                                             ┃   ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI4Zhjw1O4/H   ┃
                                                             ┃   4mSLmepCUL09W35IOgbbuzz4Ej6IbAgZE me@laptop       ┃
                                                             ┃                                                     ┃
                                                             ┃                                                     ┃
                                                             ┃   c  COPY ▄  public key only — no private material  ┃
                                                             ┃   ▀▀▀▀▀▀▀▀▀                                         ┃
                                                             ┃                                                     ┃
                                                             ┃                                                     ┃
                                                             ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
                                                               ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀



   /  git                                                              enter  apply    esc  clear    ↑/↓  move
```

### New key

```text
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄  ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃   +  NEW SSH KEY                                                       ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄  ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  TITLE         ┃ staging-bastion                                   ┃   ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                 ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                ┌───────────────────────────────────────────────────┐   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  VAULT         │ ‹ Work                                          › │   ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                └───────────────────────────────────────────────────┘   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                                                                        ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  TYPE            ED25519     RSA2048     RSA4096    ←/→                ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                                                                        ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                ┌───────────────────────────────────────────────────┐   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  COMMENT       │ optional, e.g. user@host                          │   ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                └───────────────────────────────────────────────────┘   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                ┌───────────────────────────────────────────────────┐   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  PASSPHRASE    │ ••••••••••••                                      │   ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                └───────────────────────────────────────────────────┘   ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃                                                                        ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃  ▸ The comment is stored inside the public key itself.                 ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃  ▸ A passphrase goes via the environment, never argv.                  ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃                                                                        ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┃   enter  CREATE ▄   esc  CANCEL ▄                     tab  next field  ┃  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   · ┃   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀▀▀                                      ┃    ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛  ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
```

### Move to trash

```text
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃                                                          ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃   d  MOVE TO TRASH?                                      ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃                                                          ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃                                                          ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃  github-personal  ED25519                                ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃  in Personal                                             ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃                                                          ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃  Recoverable. Restore it later with:                     ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃                                                          ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃    $ pass-cli item untrash --share-id …                  ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃                                                          ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃                                                          ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃                                                          ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┃   y  TRASH ▄   n  CANCEL ▄                               ┃   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·  ┃   ▀▀▀▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀                               ┃     ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
```

### Permanently delete

```text
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃                                                                ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃   !  PERMANENTLY DELETE?                                       ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃                                                                ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃                                                                ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃  github-personal                                               ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃  in Personal                                                   ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃                                                                ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃   THIS CANNOT BE UNDONE                                        ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃  The key is destroyed, not moved to the trash.                 ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃                                                                ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃  TYPE THE TITLE TO CONFIRM                                     ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃  ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄   ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃  ┃ github-pers                                  11/15     ┃    ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃  ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛    ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃                                                                ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┃   enter  DELETE ▄   esc  CANCEL ▄  d  trashes instead          ┃  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   · ┃   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀▀▀                              ┃    ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛  ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
```

### SSH agent

```text
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃                                                                      ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃   a  SSH AGENT DAEMON                                                ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃                                                                      ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃                                                                      ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃  STATUS      ● RUNNING ▄                                             ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃              ▀▀▀▀▀▀▀▀▀▀▀                                             ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃  PID        48213                                                    ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃  SOCKET     ~/.local/share/proton-pass/ssh-agent.sock                ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃                                                                      ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃   POINT SSH AT IT                                                    ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃                                                                      ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃    export SSH_AUTH_SOCK=~/.local/share/proton-pass/ssh-agent.sock    ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃                                                                      ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃                                                                      ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┃   s  START    x  STOP ▄   r  REFRESH ▄  esc  back                    ┃     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·┃               ▀▀▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀▀                               ┃   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·  ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛     ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
 ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·   ·
```

### Empty state

```text

   ◆ PROTUI   SSH KEYS · PROTON PASS                                                                          NO KEYS

▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀








                                   ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓▄▄
                                   ┃                                                ┃
                                   ┃   NO SSH KEYS YET                              ┃
                                   ┃                                                ┃
                                   ┃   Nothing in any vault this session can see.   ┃
                                   ┃                                                ┃
                                   ┃                                                ┃
                                   ┃    n  CREATE ONE ▄  r  reload                  ┃
                                   ┃    ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀                             ┃
                                   ┃                                                ┃
                                   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
                                     ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀












   n  new    a  agent    r  reload    ?  more    q  quit
```
