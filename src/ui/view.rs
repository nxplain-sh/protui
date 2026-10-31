//! Drawing the model.
//!
//! Only public key material is ever drawn: protui has no code path that reads
//! a private key, so none can reach the terminal.
//!
//! Every screen follows the grid in `docs/design/protui-design.md`. Text that
//! came from outside protui is drawn as it is held: `passcli` sanitises it on
//! the way in, and `TextInput` refuses anything sanitising would strip.

use std::mem;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthChar;

use crate::keys::{Algorithm, Key, State};
use crate::passcli::{AgentState, BINARY};
use crate::ui::create_form::CreateForm;
use crate::ui::draw;
use crate::ui::key_list::FilterState;
use crate::ui::keymap::{self, Hint};
use crate::ui::model::{
    DETAIL_BAND_ROWS, DETAIL_TOP, FOOTER_ROWS, HEADER_ROWS, LIST_TOP, Mode, Model, ROW_HEIGHT,
    STATUS_ROWS, Tone,
};
use crate::ui::text;
use crate::ui::theme::{
    GLYPH_DEGRADED, GLYPH_ERROR, GLYPH_LOWER_HALF, GLYPH_MARK, GLYPH_OK, GLYPH_POINTER,
    GLYPH_RUNNING, GLYPH_SEPARATOR, GLYPH_STOPPED, GLYPH_THIN_RULE, GLYPH_UPPER_HALF, GLYPH_WARN,
    HEAVY,
};

/// The detail pane's label column.
const LABEL_WIDTH: usize = 14;

/// The agent panel's label column, narrower because its labels are short.
const AGENT_LABEL_WIDTH: u16 = 11;

/// The space between two hints on the key bar.
const HINT_GAP: u16 = 3;

/// The box drawn where the list would be when there is no list: loading,
/// empty, or failed.
const STATE_BOX_WIDTH: u16 = 50;

/// A dialog's frame: everything above the rule, and how much room it needs
/// below.
struct Dialog {
    band: Color,
    icon: &'static str,
    title: &'static str,
    width: u16,
    /// The rows between the rule and the buttons.
    body_rows: u16,
    /// Hazard stripes across the band, and a red shadow on a dark terminal:
    /// the permanent delete.
    hazard: bool,
}

/// Where a dialog's contents go once its frame is drawn.
struct DialogLayout {
    /// Under the rule, two columns in from each border.
    body: Rect,
    /// The row the buttons sit on.
    buttons: u16,
}

impl Model {
    /// Draws the whole screen.
    pub fn render(&self, frame: &mut Frame<'_>) {
        if self.width == 0 {
            frame.render_widget(Paragraph::new("Loading…"), frame.area());
            return;
        }

        let area = Rect::new(0, 0, self.width, self.height).intersection(frame.area());
        let buf = frame.buffer_mut();
        let cursor = match &self.mode {
            Mode::List => self.render_list_screen(buf, area),
            Mode::Create(form) => self.render_form(buf, area, form),
            Mode::ConfirmTrash(key) => {
                self.render_trash_prompt(buf, area, key);
                None
            }
            Mode::ConfirmDelete(key) => Some(self.render_delete_prompt(buf, area, key)),
            Mode::Agent => {
                self.render_agent_panel(buf, area);
                None
            }
        };

        // A dialog has no footer, but what it reports — a title that did not
        // match, an agent that would not start — still has to be seen.
        if !matches!(self.mode, Mode::List) && self.status.is_some() && area.height > 0 {
            let bottom = Rect::new(area.x, area.bottom() - 1, area.width, 1);
            draw::clear(buf, bottom);
            self.render_status(buf, bottom);
        }

        if let Some(position) = cursor.filter(|&position| area.contains(position)) {
            frame.set_cursor_position(position);
        }
    }

    /// The transient status: a chip that says how it went, then the message.
    fn render_status(&self, buf: &mut Buffer, row: Rect) {
        let Some(status) = &self.status else {
            return;
        };
        let theme = &self.theme;
        // Errors are orange, not red: red is kept for the permanent delete.
        let (chip, message) = match status.tone {
            Tone::Muted => (None, theme.bold(theme.muted)),
            Tone::Ok => (
                Some((GLYPH_OK.to_owned(), theme.mint)),
                theme.bold(theme.ink),
            ),
            Tone::Warn => (
                Some((GLYPH_WARN.to_owned(), theme.orange)),
                theme.bold(theme.ink),
            ),
            Tone::Error => (
                Some((format!("{GLYPH_ERROR} ERROR"), theme.orange)),
                theme.bold(theme.ink),
            ),
        };

        let mut x = row.x + 2;
        if let Some((label, fill)) = chip {
            x += draw::chip(buf, x, row.y, &label, theme.chip(fill)) + 1;
        }
        let room = row.right().saturating_sub(x + 2);
        let text = text::truncate(&status.text, usize::from(room));
        draw::text(buf, x, row.y, &text, message, room);
    }

    fn render_list_screen(&self, buf: &mut Buffer, area: Rect) -> Option<Position> {
        let footer_top = area.bottom().saturating_sub(FOOTER_ROWS + self.help_rows());
        let status = Rect::new(
            area.x,
            footer_top.saturating_sub(STATUS_ROWS - 1),
            area.width,
            1,
        );

        self.render_header(buf, area);
        if let Some(error) = &self.error {
            self.render_error_body(buf, area, status.y, error);
        } else if self.list.is_empty() {
            // With nothing in the list there is no selected key for a detail
            // pane to describe, so the loading and empty states take the
            // full width rather than sitting in one half beside an empty pane.
            self.render_empty_body(buf, area, status.y);
        } else {
            let body = self.body_height() as u16;
            let label = Rect::new(area.x, area.y + LIST_TOP - 2, self.list_width(), 1);
            let list = Rect::new(area.x, area.y + LIST_TOP, self.list_width(), body);
            self.render_section_label(buf, label);
            self.render_list(buf, list);

            // The detail pane sits beside the list when there is room, and is
            // dropped entirely when there is not: below that width, splitting
            // leaves neither side readable.
            if self.has_detail_pane() {
                let slot = Rect::new(
                    area.x + self.list_width(),
                    area.y + DETAIL_TOP,
                    self.detail_pane_width(),
                    body + 2,
                );
                self.render_detail(buf, slot);
            }
        }

        // Drawn last, so on a terminal too short for everything the status
        // and the keys stay readable.
        if self.status.is_some() {
            draw::clear(buf, status);
            self.render_status(buf, status);
        }
        self.render_footer(buf, area)
    }

