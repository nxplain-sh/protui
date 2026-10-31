//! The pieces every screen is drawn from: text, fills, box outlines, hard
//! shadows, chips, buttons, the dotted backdrop and hazard stripes.
//!
//! Each one writes straight into a [`Buffer`] and clips at its edges, so a box
//! that does not fit a small terminal is cut off rather than panicking.
//!
//! None of them filters what it draws. Untrusted text is sanitised where it
//! enters protui — `passcli` for anything from Proton Pass, `TextInput` for
//! anything typed or pasted — so it is already safe by the time it gets here.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::ui::text;
use crate::ui::theme::{
    BoxGlyphs, GLYPH_FULL, GLYPH_LOWER_HALF, GLYPH_SEPARATOR, GLYPH_UPPER_HALF, Theme,
};

/// Writes `content` at `(x, y)` in at most `width` columns, and returns the
/// column after it.
pub fn text(buf: &mut Buffer, x: u16, y: u16, content: &str, style: Style, width: u16) -> u16 {
    if !on_screen(buf, x, y) {
        return x;
    }

    buf.set_stringn(x, y, content, usize::from(width), style).0
}

/// Writes a styled line at `(x, y)` in at most `width` columns.
pub fn line(buf: &mut Buffer, x: u16, y: u16, content: &Line<'_>, width: u16) {
    if on_screen(buf, x, y) {
        buf.set_line(x, y, content, width);
    }
}

/// Blanks `area` and gives every cell `style`, so a fill replaces whatever
/// was drawn there before.
pub fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    for position in area.intersection(buf.area).positions() {
        if let Some(cell) = buf.cell_mut(position) {
            cell.reset();
            cell.set_style(style);
        }
    }
}

/// Restores `area` to the terminal's own background.
pub fn clear(buf: &mut Buffer, area: Rect) {
    fill(buf, area, Style::new());
}

/// Restyles one cell, keeping its symbol: the solid cursor cell of an input.
pub fn restyle(buf: &mut Buffer, x: u16, y: u16, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_style(style);
    }
}

/// Draws the outline of `area`.
pub fn outline(buf: &mut Buffer, area: Rect, glyphs: &BoxGlyphs, style: Style) {
    if area.width < 2 || area.height < 2 {
        return;
    }

    let (left, top) = (area.left(), area.top());
    let (right, bottom) = (area.right() - 1, area.bottom() - 1);
    for x in left + 1..right {
        symbol(buf, x, top, glyphs.horizontal, style);
        symbol(buf, x, bottom, glyphs.horizontal, style);
    }
    for y in top + 1..bottom {
        symbol(buf, left, y, glyphs.vertical, style);
        symbol(buf, right, y, glyphs.vertical, style);
    }
    symbol(buf, left, top, glyphs.top_left, style);
    symbol(buf, right, top, glyphs.top_right, style);
    symbol(buf, left, bottom, glyphs.bottom_left, style);
    symbol(buf, right, bottom, glyphs.bottom_right, style);
}

/// Draws a rule across the box `area` at row `y`, joined to its sides.
pub fn tee(buf: &mut Buffer, area: Rect, y: u16, glyphs: &BoxGlyphs, style: Style) {
    if area.width < 2 {
        return;
    }

    let right = area.right() - 1;
    symbol(buf, area.left(), y, glyphs.tee_left, style);
    for x in area.left() + 1..right {
        symbol(buf, x, y, glyphs.horizontal, style);
    }
    symbol(buf, right, y, glyphs.tee_right, style);
}

/// Draws the hard shadow of `area`, `dx` columns to the right and half a row
/// down: a lower half-block beside the top row, solid cells beside the rest,
/// and upper half-blocks along the bottom.
pub fn shadow(buf: &mut Buffer, area: Rect, dx: u16, color: Color) {
    let right = area.right();
    for x in right..right.saturating_add(dx) {
        shade(buf, x, area.top(), GLYPH_LOWER_HALF, color);
        for y in area.top() + 1..area.bottom() {
            solid(buf, x, y, color);
        }
    }
    for x in area.left().saturating_add(dx)..right.saturating_add(dx) {
        shade(buf, x, area.bottom(), GLYPH_UPPER_HALF, color);
    }
}

/// Draws ` label ` and returns its width.
pub fn chip(buf: &mut Buffer, x: u16, y: u16, label: &str, style: Style) -> u16 {
    let padded = format!(" {label} ");
    text(buf, x, y, &padded, style, u16::MAX);
    text::width(&padded) as u16
}

/// Draws the button ` key  LABEL ` and returns its width. Given a style it is
/// live, with a shadow one column deep; without one it is disabled and flat.
pub fn button(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    key: &str,
    label: &str,
    style: Option<Style>,
    theme: &Theme,
) -> u16 {
    let live = style.is_some();
    let style = style.unwrap_or_else(|| theme.disabled_button());
    let width = chip(buf, x, y, &format!("{key}  {label}"), style);
    if live {
        shadow(buf, Rect::new(x, y, width, 1), 1, theme.shadow);
    }
    width
}

/// Where the next button in a row starts: past the shadow, and two columns
/// of space.
#[must_use]
pub fn after_button(x: u16, width: u16) -> u16 {
    x.saturating_add(width + 3)
}

/// Covers `area` with the dotted pattern drawn behind a dialog: a dot every
/// fourth column, shifted two columns on odd rows.
pub fn backdrop(buf: &mut Buffer, area: Rect, style: Style) {
    clear(buf, area);
    for y in area.top()..area.bottom() {
        let first = if y % 2 == 0 { 1 } else { 3 };
        for x in (area.left()..area.right()).filter(|x| x % 4 == first) {
            symbol(buf, x, y, GLYPH_SEPARATOR, style);
        }
    }
}

