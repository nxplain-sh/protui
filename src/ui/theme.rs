//! The palette, the fills and the glyphs the screens are drawn with.
//!
//! The look is neobrutalist (see `docs/design/protui-design.md` and
//! `docs/adr/0015-restyle-the-tui-as-neobrutalist.md`): heavy ink borders,
//! hard shadows, and a few saturated fills that always carry `#111111` text.
//! There are two themes. Their tokens differ — on a dark terminal, borders and
//! shadows flip to cream — but the fills are the same in both.
//!
//! The terminal's own background is the "ground" and is never painted; its
//! reference colour is used only as the text colour of inverted chips.

use std::env;
use std::io;

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode};

// Glyphs. Box drawing, block elements, geometric shapes and arrows render in
// the default fonts of common terminals; emoji and Nerd Font glyphs do not, so
// none are used.
pub const GLYPH_MARK: &str = "◆";
pub const GLYPH_POINTER: &str = "▶";
pub const GLYPH_RUNNING: &str = "●";
pub const GLYPH_STOPPED: &str = "○";
pub const GLYPH_DEGRADED: &str = "◐";
pub const GLYPH_OK: &str = "✓";
pub const GLYPH_ERROR: &str = "✗";
pub const GLYPH_WARN: &str = "!";
pub const GLYPH_PREVIOUS: &str = "‹";
pub const GLYPH_NEXT: &str = "›";
pub const GLYPH_SEPARATOR: &str = "·";
pub const GLYPH_NOTE: &str = "▸";
pub const GLYPH_UPPER_HALF: &str = "▀";
pub const GLYPH_LOWER_HALF: &str = "▄";
pub const GLYPH_FULL: &str = "█";
pub const GLYPH_LEFT_HALF: &str = "▌";
pub const GLYPH_RIGHT_HALF: &str = "▐";
/// The quadrants of a joined shadow's corner: upper right, and the lower half.
pub const GLYPH_SHADOW_CORNER: &str = "▟";
pub const GLYPH_THIN_RULE: &str = "─";

/// The pieces of a box outline.
#[derive(Clone, Copy, Debug)]
pub struct BoxGlyphs {
    pub top_left: &'static str,
    pub horizontal: &'static str,
    pub top_right: &'static str,
    pub vertical: &'static str,
    pub bottom_left: &'static str,
    pub bottom_right: &'static str,
    pub tee_left: &'static str,
}

/// The default outline: panes, dialogs and focused inputs.
pub const HEAVY: BoxGlyphs = BoxGlyphs {
    top_left: "┏",
    horizontal: "━",
    top_right: "┓",
    vertical: "┃",
    bottom_left: "┗",
    bottom_right: "┛",
    tee_left: "┣",
};

/// The outline of an input that does not have focus.
pub const LIGHT: BoxGlyphs = BoxGlyphs {
    top_left: "┌",
    horizontal: "─",
    top_right: "┐",
    vertical: "│",
    bottom_left: "└",
    bottom_right: "┘",
    tee_left: "├",
};

/// One token, as `[red, green, blue]`, for each background.
#[derive(Clone, Copy)]
struct Token {
    light: [u8; 3],
    dark: [u8; 3],
}

const INK: Token = Token {
    light: [0x11, 0x11, 0x11],
    dark: [0xf5, 0xf0, 0xe1],
};
const MUTED: Token = Token {
    light: [0x4f, 0x48, 0x3b],
    dark: [0xcf, 0xc8, 0xb6],
};
const FAINT: Token = Token {
    light: [0x6b, 0x63, 0x52],
    dark: [0xa3, 0x9c, 0x8c],
};
const CODE: Token = Token {
    light: [0xf1, 0xe6, 0xc8],
    dark: [0x26, 0x26, 0x26],
};
const DOTS: Token = Token {
    light: [0xe2, 0xd6, 0xb6],
    dark: [0x2c, 0x2c, 0x2c],
};
const DANGER: Token = Token {
    light: [0xb3, 0x12, 0x1f],
    dark: [0xff, 0x6b, 0x6b],
};
const BAR_TEXT: Token = Token {
    light: [0xff, 0xf8, 0xe7],
    dark: [0x11, 0x11, 0x11],
};
const GROUND: Token = Token {
    light: [0xff, 0xf8, 0xe7],
    dark: [0x14, 0x14, 0x14],
};

