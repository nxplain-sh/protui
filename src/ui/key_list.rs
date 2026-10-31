//! The key list: every loaded key, the filtered view of them, and the cursor.

use std::cmp::Reverse;
use std::mem;

use crossterm::event::{KeyCode, KeyEvent};

use crate::keys::{ItemRef, Key};
use crate::ui::text_input::{Echo, TextInput};

/// Where the fuzzy filter is in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterState {
    Off,
    /// The filter is being typed, and owns every key press.
    Typing,
    /// The filter is set and the list is navigable again.
    Applied,
}

/// The keys, sorted by vault then title, and the subset the filter shows.
#[derive(Debug)]
pub struct KeyList {
    keys: Vec<Key>,
    /// Indices into `keys`, in display order.
    visible: Vec<usize>,
    /// The highlighted position within `visible`.
    cursor: usize,
    /// The first position within `visible` that is on screen.
    offset: usize,
    filter: TextInput,
    filter_state: FilterState,
}

impl Default for KeyList {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            visible: Vec::new(),
            cursor: 0,
            offset: 0,
            filter: TextInput::new("", 128, Echo::Normal),
            filter_state: FilterState::Off,
        }
    }
}

impl KeyList {
    /// How many keys are loaded, whether or not the filter shows them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Reports whether no keys are loaded at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// How many keys the filter shows.
    #[must_use]
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// The highlighted position within the filtered view.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The keys from the first one on screen onwards, in display order.
    pub fn on_screen(&self) -> impl Iterator<Item = (usize, &Key)> {
        self.visible
            .iter()
            .enumerate()
            .skip(self.offset)
            .map(|(position, &index)| (position, &self.keys[index]))
    }

    /// The highlighted key, if the filtered view shows any.
    #[must_use]
    pub fn selected(&self) -> Option<&Key> {
        self.visible
            .get(self.cursor)
            .map(|&index| &self.keys[index])
    }

    /// Adds keys, keeping the cursor on the key it was on.
    pub fn extend(&mut self, keys: Vec<Key>) {
        let selected = self.selected_identity();
        self.keys.extend(keys);
        self.keys
            .sort_by_cached_key(|key| (key.vault_name.clone(), key.title.to_lowercase()));
        self.refilter();
        self.reselect(selected);
    }

