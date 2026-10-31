//! A single-line text input.

use std::mem;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;
use ratatui::text::Span;
use zeroize::Zeroizing;

use crate::keys;
use crate::ui::text;

/// How typed text is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Echo {
    Normal,
    /// Every character is drawn as `•`, so a passphrase never reaches the
    /// terminal.
    Masked,
}

/// A single-line text input with a cursor.
///
/// The buffer is zeroed when the input is dropped or reset, and is allocated
/// at full size up front so that typing never reallocates — a reallocation
/// would leave a stale copy of the text behind in freed memory, where it could
/// not be wiped.
#[derive(Debug)]
pub struct TextInput {
    value: Zeroizing<String>,
    /// The cursor, in characters from the start.
    cursor: usize,
    /// The maximum length, in characters.
    limit: usize,
    echo: Echo,
    placeholder: &'static str,
}

impl TextInput {
    /// Creates an empty input holding at most `limit` characters.
    #[must_use]
    pub fn new(placeholder: &'static str, limit: usize, echo: Echo) -> Self {
        Self {
            value: Zeroizing::new(String::with_capacity(limit * char::MAX.len_utf8())),
            cursor: 0,
            limit,
            echo,
            placeholder,
        }
    }

    /// The text typed so far.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Replaces the text, as if it had been typed.
    #[cfg(test)]
    pub fn set_value(&mut self, text: &str) {
        self.reset();
        for c in text.chars() {
            self.insert(c);
        }
    }

    /// Wipes the text.
    pub fn reset(&mut self) {
        use zeroize::Zeroize;

        self.value.zeroize();
        self.cursor = 0;
    }

    /// Moves the text out, leaving the input empty, so that exactly one buffer
    /// ever holds it.
    pub fn take_value(&mut self) -> Zeroizing<String> {
        self.cursor = 0;
        mem::take(&mut self.value)
    }

    /// Inserts pasted text at the cursor, returning whether the text changed.
    ///
    /// Pasted text comes from outside protui like any other, so it is
    /// sanitised, and it is folded onto one line. What does not fit the limit
    /// is dropped.
    pub fn paste(&mut self, text: &str) -> bool {
        let clean = keys::sanitize(text.trim_end_matches(['\r', '\n']));
        let room = self.limit.saturating_sub(self.len());

        let mut changed = false;
        let fitting = clean.chars().take(room);
        for c in fitting {
            changed |= self.insert(c);
        }
        changed
    }

    /// Applies an editing key, returning whether the text changed.
    pub fn handle_key(&mut self, event: &KeyEvent) -> bool {
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);

        match event.code {
            KeyCode::Char('a') if control => self.cursor = 0,
            KeyCode::Char('e') if control => self.cursor = self.len(),
            KeyCode::Char('u') if control => return self.remove(0, self.cursor),
            KeyCode::Char('k') if control => return self.remove(self.cursor, self.len()),
            KeyCode::Char('w') if control => return self.remove(self.word_start(), self.cursor),
            KeyCode::Backspace if alt => return self.remove(self.word_start(), self.cursor),
            KeyCode::Char(c) if !control && !alt => return self.insert(c),
            KeyCode::Backspace => return self.remove(self.cursor.saturating_sub(1), self.cursor),
            KeyCode::Delete => return self.remove(self.cursor, self.cursor + 1),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => {}
        }