/// Draws a row of hazard stripes: two-column cells alternating yellow and
/// `#111111`, offset by `shift` columns.
pub fn hazard(buf: &mut Buffer, x: u16, y: u16, width: u16, shift: u16, theme: &Theme) {
    for column in 0..width {
        let style = if (column + shift) % 4 < 2 {
            theme.fill(theme.yellow)
        } else {
            Style::new().bg(theme.fill_text)
        };
        symbol(buf, x.saturating_add(column), y, " ", style);
    }
}

fn on_screen(buf: &Buffer, x: u16, y: u16) -> bool {
    buf.area.contains(Position::new(x, y))
}

fn symbol(buf: &mut Buffer, x: u16, y: u16, glyph: &str, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(glyph).set_style(style);
    }
}

/// A half-block shadow cell keeps whatever background is behind it, so the
/// other half sits on the ground.
fn shade(buf: &mut Buffer, x: u16, y: u16, glyph: &str, color: Color) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(glyph).set_fg(color);
        cell.modifier = Modifier::empty();
    }
}

/// A solid shadow cell is a background fill, which every font draws edge to
/// edge. Without colour there is no fill, so it is a full block instead.
fn solid(buf: &mut Buffer, x: u16, y: u16, color: Color) {
    if color == Color::Reset {
        shade(buf, x, y, GLYPH_FULL, color);
    } else if let Some(cell) = buf.cell_mut((x, y)) {
        cell.reset();
        cell.set_bg(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{Background, ColorDepth, HEAVY};

    fn rows(buf: &Buffer) -> Vec<String> {
        (buf.area.top()..buf.area.bottom())
            .map(|y| {
                (buf.area.left()..buf.area.right())
                    .map(|x| buf[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_shadow_sits_beside_and_below_its_box() {
        for (dx, want) in [
            (1, ["      ", " ▄    ", " █    ", " ▀    "]),
            (2, ["      ", " ▄▄   ", " ██   ", "  ▀   "]),
        ] {
            let mut buf = Buffer::empty(Rect::new(0, 0, 6, 4));

            shadow(&mut buf, Rect::new(0, 1, 1, 2), dx, Color::Reset);

            assert_eq!(rows(&buf), want, "dx = {dx}");
        }
    }

    #[test]
    fn a_shadow_clips_at_the_right_and_bottom_edges() {
        for dx in [1, 2] {
            let mut buf = Buffer::empty(Rect::new(0, 0, 4, 3));

            // The box reaches the bottom-right corner, so its shadow falls
            // entirely off screen; drawing it must not panic.
            shadow(&mut buf, Rect::new(2, 1, 2, 2), dx, Color::Reset);

            assert_eq!(rows(&buf), ["    ", "    ", "    "], "dx = {dx}");
        }
    }

    #[test]
    fn an_outline_and_tee_use_the_given_glyphs() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 4));
        let area = buf.area;

        outline(&mut buf, area, &HEAVY, Style::new());
        tee(&mut buf, area, 2, &HEAVY, Style::new());

        assert_eq!(rows(&buf), ["┏━━┓", "┃  ┃", "┣━━┫", "┗━━┛"]);
    }

    #[test]
    fn backdrop_dots_fall_every_fourth_column_shifted_on_odd_rows() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 9, 3));
        let area = buf.area;

        backdrop(&mut buf, area, Style::new());

        assert_eq!(rows(&buf), [" ·   ·   ", "   ·   · ", " ·   ·   "]);
    }

    #[test]
    fn hazard_stripes_alternate_in_pairs_and_shift_by_one() {
        let theme = Theme::new(Background::Light, ColorDepth::TrueColor);
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 2));

        hazard(&mut buf, 0, 0, 6, 0, &theme);
        hazard(&mut buf, 0, 1, 6, 1, &theme);

        let yellow = |x, y| buf[(x, y)].bg == theme.yellow;
        assert_eq!(
            (0..6).map(|x| yellow(x, 0)).collect::<Vec<_>>(),
            [true, true, false, false, true, true]
        );
        assert_eq!(
            (0..6).map(|x| yellow(x, 1)).collect::<Vec<_>>(),
            [true, false, false, true, true, false]
        );
    }

    #[test]
    fn a_disabled_button_is_flat_and_a_live_one_casts_a_shadow() {
        let theme = Theme::new(Background::Light, ColorDepth::TrueColor);
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 4));

        let width = button(&mut buf, 0, 0, "s", "GO", None, &theme);
        button(
            &mut buf,
            0,
            2,
            "s",
            "GO",
            Some(theme.chip(theme.mint)),
            &theme,
        );

        assert_eq!(width, 7);
        assert_eq!(
            rows(&buf),
            [
                " s  GO      ",
                "            ",
                " s  GO ▄    ",
                " ▀▀▀▀▀▀▀    "
            ]
        );
        assert_eq!(buf[(1, 0)].bg, theme.disabled);
        assert_eq!(buf[(1, 2)].bg, theme.mint);
        assert_eq!(buf[(1, 2)].fg, Color::Rgb(0x11, 0x11, 0x11));
    }

    #[test]
    fn drawing_off_screen_is_a_no_op() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));

        assert_eq!(text(&mut buf, 0, 5, "below", Style::new(), 10), 0);
        line(&mut buf, 9, 0, &Line::raw("right"), 10);
        fill(&mut buf, Rect::new(2, 0, 10, 10), Style::new());

        assert_eq!(rows(&buf), ["   "]);
    }
}