    /// Drops every key, leaving any filter in place for the next load.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.visible.clear();
        self.cursor = 0;
        self.offset = 0;
    }

    /// Applies `change` to one key in place, leaving the cursor where it was.
    pub fn update(&mut self, item: &ItemRef, change: impl FnOnce(&mut Key)) {
        let Some(key) = self.keys.iter_mut().find(|key| key.is(item)) else {
            return;
        };
        change(key);

        // The filter matches derived metadata too, so a change can move a key
        // into or out of view.
        if self.filter_state != FilterState::Off {
            let selected = self.selected_identity();
            self.refilter();
            self.reselect(selected);
        }
    }

    /// Moves the cursor to `position`, clamped to the list.
    pub fn select(&mut self, position: usize) {
        self.cursor = position.min(self.visible.len().saturating_sub(1));
    }

    /// Moves the cursor down by `rows`, stopping at the last key.
    pub fn move_down(&mut self, rows: usize) {
        self.select(self.cursor.saturating_add(rows));
    }

    /// Moves the cursor up by `rows`, stopping at the first key.
    pub fn move_up(&mut self, rows: usize) {
        self.select(self.cursor.saturating_sub(rows));
    }

    /// Scrolls so that the cursor is within the `rows` on screen.
    pub fn scroll_into_view(&mut self, rows: usize) {
        let rows = rows.max(1);
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + rows {
            self.offset = self.cursor + 1 - rows;
        }
        // Never leave blank rows below the last key when there is more above.
        self.offset = self.offset.min(self.visible.len().saturating_sub(rows));
    }

    /// Where the filter is in its lifecycle.
    #[must_use]
    pub fn filter_state(&self) -> FilterState {
        self.filter_state
    }

    /// The filter input, for drawing its prompt.
    #[must_use]
    pub fn filter(&self) -> &TextInput {
        &self.filter
    }

    /// Hands the keyboard to the filter, keeping any text already in it.
    pub fn start_filtering(&mut self) {
        self.filter_state = FilterState::Typing;
    }

    /// Removes the filter, keeping the cursor on the key it was on.
    pub fn clear_filter(&mut self) {
        let selected = self.selected_identity();
        self.filter.reset();
        self.filter_state = FilterState::Off;
        self.refilter();
        self.reselect(selected);
    }

    /// Handles a key while the filter is being typed.
    pub fn handle_filter_key(&mut self, event: &KeyEvent) {
        match event.code {
            KeyCode::Esc => self.clear_filter(),
            KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                self.filter_state = if self.filter.value().is_empty() {
                    FilterState::Off
                } else {
                    FilterState::Applied
                };
            }
            _ => {
                if self.filter.handle_key(event) {
                    self.refilter();
                    // The best match is the likeliest target.
                    self.cursor = 0;
                    self.offset = 0;
                }
            }
        }
    }

    /// Inserts pasted text into the filter being typed.
    pub fn paste_into_filter(&mut self, text: &str) {
        if self.filter.paste(text) {
            self.refilter();
            self.cursor = 0;
            self.offset = 0;
        }
    }

    fn selected_identity(&self) -> Option<ItemRef> {
        self.selected().map(Key::item_ref)
    }

    fn reselect(&mut self, identity: Option<ItemRef>) {
        let position = identity.and_then(|item| {
            self.visible
                .iter()
                .position(|&index| self.keys[index].is(&item))
        });
        self.select(position.unwrap_or(self.cursor));
    }

    /// Recomputes which keys the filter shows, best match first.
    fn refilter(&mut self) {
        let pattern = self.filter.value();
        if pattern.is_empty() {
            self.visible = (0..self.keys.len()).collect();
            return;
        }

        let mut scored: Vec<(usize, usize)> = self
            .keys
            .iter()
            .enumerate()
            .filter_map(|(index, key)| fuzzy_score(pattern, &filter_text(key)).map(|s| (s, index)))
            .collect();
        // Stable, so equal scores keep the vault-then-title order.
        scored.sort_by_key(|&(score, _)| Reverse(score));
        self.visible = scored.into_iter().map(|(_, index)| index).collect();
    }
}

/// What the fuzzy filter matches against. Comment and fingerprint are included
/// so a key can be found by any of the things a user might remember about it.
fn filter_text(key: &Key) -> String {
    format!(
        "{} {} {} {} {}",
        key.title,
        key.vault_name,
        key.comment,
        key.label(),
        key.short_fingerprint()
    )
}