        false
    }

    /// Renders the input into at most `width` columns, scrolled so the cursor
    /// stays visible, and returns the cursor's column within them.
    #[must_use]
    pub fn render(
        &self,
        text_style: Style,
        placeholder_style: Style,
        width: usize,
    ) -> (Vec<Span<'static>>, u16) {
        use unicode_width::UnicodeWidthChar;

        if self.value.is_empty() {
            let placeholder = text::truncate(self.placeholder, width).into_owned();
            return (vec![Span::styled(placeholder, placeholder_style)], 0);
        }

        let shown: Vec<char> = match self.echo {
            Echo::Normal => self.value.chars().collect(),
            Echo::Masked => vec!['•'; self.len()],
        };
        let columns = |c: &char| c.width().unwrap_or(0);

        // Leave a column for the cursor past the last character.
        let room = width.saturating_sub(1).max(1);
        let mut start = self.cursor;
        let mut used = 0;
        while start > 0 && used + columns(&shown[start - 1]) <= room {
            start -= 1;
            used += columns(&shown[start]);
        }

        let mut visible = String::new();
        let mut filled = 0;
        for c in &shown[start..] {
            if filled + columns(c) > width {
                break;
            }
            visible.push(*c);
            filled += columns(c);
        }

        let cursor = shown[start..self.cursor].iter().map(columns).sum::<usize>();
        (vec![Span::styled(visible, text_style)], cursor as u16)
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    fn byte_offset(&self, chars: usize) -> usize {
        self.value
            .char_indices()
            .nth(chars)
            .map_or(self.value.len(), |(offset, _)| offset)
    }

    /// Inserts one character at the cursor, refusing what [`keys::sanitize`]
    /// would strip, so typed text is as safe to draw as text from pass-cli.
    fn insert(&mut self, c: char) -> bool {
        if !keys::is_displayable(c) || self.len() >= self.limit {
            return false;
        }

        let offset = self.byte_offset(self.cursor);
        self.value.insert(offset, c);
        self.cursor += 1;
        true
    }

    /// Removes the characters in `start..end`, clamped to the text.
    fn remove(&mut self, start: usize, end: usize) -> bool {
        let end = end.min(self.len());
        if start >= end {
            return false;
        }

        let range = self.byte_offset(start)..self.byte_offset(end);
        self.value.replace_range(range, "");
        self.cursor = start;
        true
    }

    /// Finds where the word before the cursor begins, past any spaces.
    fn word_start(&self) -> usize {
        let before: Vec<char> = self.value.chars().take(self.cursor).collect();
        let mut start = before.len();
        while start > 0 && before[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !before[start - 1].is_whitespace() {
            start -= 1;
        }
        start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(input: &mut TextInput, code: KeyCode, modifiers: KeyModifiers) -> bool {
        input.handle_key(&KeyEvent::new(code, modifiers))
    }

    fn typed(text: &str) -> TextInput {
        let mut input = TextInput::new("placeholder", 16, Echo::Normal);
        for c in text.chars() {
            press(&mut input, KeyCode::Char(c), KeyModifiers::NONE);
        }
        input
    }

    #[test]
    fn editing_moves_and_deletes_around_the_cursor() {
        let mut input = typed("clé ssh");

        press(&mut input, KeyCode::Left, KeyModifiers::NONE);
        press(&mut input, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(input.value(), "clé sh");

        press(&mut input, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(input.value(), "clé h");

        press(&mut input, KeyCode::Home, KeyModifiers::NONE);
        press(&mut input, KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(input.value(), "lé h");

        press(&mut input, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(input.value(), "");
    }

    #[test]
    fn the_limit_counts_characters() {
        let mut input = TextInput::new("", 3, Echo::Normal);
        for c in "鍵鍵鍵鍵".chars() {
            press(&mut input, KeyCode::Char(c), KeyModifiers::NONE);
        }

        assert_eq!(input.value(), "鍵鍵鍵");
    }

    #[test]
    fn typing_never_reallocates() {
        let mut input = TextInput::new("", 4, Echo::Masked);
        let capacity = input.value.capacity();

        for c in "𝄞𝄞𝄞𝄞".chars() {
            press(&mut input, KeyCode::Char(c), KeyModifiers::NONE);
        }

        assert_eq!(input.value.capacity(), capacity);
    }

    #[test]
    fn control_chords_and_control_characters_are_not_text() {
        let mut input = typed("");

        assert!(!press(
            &mut input,
            KeyCode::Char('x'),
            KeyModifiers::CONTROL
        ));
        assert!(!press(
            &mut input,
            KeyCode::Char('\u{1b}'),
            KeyModifiers::NONE
        ));
        assert_eq!(input.value(), "");
    }

    #[test]
    fn bidi_overrides_and_separators_are_not_text() {
        let mut input = typed("");

        for c in ['\u{202e}', '\u{2066}', '\u{2028}'] {
            assert!(!press(&mut input, KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(input.value(), "");
    }

    #[test]
    fn masked_input_never_renders_its_text() {
        let mut input = TextInput::new("optional", 16, Echo::Masked);
        input.set_value("hunter2");

        let (spans, cursor) = input.render(Style::new(), Style::new(), 34);

        assert_eq!(spans[0].content, "•••••••");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn render_scrolls_to_keep_the_cursor_visible() {
        let input = typed("abcdefghij");

        let (spans, cursor) = input.render(Style::new(), Style::new(), 5);

        assert_eq!(spans[0].content, "ghij");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn take_leaves_the_input_empty() {
        let mut input = typed("secret");

        assert_eq!(input.take_value().as_str(), "secret");
        assert_eq!(input.value(), "");
    }
}