// The fills, the same in both themes, and the text every fill carries.
const FILL_TEXT: [u8; 3] = [0x11, 0x11, 0x11];
const YELLOW: [u8; 3] = [0xff, 0xd9, 0x3d];
const VIOLET: [u8; 3] = [0x8c, 0x6b, 0xff];
const MINT: [u8; 3] = [0x5b, 0xe3, 0xa6];
const PINK: [u8; 3] = [0xff, 0x7a, 0xb8];
const ORANGE: [u8; 3] = [0xff, 0xa2, 0x4c];
const RED: [u8; 3] = [0xff, 0x5a, 0x5a];
const DISABLED: [u8; 3] = [0xe6, 0xd9, 0xb8];
const DISABLED_TEXT: [u8; 3] = [0x6b, 0x63, 0x52];

/// The terminal's background, which decides each token's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Light,
    Dark,
}

/// How many colours the terminal can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    /// `NO_COLOR` is set: fills become reverse video, and weight and glyphs
    /// carry the rest.
    Monochrome,
    /// The xterm 256-colour palette.
    Indexed,
    /// 24-bit colour.
    TrueColor,
}

/// The resolved palette for one terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub background: Background,
    pub depth: ColorDepth,

    /// Body text and every border.
    pub ink: Color,
    /// Labels and secondary text.
    pub muted: Color,
    /// List summaries, hints and placeholders.
    pub faint: Color,
    /// Hard shadows.
    pub shadow: Color,
    /// The public-key block, command blocks and cancel buttons.
    pub code: Color,
    /// The decorative dots behind a dialog.
    pub dots: Color,
    /// Text on the ground for the permanent delete, and nothing else.
    pub danger: Color,
    /// The footer key bar's background, and its labels.
    pub bar: Color,
    pub bar_text: Color,
    /// The reference ground, used only as text on an inverted chip.
    pub ground: Color,

    /// The text every fill carries.
    pub fill_text: Color,
    pub yellow: Color,
    pub violet: Color,
    pub mint: Color,
    pub pink: Color,
    pub orange: Color,
    /// The permanent delete, and nothing else.
    pub red: Color,
    pub disabled: Color,
    pub disabled_text: Color,
}

impl Default for Theme {
    /// The dark, 24-bit theme: what most terminals get, and a fixed point for
    /// tests.
    fn default() -> Self {
        Self::new(Background::Dark, ColorDepth::TrueColor)
    }
}

impl Theme {
    /// Resolves the palette for a background and colour depth.
    #[must_use]
    pub fn new(background: Background, depth: ColorDepth) -> Self {
        let color = |[red, green, blue]: [u8; 3]| match depth {
            ColorDepth::Monochrome => Color::Reset,
            ColorDepth::Indexed => Color::Indexed(xterm_index(red, green, blue)),
            ColorDepth::TrueColor => Color::Rgb(red, green, blue),
        };
        let token = |token: Token| {
            color(match background {
                Background::Light => token.light,
                Background::Dark => token.dark,
            })
        };

        Self {
            background,
            depth,
            ink: token(INK),
            muted: token(MUTED),
            faint: token(FAINT),
            shadow: token(INK),
            code: token(CODE),
            dots: token(DOTS),
            danger: token(DANGER),
            bar: token(INK),
            bar_text: token(BAR_TEXT),
            ground: token(GROUND),
            fill_text: color(FILL_TEXT),
            yellow: color(YELLOW),
            violet: color(VIOLET),
            mint: color(MINT),
            pink: color(PINK),
            orange: color(ORANGE),
            red: color(RED),
            disabled: color(DISABLED),
            disabled_text: color(DISABLED_TEXT),
        }
    }

    /// Inspects the environment and the terminal to choose a palette.
    ///
    /// `PROTUI_THEME` wins when set. Otherwise the terminal is asked for its
    /// background, and its answer arrives on the same input as key presses, so
    /// this must run before anything else starts reading it.
    #[must_use]
    pub fn detect() -> Self {
        Self::new(detect_background(), detect_depth())
    }

    /// Chooses a palette without asking the terminal: `PROTUI_THEME` if set,
    /// otherwise dark.
    #[must_use]
    pub fn detect_without_asking() -> Self {
        Self::new(
            background_override().unwrap_or(Background::Dark),
            detect_depth(),
        )
    }

    #[must_use]
    pub fn is_dark(&self) -> bool {
        self.background == Background::Dark
    }

