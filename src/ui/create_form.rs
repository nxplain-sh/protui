//! The form that collects the inputs for `item create ssh-key generate`.
//!
//! The passphrase is optional. When set it reaches pass-cli through the child
//! environment only — never argv, never disk. See `docs/schema.md` §6.1.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use crate::keys::{ShareId, Vault};
use crate::passcli::{GenerateRequest, KeyType};
use crate::ui::draw;
use crate::ui::text;
use crate::ui::text_input::{Echo, TextInput};
use crate::ui::theme::{GLYPH_NEXT, GLYPH_NOTE, GLYPH_PREVIOUS, GLYPH_WARN, HEAVY, LIGHT, Theme};

/// The label column, which every input lines up after.
const LABEL_WIDTH: u16 = 14;
/// A boxed field's rows: the box, then a row its shadow falls into.
const FIELD_ROWS: u16 = 4;
/// What the form says under its fields.
const NOTES: [&str; 2] = [
    "The comment is stored inside the public key itself.",
    "A passphrase goes via the environment, never the command line.",
];

/// A form field, in tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Vault,
    KeyType,
    Comment,
    Passphrase,
}

impl Field {
    const ORDER: [Self; 5] = [
        Self::Title,
        Self::Vault,
        Self::KeyType,
        Self::Comment,
        Self::Passphrase,
    ];

    fn position(self) -> usize {
        Self::ORDER
            .iter()
            .position(|&field| field == self)
            .unwrap_or(0)
    }

