//! The form that collects the inputs for `item create ssh-key generate`.
//!
//! The passphrase is optional. When set it reaches pass-cli through the child
//! environment only — never argv, never disk. See `docs/schema.md` §6.1.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;
use ratatui::text::{Line, Span};

use crate::keys::{ShareId, Vault};
use crate::passcli::{GenerateRequest, KeyType};
use crate::ui::text_input::{Echo, TextInput};
use crate::ui::theme::{GLYPH_RULE, GLYPH_SELECTED, GLYPH_WARN, Theme};

/// The width of the input column, so every field's underline lines up
/// regardless of how much has been typed into it.
const FIELD_WIDTH: usize = 34;
const LABEL_WIDTH: usize = 12;
const GUTTER_WIDTH: usize = 2;
/// Every value sits two columns into its field, where a focused choice draws
/// its left arrow.
const VALUE_INDENT: usize = 2;

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

    /// Moves focus to `field`, as the preview does to show a choice.
    #[cfg(test)]
    pub fn focus(&mut self, field: Field) {
        self.focus = field;
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

    /// Renders the form's body, and where the terminal cursor belongs within
    /// it, relative to the body's first line, if a text field has focus.
    #[must_use]
    pub fn render(&self, theme: &Theme) -> (Vec<Line<'static>>, Option<Position>) {
        let key_type = KeyType::ALL[self.key_type_index].as_str();
        let vault = self
            .vaults
            .get(self.vault_index)
            .map_or("(none)", |vault| vault.name.as_str());

        let mut lines = Vec::new();
        let mut cursor = None;

        for field in Field::ORDER {
            let (label, value) = match field {
                Field::Title => ("Title", Self::input_spans(&self.title, theme)),
                Field::Vault => ("Vault", (self.choice(field, vault, theme), 0)),
                Field::KeyType => ("Type", (self.choice(field, key_type, theme), 0)),
                Field::Comment => ("Comment", Self::input_spans(&self.comment, theme)),
                Field::Passphrase => ("Passphrase", Self::input_spans(&self.passphrase, theme)),
            };
            let (spans, column) = value;

            if field == self.focus && !field.is_choice() {
                let column = GUTTER_WIDTH + LABEL_WIDTH + VALUE_INDENT + usize::from(column);
                cursor = Some(Position::new(column as u16, lines.len() as u16));
            }
            lines.extend(self.row(field, label, spans, theme));
        }

        lines.push(Line::default());
        lines.push(Line::styled(
            "The comment is stored inside the public key itself.",
            theme.faint(),
        ));
        lines.push(Line::styled(
            "A passphrase goes via the environment, never the command line.",
            theme.faint(),
        ));

        if let Some(error) = self.error {
            lines.push(Line::default());
            lines.push(Line::styled(
                format!("{GLYPH_WARN}  {error}"),
                theme.error(),
            ));
        }

        (lines, cursor)
    }

    fn input_spans(input: &TextInput, theme: &Theme) -> (Vec<Span<'static>>, u16) {
        let (mut spans, cursor) =
            input.render(theme.text(), theme.faint(), FIELD_WIDTH - VALUE_INDENT);
        spans.insert(0, Span::raw(" ".repeat(VALUE_INDENT)));
        (spans, cursor)
    }

    /// Renders a cycling choice. The arrows appear only on the focused field,
    /// since they are an affordance for the keys that are live right now.
    fn choice(&self, field: Field, value: &str, theme: &Theme) -> Vec<Span<'static>> {
        if self.focus == field {
            vec![
                Span::styled("‹ ", theme.accent()),
                Span::styled(value.to_owned(), theme.text()),
                Span::styled(" ›", theme.accent()),
            ]
        } else {
            vec![
                Span::raw(" ".repeat(VALUE_INDENT)),
                Span::styled(value.to_owned(), theme.muted()),
            ]
        }
    }

    /// Renders one labelled field over an underline. The focused row is marked
    /// with an accent bar in the gutter and an accent underline, so focus is
    /// visible from the shape of the row rather than from the cursor alone.
    fn row(
        &self,
        field: Field,
        label: &'static str,
        value: Vec<Span<'static>>,
        theme: &Theme,
    ) -> [Line<'static>; 2] {
        let focused = self.focus == field;

        let (gutter, label_style, underline) = if focused {
            (
                Span::styled(format!("{GLYPH_SELECTED} "), theme.accent()),
                theme.wordmark(),
                theme.accent_dim(),
            )
        } else {
            (
                Span::raw(" ".repeat(GUTTER_WIDTH)),
                theme.label(),
                theme.rule(),
            )
        };

        let mut head = vec![
            gutter,
            Span::styled(format!("{label:<LABEL_WIDTH$}"), label_style),
        ];
        head.extend(value);

        [
            Line::from(head),
            Line::from(vec![
                Span::raw(" ".repeat(GUTTER_WIDTH + LABEL_WIDTH)),
                Span::styled(GLYPH_RULE.repeat(FIELD_WIDTH), underline),
            ]),
        ]
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

    #[test]
    fn the_cursor_sits_in_the_focused_text_field() {
        let mut form = CreateForm::new(vaults(), None);
        type_text(&mut form, "ab");

        let (_, cursor) = form.render(&Theme::default());
        // Gutter, label and indent, then two characters typed.
        assert_eq!(cursor, Some(Position::new(18, 0)));

        press(&mut form, KeyCode::Tab);
        let (_, cursor) = form.render(&Theme::default());
        assert_eq!(cursor, None, "a choice has no text cursor");
    }
}