    #[must_use]
    pub fn is_monochrome(&self) -> bool {
        self.depth == ColorDepth::Monochrome
    }

    /// Text in `color` on the ground.
    #[must_use]
    #[expect(
        clippy::unused_self,
        reason = "a method like the other styles, so every call site reads the same"
    )]
    pub fn text(&self, color: Color) -> Style {
        Style::new().fg(color)
    }

    /// Bold text in `color` on the ground.
    #[must_use]
    pub fn bold(&self, color: Color) -> Style {
        self.text(color).add_modifier(Modifier::BOLD)
    }

    /// A coloured fill, which always carries `#111111` text. Without colour,
    /// a fill is reverse video.
    #[must_use]
    pub fn fill(&self, fill: Color) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(self.fill_text).bg(fill)
        }
    }

    /// A bold fill: chips, keycaps and buttons.
    #[must_use]
    pub fn chip(&self, fill: Color) -> Style {
        self.fill(fill).add_modifier(Modifier::BOLD)
    }

    /// An inverted chip: ground-coloured text on ink.
    #[must_use]
    pub fn inverted(&self) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
                .fg(self.ground)
                .bg(self.ink)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// A chip of `color` text on `#111111`, the same in both themes: the
    /// header wordmark, dialog icons and the filter hints.
    #[must_use]
    pub fn on_ink(&self, color: Color) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
                .fg(color)
                .bg(self.fill_text)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// A chip on `code` rather than a colour: cancel buttons, a stopped agent,
    /// an algorithm with no colour of its own. Its text is the theme's ink,
    /// since `code` is a shade of the ground, not a fill.
    #[must_use]
    pub fn code_chip(&self) -> Style {
        Style::new()
            .fg(self.ink)
            .bg(self.code)
            .add_modifier(Modifier::BOLD)
    }

    /// The disabled button's fill and text.
    #[must_use]
    pub fn disabled_button(&self) -> Style {
        if self.is_monochrome() {
            Style::new()
        } else {
            Style::new()
                .fg(self.disabled_text)
                .bg(self.disabled)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// The solid cell an input's cursor is drawn as.
    #[must_use]
    pub fn cursor(&self) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(self.ground).bg(self.ink)
        }
    }

    /// The filter's cursor, which sits on the key bar and so takes the bar's
    /// colours the other way round.
    #[must_use]
    pub fn bar_cursor(&self) -> Style {
        if self.is_monochrome() {
            Style::new().remove_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(self.bar).bg(self.bar_text)
        }
    }

    /// The backdrop's dots, which should recede. Without colour they are
    /// dimmed instead.
    #[must_use]
    pub fn decoration(&self) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::DIM)
        } else {
            Style::new().fg(self.dots)
        }
    }

    /// The footer key bar's labels on its background.
    #[must_use]
    pub fn bar_label(&self) -> Style {
        if self.is_monochrome() {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(self.bar_text).bg(self.bar)
        }
    }
}

