//! Neutralising untrusted text before it reaches the terminal.
//!
//! Text that protui displays does not all come from protui. Item titles, vault
//! names and key comments are stored in Proton Pass, and Proton Pass supports
//! sharing items and vaults — so a title can have been written by somebody else
//! and is untrusted input.
//!
//! Drawn unfiltered into a terminal, such a string is not merely data. A title
//! containing ESC [ 2 J clears the screen; ESC ] 0 ; … BEL rewrites the window
//! title; and OSC 52, which xterm, kitty, `iTerm2`, `WezTerm` and foot all
//! honour, writes to the system clipboard. That last one matters here more than
//! elsewhere: protui's headline action is putting a public key on the
//! clipboard, and a malicious title could replace what the user believes they
//! copied.
//!
//! Everything from outside is therefore sanitised at the boundary rather than
//! at each render site, so nothing downstream has to remember.

use std::iter::Peekable;
use std::str::Chars;

const ESC: char = '\u{1b}';
const BEL: char = '\u{07}';

/// Returns `text` with terminal control sequences and display-reordering
/// characters removed, leaving text that is safe to draw.
///
/// It is deliberately lossy. Titles, vault names and comments are single-line
/// display values, so tabs and newlines collapse to spaces rather than being
/// preserved; anything relying on their layout is already being rendered
/// wrong.
#[must_use]
pub fn sanitize(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ESC => skip_escape_sequence(&mut chars),

            // Keep the word gap that whitespace represented, drop the control.
            // Line and paragraph separators are not control characters by
            // category, but they still break a single-line field.
            '\t' | '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{2028}' | '\u{2029}' => clean.push(' '),

            c if c.is_control() || is_explicit_bidi_control(c) => {}

            c => clean.push(c),
        }
    }

    clean
}

/// Consumes the remainder of an escape sequence whose ESC has already been
/// read, so that stripping it leaves clean text rather than a visible tail like
/// `[2J`. Dropping ESC alone would be enough for safety, since every sequence
/// needs it, but not enough to keep the display tidy.
fn skip_escape_sequence(chars: &mut Peekable<Chars<'_>>) {
    match chars.peek() {
        // OSC: ESC ] … terminated by BEL or ST. This is the clipboard one.
        Some(']') => {
            chars.next();
            while let Some(&c) = chars.peek() {
                match c {
                    BEL => {
                        chars.next();
                        return;
                    }
                    ESC => {
                        // ST is ESC \. Any other ESC begins a sequence of its
                        // own and is left for the caller to strip.
                        let mut lookahead = chars.clone();
                        lookahead.next();
                        if lookahead.peek() == Some(&'\\') {
                            chars.next();
                            chars.next();
                        }
                        return;
                    }
                    _ => {
                        chars.next();
                    }
                }
            }
        }

        // CSI: ESC [ parameters intermediates final.
        Some('[') => {
            chars.next();
            while chars.next_if(|c| matches!(c, '0'..='?')).is_some() {}
            while chars.next_if(|c| matches!(c, ' '..='/')).is_some() {}
            chars.next_if(|c| matches!(c, '@'..='~'));
        }

        // Two-character escapes. A trailing lone ESC needs nothing further.
        Some('@'..='Z' | '\\'..='_') => {
            chars.next();
        }

        _ => {}
    }
}

/// Reports the explicit bidirectional formatting characters.
///
/// They can reorder rendered text so that a name displays as something other
/// than what it is — the Trojan Source class of attack. Only the explicit
/// overrides are dropped; implicit bidi, which real Arabic and Hebrew text
/// relies on, still works.
fn is_explicit_bidi_control(c: char) -> bool {
    matches!(
        c,
        // LRE, RLE, PDF, LRO, RLO.
        '\u{202a}'..='\u{202e}'
            // LRI, RLI, FSI, PDI.
            | '\u{2066}'..='\u{2069}'
            // LRM, RLM.
            | '\u{200e}'
            | '\u{200f}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_table() {
        let cases = [
            (
                "ordinary text is untouched",
                "laptop key (work)",
                "laptop key (work)",
            ),
            (
                "non-ASCII text is untouched",
                "clé — ключ — 鍵 — مفتاح",
                "clé — ключ — 鍵 — مفتاح",
            ),
            ("empty stays empty", "", ""),
            (
                "CSI clear-screen is removed whole",
                "before\u{1b}[2Jafter",
                "beforeafter",
            ),
            (
                "SGR colour codes are removed whole",
                "\u{1b}[31mred\u{1b}[0m",
                "red",
            ),
            // The attack that matters most here: OSC 52 writes the system
            // clipboard on xterm, kitty, iTerm2, WezTerm and foot.
            (
                "OSC 52 clipboard write is removed",
                "key\u{1b}]52;c;aGVsbG8=\u{7}name",
                "keyname",
            ),
            (
                "OSC window-title set is removed",
                "a\u{1b}]0;pwned\u{7}b",
                "ab",
            ),
            (
                "OSC terminated by ST is removed",
                "a\u{1b}]0;x\u{1b}\\b",
                "ab",
            ),
            ("a bare escape is removed", "a\u{1b}b", "ab"),
            ("BEL and NUL are removed", "a\u{7}b\u{0}c", "abc"),
            ("DEL is removed", "a\u{7f}b", "ab"),
            (
                "newlines and tabs become spaces",
                "one\ttwo\nthree\r\nfour",
                "one two three  four",
            ),
            // Trojan Source: an override can make a name render as something
            // other than what it is.
            (
                "bidi overrides are removed",
                "safe\u{202e}reversed\u{202c}",
                "safereversed",
            ),
            ("bidi isolates are removed", "a\u{2066}b\u{2069}c", "abc"),
            (
                "line and paragraph separators become spaces",
                "a\u{2028}b\u{2029}c",
                "a b c",
            ),
            // No explicit override characters, so nothing is dropped: real
            // Arabic and Hebrew rely on implicit bidi.
            (
                "legitimate right-to-left text still works",
                "مفتاح ssh",
                "مفتاح ssh",
            ),
        ];

        for (name, input, want) in cases {
            assert_eq!(sanitize(input), want, "{name}: sanitize({input:?})");
        }
    }

    /// The property that matters, checked independently of the table above so
    /// a sequence nobody thought to enumerate still cannot get through.
    #[test]
    fn sanitize_leaves_no_control_characters() {
        let mut input: String = (0..0x100_u32).filter_map(char::from_u32).collect();
        input.push_str("\u{202e}\u{2066}\u{200e}\u{2028}");

        for c in sanitize(&input).chars() {
            assert!(
                !c.is_control(),
                "control character {c:?} survived sanitising"
            );
            assert!(
                !is_explicit_bidi_control(c),
                "bidi control {c:?} survived sanitising"
            );
        }
    }

    /// Guards against a rewrite that only strips one layer, which would let a
    /// doubled sequence through.
    #[test]
    fn sanitize_is_idempotent() {
        for input in [
            "\u{1b}[2J",
            "\u{1b}\u{1b}[2J[2J",
            "a\u{1b}]52;c;x\u{7}b",
            "plain",
        ] {
            let once = sanitize(input);
            assert_eq!(
                sanitize(&once),
                once,
                "sanitising {input:?} twice changed the result"
            );
        }
    }

    /// An OSC that is cut short by another sequence must not swallow it, and
    /// must not let it through either.
    #[test]
    fn sanitize_handles_an_interrupted_osc() {
        assert_eq!(sanitize("a\u{1b}]0;x\u{1b}[2Jb"), "ab");
        assert_eq!(sanitize("unterminated\u{1b}]52;c;aGVsbG8="), "unterminated");
    }
}