    fn next(self) -> Self {
        Self::ORDER[(self.position() + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        Self::ORDER[(self.position() + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }

    /// Choices cycle with the arrow keys instead of taking text.
    fn is_choice(self) -> bool {
        matches!(self, Self::Vault | Self::KeyType)
    }
}

/// The new-key form.
#[derive(Debug)]
pub struct CreateForm {
    focus: Field,
    title: TextInput,
    comment: TextInput,
    passphrase: TextInput,
    vaults: Vec<Vault>,
    vault_index: usize,
    key_type_index: usize,
    error: Option<&'static str>,
}

impl CreateForm {
    /// Builds the form, defaulting to the vault the cursor was on so that
    /// creating a key next to an existing one takes no extra keystrokes.
    #[must_use]
    pub fn new(vaults: Vec<Vault>, selected: Option<&ShareId>) -> Self {
        let vault_index = selected
            .and_then(|share_id| vaults.iter().position(|vault| vault.share_id == *share_id))
            .unwrap_or(0);

        Self {
            focus: Field::Title,
            title: TextInput::new("laptop", 128, Echo::Normal),
            comment: TextInput::new("optional, e.g. user@host", 128, Echo::Normal),
            passphrase: TextInput::new("optional", 256, Echo::Masked),
            vaults,
            vault_index,
            key_type_index: 0,
            error: None,
        }
    }

    /// Handles a key press, returning the request once the form is submitted.
    pub fn handle_key(&mut self, event: &KeyEvent) -> Option<GenerateRequest> {
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        let bare = !event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        match event.code {
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Char('n') if control => self.focus = self.focus.next(),
            KeyCode::BackTab => self.focus = self.focus.previous(),
            KeyCode::Char('p') if control => self.focus = self.focus.previous(),
            KeyCode::Down if !self.focus.is_choice() => self.focus = self.focus.next(),
            KeyCode::Up if !self.focus.is_choice() => self.focus = self.focus.previous(),
            // h and l only cycle on the choice fields; elsewhere they are text.
            KeyCode::Left if self.focus.is_choice() => self.cycle(Direction::Back),
            KeyCode::Char('h') if bare && self.focus.is_choice() => self.cycle(Direction::Back),
            KeyCode::Right if self.focus.is_choice() => self.cycle(Direction::Forward),
            KeyCode::Char('l') if bare && self.focus.is_choice() => self.cycle(Direction::Forward),
            KeyCode::Enter => return self.submit(),
            _ => {
                // Anything else is text, and only the focused input sees it.
                if let Some(input) = self.focused_input() {
                    input.handle_key(event);
                }
                self.error = None;
            }
        }

        None
    }

    /// Inserts pasted text into the focused text field, if there is one.
    pub fn paste(&mut self, text: &str) {
        if let Some(input) = self.focused_input() {
            input.paste(text);
        }
        self.error = None;
    }

    fn cycle(&mut self, direction: Direction) {
        let step = |index: usize, len: usize| match direction {
            Direction::Forward => (index + 1) % len,
            Direction::Back => (index + len - 1) % len,
        };

        match self.focus {
            Field::Vault if !self.vaults.is_empty() => {
                self.vault_index = step(self.vault_index, self.vaults.len());
            }
            Field::KeyType => self.key_type_index = step(self.key_type_index, KeyType::ALL.len()),
            _ => {}
        }
    }

    fn focused_input(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            Field::Title => Some(&mut self.title),
            Field::Comment => Some(&mut self.comment),
            Field::Passphrase => Some(&mut self.passphrase),
            Field::Vault | Field::KeyType => None,
        }
    }

    /// Validates the form and builds the pass-cli request from it.
    ///
    /// The passphrase is moved into the request rather than copied, and the
    /// caller drops the form immediately afterwards, so protui keeps no copy
    /// of its own.
    fn submit(&mut self) -> Option<GenerateRequest> {
        let title = self.title.value().trim();
        if title.is_empty() {
            self.error = Some("A title is required.");
            return None;
        }
        let Some(vault) = self.vaults.get(self.vault_index) else {
            self.error = Some("No vault available.");
            return None;
        };

        Some(GenerateRequest {
            title: title.to_owned(),
            share_id: vault.share_id.clone(),
            comment: self.comment.value().trim().to_owned(),
            key_type: KeyType::ALL[self.key_type_index],
            passphrase: self.passphrase.take_value(),
        })
    }

    /// The rows the form's body takes: boxed fields, or single-row ones when
    /// `compact`, for a terminal too short for the boxes.
    #[must_use]
    pub fn height(&self, compact: bool) -> u16 {
        let fields = Field::ORDER.len() as u16;
        let rows = if compact {
            // A blank row between fields, and one before the notes.
            fields * 2
        } else {
            fields * FIELD_ROWS
        };
        let error = if self.error.is_some() { 2 } else { 0 };

        rows + NOTES.len() as u16 + error
    }

    /// Draws the form into `area` and returns where the terminal cursor
    /// belongs, if a text field has focus.
    pub fn render(
        &self,
        buf: &mut Buffer,
        area: Rect,
        compact: bool,
        theme: &Theme,
    ) -> Option<Position> {
        let input_width = area.width.saturating_sub(LABEL_WIDTH + 1);
        let mut cursor = None;
        let mut y = area.y;

        for field in Field::ORDER {
            let focused = field == self.focus;
            let input = Rect::new(
                area.x + LABEL_WIDTH,
                y,
                input_width,
                if compact { 1 } else { 3 },
            );
            let middle = input.y + input.height / 2;
            Self::label(buf, area.x, middle, field, focused, theme);

            let typed = match field {
                Field::Title => Some(&self.title),
                Field::Comment => Some(&self.comment),
                Field::Passphrase => Some(&self.passphrase),
                Field::Vault => {
                    self.vault_box(buf, input, focused, theme);
                    None
                }
                Field::KeyType => {
                    self.key_types(buf, input.x, middle, area.right(), theme);
                    None
                }
            };
            if let Some(typed) = typed {
                cursor = cursor.or(Self::text_box(buf, input, typed, focused, theme));
            }
            y += if compact { 2 } else { FIELD_ROWS };
        }

        for note in NOTES {
            let note = format!("{GLYPH_NOTE} {note}");
            draw::text(buf, area.x, y, &note, theme.text(theme.muted), area.width);
            y += 1;
        }

        if let Some(error) = self.error {
            y += 1;
            let chip = draw::chip(buf, area.x, y, GLYPH_WARN, theme.chip(theme.orange));
            let room = area.width.saturating_sub(chip + 1);
            draw::text(
                buf,
                area.x + chip + 1,
                y,
                error,
                theme.bold(theme.ink),
                room,
            );
        }

        cursor
    }

    /// The field's name. Focus fills it yellow, one column wider each side, so
    /// the text stays in line with the labels around it.
    fn label(buf: &mut Buffer, x: u16, y: u16, field: Field, focused: bool, theme: &Theme) {
        let label = match field {
            Field::Title => "TITLE",
            Field::Vault => "VAULT",
            Field::KeyType => "TYPE",
            Field::Comment => "COMMENT",
            Field::Passphrase => "PASSPHRASE",
        };

        if focused {
            draw::chip(buf, x.saturating_sub(1), y, label, theme.chip(theme.yellow));
        } else {
            draw::text(buf, x, y, label, theme.bold(theme.muted), LABEL_WIDTH);
        }
    }

    /// Draws an input's frame and returns where its text goes and how wide it
    /// may be. A one-row input is a `code`-filled well instead of a box.
    fn frame(buf: &mut Buffer, area: Rect, focused: bool, theme: &Theme) -> (u16, u16, u16) {
        if area.height == 1 {
            draw::fill(buf, area, Style::new().bg(theme.code));
            return (area.x + 1, area.y, area.width.saturating_sub(2));
        }

        let ink = theme.text(theme.ink);
        if focused {
            draw::outline_with_shadow(buf, area, &HEAVY, ink, theme.shadow);
        } else {
            draw::outline(buf, area, &LIGHT, ink);
        }
        (area.x + 2, area.y + 1, area.width.saturating_sub(4))
    }

    /// A text field: its value, or its placeholder, and when focused a solid
    /// cursor cell, whose position is returned.
    fn text_box(
        buf: &mut Buffer,
        area: Rect,
        input: &TextInput,
        focused: bool,
        theme: &Theme,
    ) -> Option<Position> {
        let (x, y, width) = Self::frame(buf, area, focused, theme);
        let style = if focused {
            theme.bold(theme.ink)
        } else {
            theme.text(theme.ink)
        };
        let (spans, column) = input.render(style, theme.text(theme.faint), usize::from(width));
        draw::line(buf, x, y, &Line::from(spans), width);

        if !focused {
            return None;
        }
        let position = Position::new(x + column, y);
        draw::restyle(buf, position.x, position.y, theme.cursor());
        Some(position)
    }

    /// The vault, between the arrows that cycle it.
    fn vault_box(&self, buf: &mut Buffer, area: Rect, focused: bool, theme: &Theme) {
        let vault = self
            .vaults
            .get(self.vault_index)
            .map_or("(none)", |vault| vault.name.as_str());
        let (x, y, width) = Self::frame(buf, area, focused, theme);
        let style = if focused {
            theme.bold(theme.ink)
        } else {
            theme.text(theme.ink)
        };
        let arrows = theme.text(theme.faint);

        let room = width.saturating_sub(4);
        draw::text(buf, x, y, GLYPH_PREVIOUS, arrows, width);
        draw::text(
            buf,
            x + 2,
            y,
            &text::truncate(vault, usize::from(room)),
            style,
            room,
        );
        if width > 4 {
            draw::text(buf, x + width - 1, y, GLYPH_NEXT, arrows, 1);
        }
    }

    /// The key types as a segmented row, the chosen one filled.
    fn key_types(&self, buf: &mut Buffer, x: u16, y: u16, right: u16, theme: &Theme) {
        let mut x = x;
        for (index, key_type) in KeyType::ALL.iter().enumerate() {
            let style = if index == self.key_type_index {
                theme.chip(theme.mint)
            } else {
                Style::new().fg(theme.ink).bg(theme.code)
            };
            let segment = format!("  {}  ", key_type.as_str().to_uppercase());
            x = draw::text(buf, x, y, &segment, style, right.saturating_sub(x)) + 1;
        }
        draw::text(
            buf,
            x + 1,
            y,
            "←/→",
            theme.text(theme.faint),
            right.saturating_sub(x + 1),
        );
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Back,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vaults() -> Vec<Vault> {
        ["Personal", "Work"]
            .map(|name| Vault {
                name: name.to_owned(),
                share_id: ShareId::from(name),
                ..Vault::default()
            })
            .to_vec()
    }

    fn press(form: &mut CreateForm, code: KeyCode) -> Option<GenerateRequest> {
        form.handle_key(&KeyEvent::from(code))
    }

    fn type_text(form: &mut CreateForm, text: &str) {
        for c in text.chars() {
            press(form, KeyCode::Char(c));
        }
    }

    #[test]
    fn defaults_to_the_selected_vault() {
        let form = CreateForm::new(vaults(), Some(&ShareId::from("Work")));

        assert_eq!(form.vault_index, 1);
    }

    #[test]
    fn submits_a_trimmed_request_with_the_chosen_settings() {
        let mut form = CreateForm::new(vaults(), None);
        type_text(&mut form, "  laptop  ");
        press(&mut form, KeyCode::Tab);
        press(&mut form, KeyCode::Right);
        press(&mut form, KeyCode::Tab);
        press(&mut form, KeyCode::Char('l'));
        press(&mut form, KeyCode::Char('l'));
        press(&mut form, KeyCode::Tab);
        type_text(&mut form, "me@host");
        press(&mut form, KeyCode::Tab);
        type_text(&mut form, "hunter2");

        let request = press(&mut form, KeyCode::Enter).expect("the form submits");

        assert_eq!(request.title, "laptop");
        assert_eq!(request.share_id, ShareId::from("Work"));
        assert_eq!(request.key_type, KeyType::Rsa4096);
        assert_eq!(request.comment, "me@host");
        assert_eq!(request.passphrase.as_str(), "hunter2");
        // Moved, not copied: the form no longer holds the secret.
        assert_eq!(form.passphrase.value(), "");
    }

    #[test]
    fn h_and_l_are_text_outside_the_choices() {
        let mut form = CreateForm::new(vaults(), None);

        type_text(&mut form, "hl");

        assert_eq!(form.title.value(), "hl");
    }

    #[test]
    fn a_blank_title_is_refused() {
        let mut form = CreateForm::new(vaults(), None);
        type_text(&mut form, "   ");

        assert!(press(&mut form, KeyCode::Enter).is_none());
        assert_eq!(form.error, Some("A title is required."));

        // Typing again clears the complaint.
        type_text(&mut form, "x");
        assert_eq!(form.error, None);
    }

    #[test]
    fn no_vault_is_refused() {
        let mut form = CreateForm::new(Vec::new(), None);
        type_text(&mut form, "laptop");

        assert!(press(&mut form, KeyCode::Enter).is_none());
        assert_eq!(form.error, Some("No vault available."));
    }

    fn render(form: &CreateForm, compact: bool) -> Option<Position> {
        let mut buf = Buffer::empty(Rect::new(0, 0, 68, form.height(compact)));
        let area = buf.area;
        form.render(&mut buf, area, compact, &Theme::default())
    }

    #[test]
    fn the_cursor_sits_in_the_focused_text_field() {
        let mut form = CreateForm::new(vaults(), None);
        type_text(&mut form, "ab");

        // The label column, the box's border and a space, then two
        // characters typed, on the box's middle row.
        assert_eq!(render(&form, false), Some(Position::new(18, 1)));
        // The well has no border, only a column of padding.
        assert_eq!(render(&form, true), Some(Position::new(17, 0)));

        press(&mut form, KeyCode::Tab);
        assert_eq!(render(&form, false), None, "a choice has no text cursor");
    }

    #[test]
    fn the_form_fits_the_height_it_reports() {
        let mut form = CreateForm::new(vaults(), None);
        press(&mut form, KeyCode::Enter);
        assert!(form.error.is_some());

        assert_eq!(form.height(false), 24);
        assert_eq!(form.height(true), 14);
    }
}