/// Scores `text` against `pattern` as a case-insensitive subsequence, or
/// returns `None` if it does not match.
///
/// Consecutive matches and matches at the start of a word score higher, as
/// does matching early, so `lap` ranks "laptop" above "old backup".
fn fuzzy_score(pattern: &str, text: &str) -> Option<usize> {
    const MATCH: usize = 1;
    const ADJACENT: usize = 5;
    const WORD_START: usize = 3;
    const EARLY: usize = 10;

    let mut text_chars = text.chars().flat_map(char::to_lowercase).enumerate();
    let mut score = 0;
    let mut last_match: Option<usize> = None;
    // The start of the text counts as the start of a word.
    let mut before = ' ';

    let wanted_chars = pattern.chars().flat_map(char::to_lowercase);
    for wanted in wanted_chars {
        loop {
            let (position, c) = text_chars.next()?;
            let previous = mem::replace(&mut before, c);
            if c != wanted {
                continue;
            }

            score += MATCH;
            match last_match {
                Some(last) if last + 1 == position => score += ADJACENT,
                Some(_) => {}
                None => score += EARLY.saturating_sub(position),
            }
            if !previous.is_alphanumeric() {
                score += WORD_START;
            }
            last_match = Some(position);
            break;
        }
    }

    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{ItemId, ShareId};

    fn key(vault: &str, title: &str) -> Key {
        Key {
            id: ItemId::from(title),
            share_id: ShareId::from(vault),
            vault_name: vault.to_owned(),
            title: title.to_owned(),
            ..Key::default()
        }
    }

    fn titles(list: &KeyList) -> Vec<&str> {
        list.on_screen()
            .map(|(_, key)| key.title.as_str())
            .collect()
    }

    fn type_filter(list: &mut KeyList, text: &str) {
        list.start_filtering();
        for c in text.chars() {
            list.handle_filter_key(&KeyEvent::from(KeyCode::Char(c)));
        }
    }

    #[test]
    fn keys_sort_by_vault_then_title_ignoring_case() {
        let mut list = KeyList::default();
        list.extend(vec![
            key("Work", "b"),
            key("Personal", "Zeta"),
            key("Personal", "alpha"),
        ]);

        assert_eq!(titles(&list), ["alpha", "Zeta", "b"]);
    }

    #[test]
    fn the_cursor_follows_its_key_when_more_keys_arrive() {
        let mut list = KeyList::default();
        list.extend(vec![key("Work", "laptop")]);

        list.extend(vec![key("Personal", "deploy")]);

        assert_eq!(
            list.selected().map(|key| key.title.as_str()),
            Some("laptop")
        );
    }

    #[test]
    fn filtering_keeps_matches_best_first() {
        let mut list = KeyList::default();
        list.extend(vec![
            key("V", "old backup"),
            key("V", "laptop"),
            key("V", "server"),
        ]);

        type_filter(&mut list, "lap");

        assert_eq!(titles(&list), ["laptop", "old backup"]);
    }

    #[test]
    fn escape_clears_the_filter_and_keeps_the_selection() {
        let mut list = KeyList::default();
        list.extend(vec![key("V", "alpha"), key("V", "beta"), key("V", "gamma")]);
        type_filter(&mut list, "gam");

        list.handle_filter_key(&KeyEvent::from(KeyCode::Esc));

        assert_eq!(list.filter_state(), FilterState::Off);
        assert_eq!(list.visible_len(), 3);
        assert_eq!(list.selected().map(|key| key.title.as_str()), Some("gamma"));
    }

    #[test]
    fn enter_applies_a_filter_and_clears_an_empty_one() {
        let mut list = KeyList::default();
        list.extend(vec![key("V", "alpha")]);

        type_filter(&mut list, "al");
        list.handle_filter_key(&KeyEvent::from(KeyCode::Enter));
        assert_eq!(list.filter_state(), FilterState::Applied);

        list.clear_filter();
        list.start_filtering();
        list.handle_filter_key(&KeyEvent::from(KeyCode::Enter));
        assert_eq!(list.filter_state(), FilterState::Off);
    }

    #[test]
    fn scrolling_keeps_the_cursor_on_screen() {
        let mut list = KeyList::default();
        list.extend((0..10).map(|i| key("V", &format!("key-{i:02}"))).collect());

        list.select(7);
        list.scroll_into_view(3);
        assert_eq!(
            list.on_screen().next().map(|(position, _)| position),
            Some(5)
        );

        list.select(1);
        list.scroll_into_view(3);
        assert_eq!(
            list.on_screen().next().map(|(position, _)| position),
            Some(1)
        );
    }

    #[test]
    fn fuzzy_score_matches_subsequences_case_insensitively() {
        assert!(fuzzy_score("LPT", "laptop").is_some());
        assert!(fuzzy_score("xyz", "laptop").is_none());
        assert!(fuzzy_score("", "laptop").is_some());
    }

    #[test]
    fn fuzzy_score_prefers_tight_matches_at_word_starts() {
        let tight = fuzzy_score("lap", "laptop").expect("matches");
        let loose = fuzzy_score("lap", "old backup").expect("matches");

        assert!(tight > loose, "{tight} should beat {loose}");
    }
}
