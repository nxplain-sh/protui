//! The palette, the styles built from it, and the glyphs drawn with them.
//!
//! The palette is one accent against a neutral ramp. The accent is Proton's
//! violet, which is both on-brand for a Proton Pass client and distinct from
//! the green/blue most terminal tools default to.
//!
//! Every colour is adaptive so the UI stays legible on light and dark
//! terminals, and each is a hex pair rather than an ANSI index so the two
//! themes can be tuned independently — the dark variants are lifted and
//! desaturated, since a colour that reads well on white is muddy on black.

use std::env;
use std::io;

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{QueryOptions, ThemeMode};

// Glyphs. Kept to widely-supported code points: box drawing, geometric shapes
// and arrows all render in the default fonts of common terminals, whereas
// emoji and Nerd Font glyphs do not.
pub const GLYPH_MARK: &str = "◆";
pub const GLYPH_SELECTED: &str = "▌";
pub const GLYPH_SEPARATOR: &str = "·";
pub const GLYPH_RUNNING: &str = "●";
pub const GLYPH_STOPPED: &str = "○";
pub const GLYPH_DEGRADED: &str = "◐";
pub const GLYPH_WARN: &str = "!";
pub const GLYPH_ERROR: &str = "✗";
pub const GLYPH_RULE: &str = "─";

/// One colour, as `[red, green, blue]`, tuned separately for each
/// background.
#[derive(Clone, Copy)]
struct Adaptive {
    light: [u8; 3],
    dark: [u8; 3],
}

const ACCENT: Adaptive = Adaptive {
    light: [0x5b, 0x34, 0xe0],
    dark: [0x9c, 0x82, 0xff],
};
const ACCENT_DIM: Adaptive = Adaptive {
    light: [0x8b, 0x6f, 0xef],
    dark: [0x6a, 0x55, 0xb8],
};
// The neutral ramp, brightest to dimmest.
const TEXT: Adaptive = Adaptive {
    light: [0x18, 0x18, 0x1b],
    dark: [0xe8, 0xe8, 0xed],
};
const MUTED: Adaptive = Adaptive {
    light: [0x5f, 0x65, 0x70],
    dark: [0x9a, 0x9a, 0xab],
};
const FAINT: Adaptive = Adaptive {
    light: [0x9c, 0xa3, 0xaf],
    dark: [0x61, 0x61, 0x6f],
};
const LINE: Adaptive = Adaptive {
    light: [0xe3, 0xe3, 0xe8],
    dark: [0x2c, 0x2c, 0x36],
};
// Backs badges and inline chips.
const SURFACE: Adaptive = Adaptive {
    light: [0xf1, 0xee, 0xfd],
    dark: [0x2a, 0x24, 0x38],
};
const OK: Adaptive = Adaptive {
    light: [0x15, 0x80, 0x3d],
    dark: [0x5d, 0xd6, 0x8a],
};
const WARN: Adaptive = Adaptive {
    light: [0xb4, 0x53, 0x09],
    dark: [0xf0, 0xb4, 0x29],
};
const ERROR: Adaptive = Adaptive {
    light: [0xb9, 0x1c, 0x1c],
    dark: [0xff, 0x7b, 0x72],
};

/// The terminal's background, which decides which half of each pair is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Light,
    Dark,
}

/// How many colours the terminal can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    /// `NO_COLOR` is set: emphasis comes from weight and glyphs alone.
    Monochrome,
    /// The xterm 256-colour palette.
    Indexed,
    /// 24-bit colour.
    TrueColor,
}