    /// The filled band: the wordmark on the left and the counts on the right,
    /// over a thick rule. What does not fit is dropped, least useful first.
    fn render_header(&self, buf: &mut Buffer, area: Rect) {
        let theme = &self.theme;
        let band = Rect::new(area.x, area.y, area.width, HEADER_ROWS - 1);
        draw::fill(buf, band, theme.fill(theme.yellow));
        let rule = GLYPH_UPPER_HALF.repeat(usize::from(area.width));
        draw::text(
            buf,
            area.x,
            band.bottom(),
            &rule,
            theme.text(theme.ink),
            area.width,
        );

        let y = area.y + 1;
        let wordmark = format!("{GLYPH_MARK} PROTUI");
        let left =
            area.x + 2 + draw::chip(buf, area.x + 2, y, &wordmark, theme.on_ink(theme.yellow));
        let mut right = area.right().saturating_sub(2);

        let count = self.header_count();
        let count_width = text::width(&count) as u16 + 2;
        if left + 2 + count_width <= right {
            right -= count_width;
            draw::chip(buf, right, y, &count, theme.chip(theme.violet));

            // Only from the status protui already has. Asking pass-cli for it
            // on every load would cost a call for a nicety, so it can be stale
            // until the agent panel is next opened.
            let agent = format!("{GLYPH_RUNNING} AGENT UP");
            let agent_width = text::width(&agent) as u16 + 2;
            if self.agent.state == AgentState::Running && left + 2 + agent_width + 2 <= right {
                right -= agent_width + 2;
                draw::chip(buf, right, y, &agent, theme.chip(theme.mint));
            }
        }

        let subtitle = "SSH KEYS · PROTON PASS";
        if left + 2 + text::width(subtitle) as u16 + 2 <= right {
            draw::text(
                buf,
                left + 2,
                y,
                subtitle,
                theme.chip(theme.yellow),
                u16::MAX,
            );
        }
    }

    /// Reports where the cursor is as well as how many keys there are. Only a
    /// fraction of a long list fits on screen and nothing else indicates
    /// position, so a bare total would leave a user unable to tell whether
    /// more exists below.
    fn header_count(&self) -> String {
        let total = self.list.len();
        if total == 0 {
            return if self.busy() {
                format!("{} LOADING", self.spinner_frame())
            } else {
                "NO KEYS".to_owned()
            };
        }

        let visible = self.list.visible_len();
        let position = (self.list.cursor() + 1).min(visible);

        // While a filter is set, both numbers matter: where you are in the
        // matches, and how much of the collection they represent.
        if !self.list.filter().value().is_empty() {
            return format!("{position}/{visible} OF {total} KEYS");
        }
        if total == 1 {
            return "1 KEY".to_owned();
        }

        format!("{position}/{total} KEYS")
    }

    /// Names what the list shows: every key, or what the filter matched.
    fn render_section_label(&self, buf: &mut Buffer, row: Rect) {
        let theme = &self.theme;
        let filter = self.list.filter().value();
        let (label, style, description) = if filter.is_empty() {
            (
                "KEYS",
                theme.inverted(),
                "all vaults · sorted by vault, then title".to_owned(),
            )
        } else {
            (
                "FILTER",
                theme.chip(theme.yellow),
                format!(
                    "matching “{filter}” in {} of {} keys",
                    self.list.visible_len(),
                    self.list.len()
                ),
            )
        };

        let x = row.x + 2;
        let after = x + draw::chip(buf, x, row.y, label, style) + 1;
        let room = row.right().saturating_sub(after + 2);
        let description = text::truncate(&description, usize::from(room));
        draw::text(
            buf,
            after,
            row.y,
            &description,
            theme.text(theme.faint),
            room,
        );
    }

    /// Draws each key as two lines, the title then where it lives, with its
    /// algorithm as a chip. The selected key is a filled block with a shadow.
    ///
    /// Two lines rather than aligned columns, because a key list is scanned by
    /// name and columns force every field to the width of its longest value —
    /// one long vault name would push fingerprints off the edge of every other
    /// row.
    fn render_list(&self, buf: &mut Buffer, area: Rect) {
        let theme = &self.theme;
        // Inset two columns, leaving room on the right for the selected
        // block's shadow and a margin.
        let block_x = area.x + 2;
        let block_width = area.width.saturating_sub(6);
        let text_x = block_x + 3;

        let rows: Vec<(usize, &Key)> = self.list.on_screen().take(self.visible_rows()).collect();
        for (row, &(position, key)) in rows.iter().enumerate() {
            let y = area.y + (row * ROW_HEIGHT) as u16;
            let block = Rect::new(block_x, y, block_width, 2);
            let selected = position == self.list.cursor();

            let (title_style, summary_style) = if selected {
                draw::fill(buf, block, theme.fill(theme.violet));
                draw::shadow(buf, block, theme.shadow);
                draw::text(
                    buf,
                    block_x + 1,
                    y,
                    GLYPH_POINTER,
                    theme.fill(theme.violet),
                    1,
                );
                (theme.chip(theme.violet), theme.fill(theme.violet))
            } else {
                (theme.bold(theme.ink), theme.text(theme.faint))
            };

            // The chip sits one column in from the block's edge, when that
            // still leaves the title some room.
            let mut title_right = block.right().saturating_sub(1);
            if let Some((label, style)) = self.algorithm_chip(key) {
                let width = text::width(&label) as u16 + 2;
                if block_width >= width + 12 {
                    let x = block.right() - 1 - width;
                    // On a light terminal the chip would vanish into the
                    // selection's fill, so it inverts there.
                    let style = if selected && !theme.is_dark() {
                        theme.inverted()
                    } else {
                        style
                    };
                    draw::chip(buf, x, y, &label, style);
                    title_right = x.saturating_sub(2);
                }
            }

            self.render_title(
                buf,
                text_x,
                y,
                key,
                title_style,
                title_right.saturating_sub(text_x),
            );
            let room = block.right().saturating_sub(text_x + 1);
            let summary = text::truncate(&Self::summary(key), usize::from(room)).into_owned();
            draw::text(buf, text_x, y + 1, &summary, summary_style, room);
        }

        // Below the list: how many more keys there are, or how many the
        // filter is hiding.
        let shown_to = rows.last().map_or(0, |(position, _)| position + 1);
        let below = self.list.visible_len().saturating_sub(shown_to);
        let hidden = self.list.len() - self.list.visible_len();
        let room = area.width.saturating_sub(4);
        if below > 0 {
            let more = format!("↓ {below} more");
            draw::text(
                buf,
                area.x + 2,
                area.bottom(),
                &more,
                theme.text(theme.faint),
                room,
            );
        } else if hidden > 0 {
            let noun = if hidden == 1 { "key" } else { "keys" };
            let note = format!("{hidden} {noun} hidden by the filter");
            let y = (area.y + (rows.len() * ROW_HEIGHT) as u16 + 2).min(area.bottom());
            draw::text(buf, area.x + 2, y, &note, theme.text(theme.faint), room);
        }
    }

