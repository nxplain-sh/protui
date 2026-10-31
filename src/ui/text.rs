//! Measuring and fitting text to terminal columns.

use std::borrow::Cow;
use std::mem;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Returns how many terminal columns `text` occupies.
#[must_use]
pub fn width(text: &str) -> usize {
    text.width()
}

/// Shortens `text` to fit `width` columns, marking the cut with an ellipsis.
#[must_use]
pub fn truncate(text: &str, width: usize) -> Cow<'_, str> {
    if text.width() <= width {
        return Cow::Borrowed(text);
    }
    if width == 0 {
        return Cow::Borrowed("");
    }

    let mut cut = String::with_capacity(text.len());
    let mut used = 0;
    for c in text.chars() {
        let columns = c.width().unwrap_or(0);
        if used + columns > width - 1 {
            break;
        }
        cut.push(c);
        used += columns;
    }
    cut.push('…');

    Cow::Owned(cut)
}

/// Hard-wraps `text` into lines of at most `width` columns, collapsing runs of
/// whitespace first.
///
/// It breaks mid-token on purpose. A key is one long base64 run with no useful
/// break points, so wrapping on whitespace would strand the short `ssh-rsa`
/// prefix on a line of its own and leave the body ragged.
///
/// It measures characters rather than bytes: the body is ASCII, but the
/// trailing comment is arbitrary user text, and slicing that by byte offset
/// would cut a multi-byte character in half.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let collapsed = text.split_whitespace().collect::<Vec<&str>>().join(" ");
    if width == 0 {
        return vec![collapsed];
    }

    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for c in collapsed.chars() {
        let columns = c.width().unwrap_or(0);
        if used + columns > width && !line.is_empty() {
            lines.push(mem::take(&mut line));
            used = 0;
        }
        line.push(c);
        used += columns;
    }
    lines.push(line);

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_fits_the_width() {
        assert_eq!(truncate("laptop", 10), "laptop");
        assert_eq!(truncate("laptop", 6), "laptop");
        assert_eq!(truncate("laptop", 5), "lapt…");
        assert_eq!(truncate("laptop", 1), "…");
        assert_eq!(truncate("laptop", 0), "");
    }

    #[test]
    fn truncate_counts_columns_not_characters() {
        // Each of these takes two columns.
        let cut = truncate("鍵鍵鍵鍵", 5);

        assert_eq!(cut, "鍵鍵…");
        assert!(width(&cut) <= 5);
    }

    #[test]
    fn wrap_breaks_mid_token_at_the_width() {
        assert_eq!(
            wrap("ssh-ed25519 AAAAC3Nz", 8),
            ["ssh-ed25", "519 AAAA", "C3Nz"]
        );
    }

    #[test]
    fn wrap_collapses_whitespace_and_never_splits_a_character() {
        assert_eq!(wrap("  a\t\tb \n c ", 80), ["a b c"]);
        assert_eq!(wrap("clé鍵", 3), ["clé", "鍵"]);
    }

    #[test]
    fn wrap_always_returns_a_line() {
        assert_eq!(wrap("", 10), [""]);
    }
}