/// Reports whether `NO_COLOR` asks for no colour, per <https://no-color.org>.
#[must_use]
pub fn no_color() -> bool {
    env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn detect_depth() -> ColorDepth {
    if no_color() {
        return ColorDepth::Monochrome;
    }

    match env::var("COLORTERM").as_deref() {
        Ok("truecolor" | "24bit") => ColorDepth::TrueColor,
        _ => ColorDepth::Indexed,
    }
}

/// Reads `PROTUI_THEME`, for terminals that do not answer the background
/// query — tmux or screen without passthrough, some SSH setups — and for
/// anyone who wants to choose.
fn background_override() -> Option<Background> {
    let value = env::var("PROTUI_THEME").ok()?;
    parse_background(&value)
}

fn parse_background(value: &str) -> Option<Background> {
    if value.eq_ignore_ascii_case("light") {
        Some(Background::Light)
    } else if value.eq_ignore_ascii_case("dark") {
        Some(Background::Dark)
    } else {
        None
    }
}

/// Chooses the background: `PROTUI_THEME` if set, otherwise the terminal's
/// answer to a background query, otherwise dark — the more common setup, and
/// the one a terminal that does not answer is likeliest to have.
fn detect_background() -> Background {
    use std::io::IsTerminal;

    if let Some(background) = background_override() {
        return background;
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Background::Dark;
    }

    match terminal_colorsaurus::theme_mode(QueryOptions::default()) {
        Ok(ThemeMode::Light) => Background::Light,
        Ok(ThemeMode::Dark) | Err(_) => Background::Dark,
    }
}

/// Maps a colour to the nearest entry of the xterm 256-colour palette, for
/// terminals without 24-bit colour, choosing between the 6×6×6 cube and the
/// grey ramp by distance.
fn xterm_index(red: u8, green: u8, blue: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

    let cube_step = |value: u8| -> u8 {
        match value {
            0..48 => 0,
            48..115 => 1,
            _ => (value - 35) / 40,
        }
    };
    let distance = |(r, g, b): (u8, u8, u8)| -> u32 {
        let delta = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
        delta(r, red) + delta(g, green) + delta(b, blue)
    };

    let (r, g, b) = (cube_step(red), cube_step(green), cube_step(blue));
    let cube = (
        LEVELS[usize::from(r)],
        LEVELS[usize::from(g)],
        LEVELS[usize::from(b)],
    );

    let average = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
    // The ramp runs 8, 18, …, 238 over indices 232..=255.
    let step = u8::try_from(average.saturating_sub(3) / 10)
        .unwrap_or(23)
        .min(23);
    let level = 8 + 10 * step;
    let grey = (level, level, level);

    if distance(grey) < distance(cube) {
        232 + step
    } else {
        16 + 36 * r + 6 * g + b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_index_hits_exact_palette_entries() {
        assert_eq!(xterm_index(0, 0, 0), 16);
        assert_eq!(xterm_index(255, 255, 255), 231);
        assert_eq!(xterm_index(255, 0, 0), 196);
        assert_eq!(xterm_index(0x58, 0x58, 0x58), 240);
    }

    /// Each fill must stay a colour, not collapse into grey, on a 256-colour
    /// terminal, and the fills must stay distinct from one another.
    #[test]
    fn fills_degrade_to_distinct_cube_colours() {
        let fills = [YELLOW, VIOLET, MINT, PINK, ORANGE, RED];
        let indices: Vec<u8> = fills
            .iter()
            .map(|&[r, g, b]| xterm_index(r, g, b))
            .collect();

        for (fill, index) in fills.iter().zip(&indices) {
            assert!(
                (16..232).contains(index),
                "{fill:02x?} fell onto the grey ramp ({index})"
            );
        }
        let mut distinct = indices.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), fills.len(), "fills collide: {indices:?}");
    }

    #[test]
    fn indexed_depth_sends_no_rgb() {
        let theme = Theme::new(Background::Light, ColorDepth::Indexed);

        assert!(matches!(theme.yellow, Color::Indexed(_)));
        assert!(matches!(theme.ink, Color::Indexed(_)));
    }

    #[test]
    fn dark_flips_borders_and_shadows_to_cream_but_keeps_the_fills() {
        let light = Theme::new(Background::Light, ColorDepth::TrueColor);
        let dark = Theme::new(Background::Dark, ColorDepth::TrueColor);

        assert_eq!(light.ink, Color::Rgb(0x11, 0x11, 0x11));
        assert_eq!(dark.ink, Color::Rgb(0xf5, 0xf0, 0xe1));
        assert_eq!(dark.shadow, dark.ink);
        assert_eq!(light.yellow, dark.yellow);
        assert_eq!(light.violet, dark.violet);
    }

    /// Rule 3 of the spec: a coloured fill always carries `#111111` text, in
    /// either theme.
    #[test]
    fn fills_carry_ink_text_in_both_themes() {
        for background in [Background::Light, Background::Dark] {
            let theme = Theme::new(background, ColorDepth::TrueColor);
            let style = theme.fill(theme.mint);

            assert_eq!(style.fg, Some(Color::Rgb(0x11, 0x11, 0x11)));
            assert_eq!(style.bg, Some(theme.mint));
        }
    }

    #[test]
    fn monochrome_fills_are_reverse_video() {
        let theme = Theme::new(Background::Dark, ColorDepth::Monochrome);

        assert_eq!(theme.ink, Color::Reset);
        assert!(
            theme
                .fill(theme.yellow)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn the_override_accepts_light_and_dark_in_any_case() {
        assert_eq!(parse_background("Light"), Some(Background::Light));
        assert_eq!(parse_background("DARK"), Some(Background::Dark));
        assert_eq!(parse_background("sepia"), None);
    }
}