    /// Draws a key's title, filling the characters the filter matched yellow.
    fn render_title(&self, buf: &mut Buffer, x: u16, y: u16, key: &Key, style: Style, width: u16) {
        let title = text::truncate(&key.title, usize::from(width));
        let matches = self.list.title_matches(key);
        if matches.is_empty() {
            draw::text(buf, x, y, &title, style, width);
            return;
        }

        // The ellipsis that marks a cut is not part of the title.
        let kept = if title.len() == key.title.len() {
            usize::MAX
        } else {
            title.chars().count() - 1
        };
        let highlight = self.theme.chip(self.theme.yellow);

        let mut spans = Vec::new();
        let mut run = String::new();
        let mut run_style = style;
        for (index, c) in title.chars().enumerate() {
            let mut wanted = if index < kept && matches.contains(&index) {
                highlight
            } else {
                style
            };
            // A zero-width character, such as a combining accent, belongs to
            // the one before it; drawn on its own it would be dropped.
            if c.width() == Some(0) && !run.is_empty() {
                wanted = run_style;
            }
            if wanted != run_style && !run.is_empty() {
                spans.push(Span::styled(mem::take(&mut run), run_style));
            }
            run_style = wanted;
            run.push(c);
        }
        spans.push(Span::styled(run, run_style));

        draw::line(buf, x, y, &Line::from(spans), width);
    }

    /// The second line of a row: the vault, and the start of the fingerprint.
    /// Empty parts are dropped so the separator never dangles.
    ///
    /// Until the public key arrives the fingerprint is unknown, and saying so
    /// beats a blank for a key that is merely still loading.
    fn summary(key: &Key) -> String {
        const SHOWN: usize = 16;

        let fingerprint = if key.public_key.is_empty() {
            "loading…".to_owned()
        } else {
            let short = key.short_fingerprint();
            let start: String = short.chars().take(SHOWN).collect();
            if short.chars().count() > SHOWN {
                format!("{start}…")
            } else {
                start
            }
        };

        [key.vault_name.as_str(), &fingerprint]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<&str>>()
            .join(&format!("  {GLYPH_SEPARATOR}  "))
    }

    /// The key's algorithm as a chip, so the list scans by colour without
    /// being read. There is none until its public key has been read.
    fn algorithm_chip(&self, key: &Key) -> Option<(String, Style)> {
        if key.public_key.is_empty() {
            return None;
        }

        let theme = &self.theme;
        let style = match key.algorithm {
            Algorithm::Ed25519 | Algorithm::SkEd25519 => theme.chip(theme.mint),
            Algorithm::Rsa if key.bits >= 4096 => theme.chip(theme.pink),
            Algorithm::Rsa | Algorithm::Dsa => theme.chip(theme.orange),
            Algorithm::Ecdsa | Algorithm::SkEcdsa | Algorithm::Unknown => theme.code_chip(),
        };
        Some((key.label().to_uppercase(), style))
    }

    /// The heavy box beside the list: a violet band naming the key, a rule,
    /// then its details, which scroll when they do not fit.
    fn render_detail(&self, buf: &mut Buffer, slot: Rect) {
        let theme = &self.theme;
        // A column of space before the box, and room after it for its shadow.
        let rect = Rect::new(
            slot.x + 1,
            slot.y,
            slot.width.saturating_sub(3),
            slot.height,
        );
        draw::clear(buf, rect);
        draw::outline_with_shadow(buf, rect, &HEAVY, theme.text(theme.ink), theme.shadow);

        let inner = rect.inner(Margin::new(1, 1));
        let x = inner.x + 2;
        let width = inner.width.saturating_sub(4);
        let mut body = inner;

        if let Some(key) = self.list.selected()
            && self.has_detail_band()
        {
            self.render_detail_band(buf, Rect::new(inner.x, inner.y, inner.width, 3), key);
            draw::tee(buf, rect, inner.y + 3, &HEAVY, theme.text(theme.ink));
            let band = DETAIL_BAND_ROWS as u16;
            body = Rect::new(inner.x, inner.y + band, inner.width, inner.height - band);
        }

        for (row, line) in self.detail_window().iter().enumerate() {
            draw::line(buf, x, body.y + row as u16, line, width);
        }
    }