/// The resolved palette for one terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    accent: Color,
    accent_dim: Color,
    text: Color,
    muted: Color,
    faint: Color,
    line: Color,
    surface: Color,
    ok: Color,
    warn: Color,
    error: Color,
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
        let resolve = |pair: Adaptive| {
            let rgb = match background {
                Background::Light => pair.light,
                Background::Dark => pair.dark,
            };
            let [red, green, blue] = rgb;
            match depth {
                ColorDepth::Monochrome => Color::Reset,
                ColorDepth::Indexed => Color::Indexed(xterm_index(red, green, blue)),
                ColorDepth::TrueColor => Color::Rgb(red, green, blue),
            }
        };

        Self {
            accent: resolve(ACCENT),
            accent_dim: resolve(ACCENT_DIM),
            text: resolve(TEXT),
            muted: resolve(MUTED),
            faint: resolve(FAINT),
            line: resolve(LINE),
            surface: resolve(SURFACE),
            ok: resolve(OK),
            warn: resolve(WARN),
            error: resolve(ERROR),
        }
    }

    /// Inspects the environment and the terminal to choose a palette.
    ///
    /// The terminal's answer arrives on the same input as key presses, so this
    /// must run before anything else starts reading it.
    #[must_use]
    pub fn detect() -> Self {
        Self::new(detect_background(), detect_depth())
    }

    /// Chooses a palette from the environment alone, assuming a dark
    /// background, for when the terminal cannot be asked.
    #[must_use]
    pub fn detect_without_asking() -> Self {
        Self::new(Background::Dark, detect_depth())
    }

    #[must_use]
    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }

    #[must_use]
    pub fn accent_dim(&self) -> Style {
        Style::new().fg(self.accent_dim)
    }

    #[must_use]
    pub fn text(&self) -> Style {
        Style::new().fg(self.text)
    }

    #[must_use]
    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }

    #[must_use]
    pub fn faint(&self) -> Style {
        Style::new().fg(self.faint)
    }

    #[must_use]
    pub fn ok(&self) -> Style {
        Style::new().fg(self.ok)
    }

    #[must_use]
    pub fn warn(&self) -> Style {
        Style::new().fg(self.warn)
    }

    #[must_use]
    pub fn error(&self) -> Style {
        Style::new().fg(self.error)
    }

    /// The product name in the header. There is no filled banner: a coloured
    /// block across the top of a terminal reads as chrome, and the list is
    /// what deserves attention.
    #[must_use]
    pub fn wordmark(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    /// Labels a group inside the detail pane. Uppercase and faint rather than
    /// bold and bright, so it separates without competing.
    #[must_use]
    pub fn section(&self) -> Style {
        self.faint()
    }

    /// The left column of a key/value row.
    #[must_use]
    pub fn label(&self) -> Style {
        self.muted()
    }

    /// The highlighted row.
    #[must_use]
    pub fn selected(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    /// An inline chip, used for the key algorithm.
    #[must_use]
    pub fn badge(&self) -> Style {
        self.accent().bg(self.surface)
    }

    /// A dialog's heading.
    #[must_use]
    pub fn title(&self) -> Style {
        self.text().add_modifier(Modifier::BOLD)
    }

    /// The heading of the dialog for an irreversible action.
    #[must_use]
    pub fn danger_title(&self) -> Style {
        self.error().add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn rule(&self) -> Style {
        Style::new().fg(self.line)
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

/// Asks the terminal for its background, assuming dark if it cannot say:
/// that is the more common setup, and the one every terminal that does not
/// answer is likeliest to have.
fn detect_background() -> Background {
    use std::io::IsTerminal;

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

    #[test]
    fn xterm_index_keeps_the_accent_violet() {
        // [0x9c, 0x82, 0xff] lands in the cube's blue-violet corner, not the grey ramp.
        let index = xterm_index(0x9c, 0x82, 0xff);
        assert!((16..232).contains(&index), "{index} is not a cube colour");
    }

    #[test]
    fn monochrome_uses_no_colour_at_all() {
        let theme = Theme::new(Background::Dark, ColorDepth::Monochrome);

        assert_eq!(theme.accent().fg, Some(Color::Reset));
        // Weight still carries emphasis.
        assert!(theme.selected().add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn backgrounds_pick_their_half_of_each_pair() {
        let light = Theme::new(Background::Light, ColorDepth::TrueColor);
        let dark = Theme::new(Background::Dark, ColorDepth::TrueColor);

        assert_eq!(light.text().fg, Some(Color::Rgb(0x18, 0x18, 0x1b)));
        assert_eq!(dark.text().fg, Some(Color::Rgb(0xe8, 0xe8, 0xed)));
    }
}
