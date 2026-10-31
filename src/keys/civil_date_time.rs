//! Zoneless wall-clock timestamps.

use std::fmt;
use std::str::FromStr;

/// A date and time with no zone and no offset, as upstream serialises
/// `jiff::civil::DateTime`.
///
/// The value is wall-clock only, so it is never converted to an absolute
/// instant. It is a display and sort value, never used for arithmetic across
/// zones. See `docs/schema.md` §3.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDateTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

/// The error returned when a string is not a civil `YYYY-MM-DDTHH:MM:SS`
/// timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseCivilDateTimeError {
    input: String,
}

impl fmt::Display for ParseCivilDateTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a civil YYYY-MM-DDTHH:MM:SS timestamp",
            self.input
        )
    }
}

impl std::error::Error for ParseCivilDateTimeError {}

impl FromStr for CivilDateTime {
    type Err = ParseCivilDateTimeError;

    /// Parses `YYYY-MM-DDTHH:MM:SS`, optionally followed by a fractional
    /// second, which is discarded.
    ///
    /// Anything carrying a zone or offset is rejected: upstream timestamps are
    /// zoneless, and if they ever gain one that is a schema change worth
    /// hearing about loudly.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input).ok_or_else(|| ParseCivilDateTimeError {
            input: input.to_owned(),
        })
    }
}

impl CivilDateTime {
    fn parse(input: &str) -> Option<Self> {
        // ASCII-only up front, so every byte offset below is a char boundary.
        if !input.is_ascii() {
            return None;
        }

        let (main, fraction) = input.split_at_checked(19)?;
        match fraction.strip_prefix('.') {
            Some(digits) if (1..=9).contains(&digits.len()) && is_digits(digits) => {}
            None if fraction.is_empty() => {}
            _ => return None,
        }

        let bytes = main.as_bytes();
        let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
        if separators
            .iter()
            .any(|&(index, expected)| bytes[index] != expected)
        {
            return None;
        }

        let field = |start: usize, end: usize| -> Option<u8> {
            let digits = &main[start..end];
            if is_digits(digits) {
                digits.parse().ok()
            } else {
                None
            }
        };
        let year_digits = &main[0..4];
        let year: u16 = if is_digits(year_digits) {
            year_digits.parse().ok()?
        } else {
            return None;
        };

        let parsed = Self {
            year,
            month: field(5, 7)?,
            day: field(8, 10)?,
            hour: field(11, 13)?,
            minute: field(14, 16)?,
            second: field(17, 19)?,
        };

        parsed.is_valid().then_some(parsed)
    }

    fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
    }
}

/// Displays the value to the minute, which is the precision the detail pane
/// shows.
impl fmt::Display for CivilDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

fn is_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_upstream_layout() {
        let parsed: CivilDateTime = "2026-01-23T19:48:23".parse().expect("valid timestamp");

        assert_eq!(
            parsed,
            CivilDateTime {
                year: 2026,
                month: 1,
                day: 23,
                hour: 19,
                minute: 48,
                second: 23
            }
        );
        assert_eq!(parsed.to_string(), "2026-01-23 19:48");
    }

    #[test]
    fn accepts_a_fractional_second() {
        let parsed: CivilDateTime = "2024-02-29T00:00:00.123456".parse().expect("leap day");

        assert_eq!(parsed.to_string(), "2024-02-29 00:00");
    }

    #[test]
    fn rejects_anything_else() {
        for input in [
            "",
            // A zone means upstream changed the schema; it must not parse.
            "2026-01-23T19:48:23Z",
            "2026-01-23T19:48:23+01:00",
            "2026-01-23 19:48:23",
            "2026-01-23",
            "2026-13-01T00:00:00",
            "2026-02-29T00:00:00",
            "1900-02-29T00:00:00",
            "2026-04-31T00:00:00",
            "2026-01-23T24:00:00",
            "2026-01-23T19:60:00",
            "2026-01-23T19:48:60",
            "2026-01-23T19:48:23.",
            "+026-01-23T19:48:23",
            "2026-01-23T19:48:2٣",
            // Multi-byte text must be rejected, not sliced mid-character.
            "202é-01-23T19:48:23",
        ] {
            assert!(input.parse::<CivilDateTime>().is_err(), "{input:?} parsed");
        }
    }

    #[test]
    fn orders_chronologically() {
        let earlier: CivilDateTime = "2025-11-02T11:00:09".parse().expect("valid");
        let later: CivilDateTime = "2026-01-23T19:48:23".parse().expect("valid");

        assert!(earlier < later);
    }
}