    /// The title band: the key's title, and on the right what it is.
    fn render_detail_band(&self, buf: &mut Buffer, band: Rect, key: &Key) {
        let theme = &self.theme;
        let y = band.y + 1;
        draw::fill(buf, band, theme.fill(theme.violet));

        // The algorithm is a chip rather than another label/value row: it is
        // the one attribute worth recognising at a glance.
        let mut right = band.right().saturating_sub(2);
        let mut chips = Vec::new();
        match self.algorithm_chip(key) {
            Some(chip) => chips.push(chip),
            None => chips.push((
                format!("{} reading key", self.spinner_frame()),
                theme.fill(theme.violet),
            )),
        }
        if key.state == State::Trashed {
            chips.push(("TRASHED".to_owned(), theme.chip(theme.orange)));
        }
        for (label, style) in chips {
            let width = text::width(&label) as u16 + 2;
            if right < band.x + 2 + width + 8 {
                break;
            }
            right -= width;
            draw::chip(buf, right, y, &label, style);
            right = right.saturating_sub(1);
        }

        let x = band.x + 2;
        let room = right.saturating_sub(x + 1);
        let title = text::truncate(&key.title, usize::from(room));
        draw::text(buf, x, y, &title, theme.chip(theme.violet), room);
    }

    /// The slice of the detail pane's body currently on screen, with markers
    /// where there is more above or below.
    ///
    /// The pane is clamped rather than allowed to overflow: a
    /// taller-than-terminal block would be cut by the terminal itself,
    /// silently and from the bottom.
    #[must_use]
    pub(super) fn detail_window(&self) -> Vec<Line<'static>> {
        let lines = self.detail_lines();
        let height = self.detail_body_height();
        if lines.len() <= height {
            return lines;
        }

        let offset = self.detail_offset.min(self.max_detail_offset());
        let mut window: Vec<Line<'static>> = lines[offset..offset + height].to_vec();
        let faint = self.theme.text(self.theme.faint);

        // The markers replace a line rather than being added to one, so the
        // pane never grows past the height it was given.
        if offset > 0 {
            window[0] = Line::styled(format!("↑ {offset} more"), faint);
        }
        let below = lines.len() - offset - height;
        if below > 0 {
            window[height - 1] = Line::styled(format!("↓ {below} more · ^e/^y scrolls"), faint);
        }

