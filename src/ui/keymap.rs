//! protui's key bindings.
//!
//! Navigation is genuinely vim: j/k, gg/G, and Ctrl-f/b/d/u for paging, all
//! with the same semantics and the same modifiers vim uses.
//!
//! Actions follow the lazygit and k9s idiom instead: one mnemonic letter each.
//! They are not vim operators. In vim, d and y take a motion, which is why dd
//! and yy exist — the doubling disambiguates "delete a line" from "delete a
//! word". A list has one object under the cursor and no motions, so the second
//! keystroke would resolve no ambiguity.
//!
//! Deletion is deliberately split across two keys. `d` trashes, which upstream
//! can undo with `pass-cli item untrash`; `D` permanently destroys, which
//! nothing can undo. Both confirm, but only the permanent one requires typing
//! the title.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One physical key, as a binding names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character. Shift is implied by the character itself.
    Char(char),
    Ctrl(char),
    Up,
    Down,
    End,
    PageUp,
    PageDown,
}

impl Key {
    /// Reports whether `event` is this key.
    #[must_use]
    pub fn matches(self, event: &KeyEvent) -> bool {
        let chorded = event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        match self {
            Self::Char(c) => event.code == KeyCode::Char(c) && !chorded,
            Self::Ctrl(c) => {
                event.code == KeyCode::Char(c)
                    && event.modifiers.contains(KeyModifiers::CONTROL)
                    && !event.modifiers.contains(KeyModifiers::ALT)
            }
            Self::Up => event.code == KeyCode::Up && !chorded,
            Self::Down => event.code == KeyCode::Down && !chorded,
            Self::End => event.code == KeyCode::End && !chorded,
            Self::PageUp => event.code == KeyCode::PageUp && !chorded,
            Self::PageDown => event.code == KeyCode::PageDown && !chorded,
        }
    }
}

/// A set of keys that trigger one action, and how the help bar names it.
#[derive(Debug)]
pub struct Binding {
    pub keys: &'static [Key],
    pub help_key: &'static str,
    pub help: &'static str,
}

impl Binding {
    /// Reports whether `event` is any of this binding's keys.
    #[must_use]
    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.keys.iter().any(|key| key.matches(event))
    }
}

pub const UP: Binding = Binding {
    keys: &[Key::Char('k'), Key::Up],
    help_key: "k/↑",
    help: "up",
};
pub const DOWN: Binding = Binding {
    keys: &[Key::Char('j'), Key::Down],
    help_key: "j/↓",
    help: "down",
};
/// Two-key sequence, as in vim. A bare g is a prefix there and never acts on
/// its own.
pub const TOP: Binding = Binding {
    keys: &[Key::Char('g')],
    help_key: "gg",
    help: "top",
};
pub const BOTTOM: Binding = Binding {
    keys: &[Key::Char('G'), Key::End],
    help_key: "G",
    help: "bottom",
};
pub const PAGE_DOWN: Binding = Binding {
    keys: &[Key::Ctrl('f'), Key::PageDown],
    help_key: "^f",
    help: "page down",
};
pub const PAGE_UP: Binding = Binding {
    keys: &[Key::Ctrl('b'), Key::PageUp],
    help_key: "^b",
    help: "page up",
};
pub const HALF_DOWN: Binding = Binding {
    keys: &[Key::Ctrl('d')],
    help_key: "^d",
    help: "half page down",
};
pub const HALF_UP: Binding = Binding {
    keys: &[Key::Ctrl('u')],
    help_key: "^u",
    help: "half page up",
};
/// Scrolls the detail pane, not the list. vim's Ctrl-e and Ctrl-y scroll the
/// window without moving the cursor, which is exactly the distinction wanted.
pub const SCROLL_DOWN: Binding = Binding {
    keys: &[Key::Ctrl('e')],
    help_key: "^e",
    help: "scroll detail",
};
pub const SCROLL_UP: Binding = Binding {
    keys: &[Key::Ctrl('y')],
    help_key: "^y",
    help: "scroll detail up",
};
pub const FILTER: Binding = Binding {
    keys: &[Key::Char('/')],
    help_key: "/",
    help: "filter",
};
/// c is the advertised key; y stays bound as an alias, since the navigation
/// around it is vim's and yank is the reflex that comes with that.
pub const COPY: Binding = Binding {
    keys: &[Key::Char('c'), Key::Char('y')],
    help_key: "c",
    help: "copy pubkey",
};
/// n, as in lazygit. Binding n to list movement instead would only duplicate
/// j, because `/` here filters rather than searches: there is no hidden next
/// match to jump to.
pub const NEW: Binding = Binding {
    keys: &[Key::Char('n')],
    help_key: "n",
    help: "new",
};
pub const TRASH: Binding = Binding {
    keys: &[Key::Char('d')],
    help_key: "d",
    help: "trash",
};
pub const DELETE: Binding = Binding {
    keys: &[Key::Char('D')],
    help_key: "D",
    help: "delete!",
};
pub const AGENT: Binding = Binding {
    keys: &[Key::Char('a')],
    help_key: "a",
    help: "agent",
};
pub const REFRESH: Binding = Binding {
    keys: &[Key::Char('r')],
    help_key: "r",
    help: "refresh",
};
pub const HELP: Binding = Binding {
    keys: &[Key::Char('?')],
    help_key: "?",
    help: "help",
};
pub const QUIT: Binding = Binding {
    keys: &[Key::Char('q'), Key::Ctrl('c')],
    help_key: "q",
    help: "quit",
};

/// The single-line help bar shown under the list.
pub const SHORT_HELP: [&Binding; 9] = [
    &UP, &DOWN, &FILTER, &COPY, &NEW, &TRASH, &AGENT, &HELP, &QUIT,
];

/// The expanded help, toggled with `?`, as columns.
pub const FULL_HELP: [&[&Binding]; 6] = [
    &[&UP, &DOWN, &TOP, &BOTTOM],
    &[&PAGE_DOWN, &PAGE_UP],
    &[&HALF_DOWN, &HALF_UP, &SCROLL_DOWN, &SCROLL_UP],
    &[&FILTER, &REFRESH],
    &[&COPY, &NEW, &TRASH, &DELETE],
    &[&AGENT, &HELP, &QUIT],
];

/// Reports whether `event` is Ctrl-c, which quits from anywhere.
#[must_use]
pub fn is_interrupt(event: &KeyEvent) -> bool {
    Key::Ctrl('c').matches(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_keys_ignore_shift_but_not_control() {
        let shifted = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        let controlled = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);

        assert!(BOTTOM.matches(&shifted));
        assert!(!TOP.matches(&controlled));
    }

    #[test]
    fn control_keys_need_control() {
        let bare = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE);
        let held = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);

        assert!(!PAGE_DOWN.matches(&bare));
        assert!(PAGE_DOWN.matches(&held));
    }

    #[test]
    fn full_help_lists_every_binding_once() {
        let mut names: Vec<&str> = FULL_HELP
            .iter()
            .flat_map(|group| group.iter().map(|binding| binding.help))
            .collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), total);
        assert_eq!(total, 19);
    }
}