        window
    }

    /// The detail pane's body for the highlighted key, one entry per line.
    ///
    /// It is a pure function of the model, so the update loop can measure it
    /// when clamping a scroll without the view having to record anything.
    #[must_use]
    pub(super) fn detail_lines(&self) -> Vec<Line<'static>> {
        let theme = &self.theme;
        let width = self.detail_inner_width();
        let ink = theme.text(theme.ink);
        let faint = theme.text(theme.faint);
        let Some(key) = self.list.selected() else {
            return vec![Line::default(), Line::styled("No key selected.", faint)];
        };
        let pending = self.public_key_pending(key);

        // Without the band, the title has nowhere else to go.
        let mut lines = if self.has_detail_band() {
            vec![Line::default()]
        } else {
            let title = text::truncate(&key.title, width).into_owned();
            vec![Line::styled(title, theme.bold(theme.ink))]
        };

        lines.push(self.detail_row("VAULT", &key.vault_name, theme.bold(theme.ink), width));

        // The fingerprint wraps rather than truncating. Comparing one against
        // a forge or `ssh-keygen -lf` is a main reason to open this pane, and a
        // fingerprint with its middle elided cannot be compared at all.
        let (fingerprint, fingerprint_style) = match &pending {
            Some(placeholder) => (placeholder.as_str(), faint),
            None => (key.fingerprint.as_str(), theme.chip(theme.yellow)),
        };
        lines.extend(self.detail_wrapped("FINGERPRINT", fingerprint, fingerprint_style, width));

        let (comment, comment_style) = match &pending {
            Some(placeholder) => (placeholder.as_str(), faint),
            None if key.comment.is_empty() => ("—", faint),
            None => (key.comment.as_str(), ink),
        };
        lines.extend(self.detail_wrapped("COMMENT", comment, comment_style, width));

        // Upstream timestamps carry no zone, so they are shown as the
        // wall-clock values they are rather than converted.
        if let Some(created) = key.created_at {
            lines.push(self.detail_row("CREATED", &created.to_string(), ink, width));
        }
        if let Some(modified) = key.modified_at {
            lines.push(self.detail_row("MODIFIED", &modified.to_string(), ink, width));
        }

        // Item IDs are long base64; truncating one makes it useless for
        // passing back to pass-cli, which is the only reason to show it.
        lines.extend(self.detail_wrapped("ITEM ID", key.id.as_str(), faint, width));

        lines.push(Line::default());
        let chip = " PUBLIC KEY ";
        lines.push(Line::from(vec![
            Span::styled(chip, theme.inverted()),
            Span::raw(" "),
            Span::styled(
                GLYPH_THIN_RULE.repeat(width.saturating_sub(text::width(chip) + 1)),
                ink,
            ),
        ]));

        if pending.is_some() {
            lines.push(Line::styled(
                format!("{} loading…", self.spinner_frame()),
                faint,
            ));
        } else {
            let key_lines = text::wrap(&key.public_key, width.saturating_sub(2).max(20));
            lines.extend(self.code_block(&key_lines, width));
            lines.push(Line::default());
            lines.extend(self.copy_button());
        }

        lines
    }

    /// Lines of text on a `code` fill, `width` columns wide, with a cell of
    /// padding all round.
    fn code_block(&self, rows: &[String], width: usize) -> Vec<Line<'static>> {
        let style = Style::new().fg(self.theme.ink).bg(self.theme.code);
        let blank = Line::styled(" ".repeat(width), style);

        let mut lines = vec![blank.clone()];
        lines.extend(rows.iter().map(|row| {
            let pad = width.saturating_sub(text::width(row) + 1);
            Line::styled(format!(" {row}{}", " ".repeat(pad)), style)
        }));
        lines.push(blank);
        lines
    }

    /// The copy button and its shadow, as lines so they scroll with the pane.
    fn copy_button(&self) -> [Line<'static>; 2] {
        let theme = &self.theme;
        let button = format!(" {}  COPY ", keymap::COPY.help_key);
        let shadow = GLYPH_UPPER_HALF.repeat(text::width(&button));

        [
            Line::from(vec![
                Span::styled(button, theme.chip(theme.yellow)),
                Span::styled(GLYPH_LOWER_HALF, theme.text(theme.shadow)),
                Span::styled(
                    "  public key only — no private material",
                    theme.text(theme.faint),
                ),
            ]),
            Line::from(vec![
                Span::raw(" "),
                Span::styled(shadow, theme.text(theme.shadow)),
            ]),
        ]
    }

    /// While a key's public key is still being fetched, the text shown in place
    /// of the values derived from it, so a blank field never reads as "this key
    /// has none".
    fn public_key_pending(&self, key: &Key) -> Option<String> {
        key.public_key
            .is_empty()
            .then(|| format!("{} …", self.spinner_frame()))
    }

    /// One label/value line, truncated to fit. Used for values short enough
    /// that eliding them loses nothing: a vault name, a timestamp.
    fn detail_row(&self, label: &str, value: &str, style: Style, width: usize) -> Line<'static> {
        let value = text::truncate(value, value_width(width)).into_owned();

        Line::from(vec![self.detail_label(label), Span::styled(value, style)])
    }

    /// A label/value row whose value continues on further lines, indented
    /// under the value column. Used where the whole value matters.
    fn detail_wrapped(
        &self,
        label: &str,
        value: &str,
        style: Style,
        width: usize,
    ) -> Vec<Line<'static>> {
        text::wrap(value, value_width(width))
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let lead = if index == 0 {
                    self.detail_label(label)
                } else {
                    Span::raw(" ".repeat(LABEL_WIDTH))
                };
                Line::from(vec![lead, Span::styled(line, style)])
            })
            .collect()
    }

    fn detail_label(&self, label: &str) -> Span<'static> {
        Span::styled(
            format!("{label:<LABEL_WIDTH$}"),
            self.theme.bold(self.theme.muted),
        )
    }

    /// The key bar: filled rows with the keys on the middle one, or the filter
    /// as it is typed. Returns where the terminal cursor belongs, if anywhere.
    fn render_footer(&self, buf: &mut Buffer, area: Rect) -> Option<Position> {
        let rows = FOOTER_ROWS + self.help_rows();
        let bar = Rect::new(
            area.x,
            area.bottom().saturating_sub(rows),
            area.width,
            rows.min(area.height),
        );
        draw::fill(buf, bar, self.theme.bar_label());

        if self.list.filter_state() == FilterState::Typing {
            return Some(self.render_filter_bar(buf, bar));
        }
        if self.full_help {
            self.render_full_help(buf, bar);
        } else {
            let hints: &[Hint] = if self.list.is_empty() {
                &keymap::EMPTY_HELP
            } else {
                &keymap::SHORT_HELP
            };
            self.render_hints(
                buf,
                bar.x + 2,
                bar.y + 1,
                bar.right().saturating_sub(2),
                hints,
            );
        }
        None
    }

    /// One row of hints, cut with an ellipsis where the bar runs out.
    fn render_hints(&self, buf: &mut Buffer, x: u16, y: u16, right: u16, hints: &[Hint]) {
        let label_style = self.theme.bar_label();
        let mut x = x;

        for (index, &(key, label)) in hints.iter().enumerate() {
            let gap = if index == 0 { 0 } else { HINT_GAP };
            let needed = gap + (text::width(key) + 3 + text::width(label)) as u16;
            if x + needed > right {
                if x + 2 <= right {
                    draw::text(buf, x + 1, y, "…", label_style, 1);
                }
                break;
            }

            x += gap;
            x += self.keycap(buf, x, y, key);
            x = draw::text(buf, x, y, &format!(" {label}"), label_style, u16::MAX);
        }
    }

    /// The expanded help: one column per group of related bindings.
    fn render_full_help(&self, buf: &mut Buffer, bar: Rect) {
        const GAP: u16 = 4;

        let right = bar.right().saturating_sub(2);
        let mut x = bar.x + 2;
        for (index, group) in keymap::FULL_HELP.iter().enumerate() {
            let key_width = group
                .iter()
                .map(|b| text::width(b.help_key))
                .max()
                .unwrap_or(0);
            let help_width = group.iter().map(|b| text::width(b.help)).max().unwrap_or(0);
            let gap = if index == 0 { 0 } else { GAP };
            let column = (key_width + 3 + help_width) as u16;
            if x + gap + column > right {
                break;
            }

            x += gap;
            for (row, binding) in group.iter().enumerate() {
                let y = bar.y + 1 + row as u16;
                let key = format!("{:<key_width$}", binding.help_key);
                let cap = self.keycap(buf, x, y, &key);
                let help = format!(" {}", binding.help);
                draw::text(buf, x + cap, y, &help, self.theme.bar_label(), u16::MAX);
            }
            x += column;
        }
    }

    /// A key as a yellow cap. `D` alone is red: it is the permanent delete.
    fn keycap(&self, buf: &mut Buffer, x: u16, y: u16, key: &str) -> u16 {
        let fill = if key == keymap::DELETE.help_key {
            self.theme.red
        } else {
            self.theme.yellow
        };
        draw::chip(buf, x, y, key, self.theme.chip(fill))
    }

    /// The filter as it is typed: a `/` cap, the text with a solid cursor, and
    /// on the right the keys that finish it. Returns the cursor's position.
    fn render_filter_bar(&self, buf: &mut Buffer, bar: Rect) -> Position {
        const HINTS: [Hint; 3] = [("enter", "apply"), ("esc", "clear"), ("↑/↓", "move")];
        const MIN_TYPING_ROOM: u16 = 20;

        let theme = &self.theme;
        let y = bar.y + 1;
        let right = bar.right().saturating_sub(2);
        let x = bar.x + 2;
        let text_x = x + self.keycap(buf, x, y, keymap::FILTER.help_key) + 1;

        let hints_width = HINTS
            .iter()
            .map(|(key, label)| (text::width(key) + 3 + text::width(label)) as u16)
            .sum::<u16>()
            + HINT_GAP * (HINTS.len() as u16 - 1);
        let mut text_right = right;
        if text_x + MIN_TYPING_ROOM + hints_width <= right {
            let mut hint_x = right - hints_width;
            text_right = hint_x.saturating_sub(2);
            for (index, (key, label)) in HINTS.iter().enumerate() {
                if index > 0 {
                    hint_x += HINT_GAP;
                }
                hint_x += draw::chip(buf, hint_x, y, key, theme.on_ink(theme.yellow));
                hint_x = draw::text(
                    buf,
                    hint_x,
                    y,
                    &format!(" {label}"),
                    theme.bar_label(),
                    u16::MAX,
                );
            }
        }

        let width = text_right.saturating_sub(text_x);
        let typed = theme.bar_label().add_modifier(Modifier::BOLD);
        let (spans, column) =
            self.list
                .filter()
                .render(typed, theme.bar_label(), usize::from(width));
        draw::line(buf, text_x, y, &Line::from(spans), width);

        let cursor = Position::new(text_x + column, y);
        draw::restyle(buf, cursor.x, cursor.y, theme.bar_cursor());
        cursor
    }

    /// A centred box of `height` rows where the list would be, between the
    /// header and the status line, filled with `fill` or with the ground.
    fn render_state_box(
        &self,
        buf: &mut Buffer,
        area: Rect,
        status_row: u16,
        height: u16,
        fill: Option<Color>,
    ) -> Rect {
        let theme = &self.theme;
        let top = area.y + HEADER_ROWS;
        let region = Rect::new(area.x, top, area.width, status_row.saturating_sub(top));
        let width = STATE_BOX_WIDTH.min(region.width.saturating_sub(6));
        let rect = Rect::new(
            region.x + (region.width - width) / 2,
            region.y + region.height.saturating_sub(height) / 2,
            width,
            height,
        );

        draw::clear(buf, rect);
        if let Some(fill) = fill {
            draw::fill(buf, rect.inner(Margin::new(1, 1)), theme.fill(fill));
        }
        draw::outline_with_shadow(buf, rect, &HEAVY, theme.text(theme.ink), theme.shadow);
        rect
    }

    /// Covers both states where the list has nothing in it: still loading, and
    /// genuinely empty.
    fn render_empty_body(&self, buf: &mut Buffer, area: Rect, status_row: u16) {
        let theme = &self.theme;

        if self.loading {
            let rect = self.render_state_box(buf, area, status_row, 7, None);
            let line = format!("{} Loading keys…", self.spinner_frame());
            let width = rect.width.saturating_sub(8);
            draw::text(
                buf,
                rect.x + 4,
                rect.y + 3,
                &line,
                theme.bold(theme.ink),
                width,
            );
            return;
        }

        let rect = self.render_state_box(buf, area, status_row, 11, Some(theme.yellow));
        let (x, width) = (rect.x + 4, rect.width.saturating_sub(8));
        let on_yellow = theme.fill(theme.yellow);
        draw::text(
            buf,
            x,
            rect.y + 2,
            "NO SSH KEYS YET",
            theme.chip(theme.yellow),
            width,
        );
        let explanation = "Nothing in any vault this session can see.";
        draw::text(buf, x, rect.y + 4, explanation, on_yellow, width);

        let y = rect.y + 7;
        let new = Some(theme.chip(theme.violet));
        let button = draw::button(buf, x, y, keymap::NEW.help_key, "CREATE ONE", new, theme);
        let after = draw::after_button(x, button);
        let reload = format!("{}  reload", keymap::REFRESH.help_key);
        draw::text(
            buf,
            after,
            y,
            &reload,
            on_yellow,
            rect.right().saturating_sub(after + 2),
        );
    }

    fn render_error_body(&self, buf: &mut Buffer, area: Rect, status_row: u16, error: &str) {
        let theme = &self.theme;
        let wrap_width = STATE_BOX_WIDTH
            .min(area.width.saturating_sub(6))
            .saturating_sub(8)
            .max(10);
        let lines = text::wrap(error, usize::from(wrap_width));
        let height = 10 + lines.len() as u16;

        let rect = self.render_state_box(buf, area, status_row, height, None);
        let (x, width) = (rect.x + 4, rect.width.saturating_sub(8));
        let chip = format!("{GLYPH_ERROR} COULDN'T LOAD KEYS");
        draw::chip(buf, x, rect.y + 2, &chip, theme.chip(theme.orange));
        for (row, line) in lines.iter().enumerate() {
            draw::text(
                buf,
                x,
                rect.y + 4 + row as u16,
                line,
                theme.bold(theme.ink),
                width,
            );
        }

        let y = rect.bottom().saturating_sub(4);
        let retry = Some(theme.chip(theme.yellow));
        let button = draw::button(buf, x, y, keymap::REFRESH.help_key, "RETRY", retry, theme);
        let after = draw::after_button(x, button);
        let quit = format!("{}  quit", keymap::QUIT.help_key);
        draw::text(
            buf,
            after,
            y,
            &quit,
            theme.text(theme.ink),
            rect.right().saturating_sub(after + 2),
        );
    }

    /// Draws the dotted backdrop and a centred dialog on it: a band in the
    /// dialog's colour naming it, a rule, and room for a body and a row of
    /// buttons, which the caller draws.
    fn render_dialog(&self, buf: &mut Buffer, area: Rect, dialog: &Dialog) -> DialogLayout {
        let theme = &self.theme;
        draw::backdrop(buf, area, theme.decoration());

        let width = dialog_width(area, dialog.width);
        let height = (dialog.body_rows + 10).min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + area.height.saturating_sub(height + 1) / 2,
            width,
            height,
        );
        let shadow = if dialog.hazard && theme.is_dark() {
            theme.red
        } else {
            theme.shadow
        };
        draw::clear(buf, rect);
        draw::outline_with_shadow(buf, rect, &HEAVY, theme.text(theme.ink), shadow);

        let inner = rect.inner(Margin::new(1, 1));
        let band = Rect::new(inner.x, inner.y, inner.width, inner.height.min(3));
        draw::fill(buf, band, theme.fill(dialog.band));
        if dialog.hazard {
            draw::hazard(buf, band.x, band.y, band.width, 0, theme);
            draw::hazard(buf, band.x, band.y + 2, band.width, 1, theme);
        }
        let x = inner.x + 2;
        let y = inner.y + 1;
        let after = x + draw::chip(buf, x, y, dialog.icon, theme.on_ink(dialog.band)) + 1;
        let room = inner.right().saturating_sub(after + 2);
        draw::text(buf, after, y, dialog.title, theme.chip(dialog.band), room);
        if rect.height > 5 {
            draw::tee(buf, rect, inner.y + 3, &HEAVY, theme.text(theme.ink));
        }

        DialogLayout {
            body: Rect::new(
                x,
                rect.y + 6,
                inner.width.saturating_sub(4),
                dialog.body_rows,
            ),
            buttons: rect.bottom().saturating_sub(3),
        }
    }

    fn render_form(&self, buf: &mut Buffer, area: Rect, form: &CreateForm) -> Option<Position> {
        let theme = &self.theme;
        // The boxed fields need about 32 rows. On a shorter terminal they
        // collapse to one row each rather than run off the bottom.
        let compact = form.height(false) + 11 > area.height;
        let layout = self.render_dialog(
            buf,
            area,
            &Dialog {
                band: theme.violet,
                icon: "+",
                title: "NEW SSH KEY",
                width: 74,
                body_rows: form.height(compact),
                hazard: false,
            },
        );
        let cursor = form.render(buf, layout.body, compact, theme);

        let (body, y) = (layout.body, layout.buttons);
        let create = draw::button(
            buf,
            body.x,
            y,
            "enter",
            "CREATE",
            Some(theme.chip(theme.yellow)),
            theme,
        );
        let x = draw::after_button(body.x, create);
        let cancel = draw::button(buf, x, y, "esc", "CANCEL", Some(theme.code_chip()), theme);
        let hint = "tab  next field";
        let hint_x = body.right().saturating_sub(text::width(hint) as u16);
        if hint_x >= draw::after_button(x, cancel) {
            draw::text(buf, hint_x, y, hint, theme.text(theme.faint), u16::MAX);
        }

        cursor
    }

    /// The key a dialog acts on: its title, optionally with its algorithm
    /// chip, and the vault it is in beneath.
    fn render_subject(&self, buf: &mut Buffer, body: Rect, key: &Key, with_chip: bool) {
        let theme = &self.theme;
        let chip = with_chip.then(|| self.algorithm_chip(key)).flatten();
        let chip_width = chip
            .as_ref()
            .map_or(0, |(label, _)| text::width(label) as u16 + 3);
        let room = body.width.saturating_sub(chip_width);
        let title = text::truncate(&key.title, usize::from(room));
        let end = draw::text(buf, body.x, body.y, &title, theme.bold(theme.ink), room);
        if let Some((label, style)) = chip {
            draw::chip(buf, end + 1, body.y, &label, style);
        }

        let vault = format!("in {}", key.vault_name);
        let vault = text::truncate(&vault, usize::from(body.width));
        draw::text(
            buf,
            body.x,
            body.y + 1,
            &vault,
            theme.text(theme.faint),
            body.width,
        );
    }

    fn render_trash_prompt(&self, buf: &mut Buffer, area: Rect, key: &Key) {
        let theme = &self.theme;
        let layout = self.render_dialog(
            buf,
            area,
            &Dialog {
                band: theme.orange,
                icon: keymap::TRASH.help_key,
                title: "MOVE TO TRASH?",
                width: 60,
                body_rows: 7,
                hazard: false,
            },
        );
        let body = layout.body;

        self.render_subject(buf, body, key, true);
        let restore = "Recoverable. Restore it later with:";
        draw::text(
            buf,
            body.x,
            body.y + 3,
            restore,
            theme.text(theme.muted),
            body.width,
        );
        let command = format!("$ {BINARY} item untrash --share-id …");
        let block = self.code_block(&[command], usize::from(body.width));
        for (row, line) in block.iter().enumerate() {
            draw::line(buf, body.x, body.y + 4 + row as u16, line, body.width);
        }

        let y = layout.buttons;
        let trash = draw::button(
            buf,
            body.x,
            y,
            "y",
            "TRASH",
            Some(theme.chip(theme.orange)),
            theme,
        );
        let x = draw::after_button(body.x, trash);
        draw::button(buf, x, y, "n", "CANCEL", Some(theme.code_chip()), theme);
    }

    /// Returns where the terminal cursor belongs: in the confirmation input.
    fn render_delete_prompt(&self, buf: &mut Buffer, area: Rect, key: &Key) -> Position {
        // The input's box starts on the body's eighth row.
        const INPUT_ROW: u16 = 7;

        let theme = &self.theme;
        let layout = self.render_dialog(
            buf,
            area,
            &Dialog {
                band: theme.red,
                icon: GLYPH_WARN,
                title: "PERMANENTLY DELETE?",
                width: 66,
                body_rows: 11,
                hazard: true,
            },
        );
        let body = layout.body;

        self.render_subject(buf, body, key, false);
        draw::chip(
            buf,
            body.x,
            body.y + 3,
            "THIS CANNOT BE UNDONE",
            theme.chip(theme.red),
        );
        let consequence = "The key is destroyed, not moved to the trash.";
        draw::text(
            buf,
            body.x,
            body.y + 4,
            consequence,
            theme.text(theme.danger),
            body.width,
        );
        let prompt = "TYPE THE TITLE TO CONFIRM";
        draw::text(
            buf,
            body.x,
            body.y + 6,
            prompt,
            theme.bold(theme.muted),
            body.width,
        );

        let input = Rect::new(body.x, body.y + INPUT_ROW, body.width.saturating_sub(2), 3);
        draw::outline_with_shadow(buf, input, &HEAVY, theme.text(theme.ink), theme.red);

        let count = format!(
            "{}/{}",
            self.confirm.value().chars().count(),
            key.title.chars().count()
        );
        let count_x = input.right().saturating_sub(3 + text::width(&count) as u16);
        draw::text(
            buf,
            count_x,
            input.y + 1,
            &count,
            theme.text(theme.faint),
            u16::MAX,
        );

        let text_x = input.x + 2;
        let width = count_x.saturating_sub(text_x + 2);
        let (spans, column) = self.confirm.render(
            theme.bold(theme.ink),
            theme.text(theme.faint),
            usize::from(width),
        );
        draw::line(buf, text_x, input.y + 1, &Line::from(spans), width);
        let cursor = Position::new(text_x + column, input.y + 1);
        draw::restyle(buf, cursor.x, cursor.y, theme.cursor());

        // Delete stays disabled until the typed title matches.
        let matched = self.confirm.value() == key.title;
        let y = layout.buttons;
        let delete = matched.then(|| theme.chip(theme.red));
        let x = draw::after_button(
            body.x,
            draw::button(buf, body.x, y, "enter", "DELETE", delete, theme),
        );
        let cancel = draw::button(buf, x, y, "esc", "CANCEL", Some(theme.code_chip()), theme);
        let x = draw::after_button(x, cancel);
        let instead = format!("{}  trashes instead", keymap::TRASH.help_key);
        draw::text(
            buf,
            x,
            y,
            &instead,
            theme.text(theme.faint),
            body.right().saturating_sub(x),
        );

        cursor
    }

    fn render_agent_panel(&self, buf: &mut Buffer, area: Rect) {
        const PREFERRED_WIDTH: u16 = 72;

        let theme = &self.theme;
        let status = &self.agent;
        let export = (status.state == AgentState::Running && !status.socket.is_empty())
            .then(|| format!("export SSH_AUTH_SOCK={}", status.socket));

        // Wide enough for the export line where the terminal allows, so it
        // can be selected and copied whole; wrapped where it does not.
        let wanted = export.as_ref().map_or(PREFERRED_WIDTH, |line| {
            (text::width(line) as u16 + 8).max(PREFERRED_WIDTH)
        });
        let code_width = dialog_width(area, wanted).saturating_sub(8);
        let export_lines = export
            .as_ref()
            .map(|line| text::wrap(line, usize::from(code_width)))
            .unwrap_or_default();

        let details: Vec<(&str, &str)> = if self.agent_busy {
            Vec::new()
        } else {
            [
                ("PID", status.pid.as_str()),
                ("SOCKET", status.socket.as_str()),
            ]
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .collect()
        };
        let point = if export_lines.is_empty() {
            0
        } else {
            export_lines.len() as u16 + 4
        };
        let layout = self.render_dialog(
            buf,
            area,
            &Dialog {
                band: theme.mint,
                icon: keymap::AGENT.help_key,
                title: "SSH AGENT DAEMON",
                width: wanted,
                body_rows: 2 + details.len() as u16 + point,
                hazard: false,
            },
        );
        let body = layout.body;
        let label = theme.bold(theme.muted);
        let value_x = body.x + AGENT_LABEL_WIDTH;
        let value_width = body.width.saturating_sub(AGENT_LABEL_WIDTH);

        let mut y = body.y;
        draw::text(buf, body.x, y, "STATUS", label, AGENT_LABEL_WIDTH);
        if self.agent_busy {
            let checking = format!("{} Checking…", self.spinner_frame());
            draw::text(
                buf,
                value_x,
                y,
                &checking,
                theme.text(theme.faint),
                value_width,
            );
        } else {
            let (state, style) = self.agent_state_chip();
            let width = draw::chip(buf, value_x, y, &state, style);
            draw::shadow(buf, Rect::new(value_x, y, width, 1), theme.shadow);
        }
        y += 2;

        for (name, value) in details {
            draw::text(buf, body.x, y, name, label, AGENT_LABEL_WIDTH);
            let value = text::truncate(value, usize::from(value_width));
            draw::text(buf, value_x, y, &value, theme.text(theme.ink), value_width);
            y += 1;
        }

        if !export_lines.is_empty() {
            y += 1;
            draw::chip(buf, body.x, y, "POINT SSH AT IT", theme.inverted());
            y += 1;
            for line in self.code_block(&export_lines, usize::from(body.width)) {
                draw::line(buf, body.x, y, &line, body.width);
                y += 1;
            }
        }

        // Each action is disabled when it would do nothing.
        let y = layout.buttons;
        let start = (status.state != AgentState::Running).then(|| theme.chip(theme.mint));
        let stop = (status.state != AgentState::Stopped).then(|| theme.chip(theme.pink));
        let mut x = body.x;
        x = draw::after_button(x, draw::button(buf, x, y, "s", "START", start, theme));
        x = draw::after_button(x, draw::button(buf, x, y, "x", "STOP", stop, theme));
        let refresh = Some(theme.chip(theme.yellow));
        x = draw::after_button(x, draw::button(buf, x, y, "r", "REFRESH", refresh, theme));
        draw::text(
            buf,
            x,
            y,
            "esc  back",
            theme.text(theme.muted),
            body.right().saturating_sub(x),
        );
    }

    /// The daemon's state as a chip, so it reads before the text does.
    fn agent_state_chip(&self) -> (String, Style) {
        let theme = &self.theme;
        let status = &self.agent;
        let or = |fallback: &str| {
            if status.detail.is_empty() {
                fallback.to_owned()
            } else {
                status.detail.clone()
            }
        };
        let (glyph, detail, style) = match status.state {
            AgentState::Running => (GLYPH_RUNNING, "running".to_owned(), theme.chip(theme.mint)),
            AgentState::Degraded => (GLYPH_DEGRADED, or("degraded"), theme.chip(theme.orange)),
            AgentState::Stopped => (GLYPH_STOPPED, or("stopped"), theme.code_chip()),
            AgentState::Unknown => (GLYPH_STOPPED, or("unknown"), theme.chip(theme.orange)),
        };

        (format!("{glyph} {}", detail.to_uppercase()), style)
    }
}

/// A dialog's width: what it asks for, but never closer than three columns to
/// either edge of the terminal.
fn dialog_width(area: Rect, wanted: u16) -> u16 {
    wanted
        .min(area.width.saturating_sub(6))
        .max(area.width.min(12))
}

fn value_width(width: usize) -> usize {
    width.saturating_sub(LABEL_WIDTH).max(10)
}
