//! Drawing the model.
//!
//! Only public key material is ever drawn: protui has no code path that reads
//! a private key, so none can reach the terminal.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};

use crate::keys::{Key, State};
use crate::passcli::{AgentState, AgentStatus, BINARY};
use crate::ui::create_form::CreateForm;
use crate::ui::key_list::FilterState;
use crate::ui::keymap;
use crate::ui::model::{FOOTER_ROWS, HEADER_ROWS, Mode, Model, Tone};
use crate::ui::text;
use crate::ui::theme::{
    GLYPH_DEGRADED, GLYPH_ERROR, GLYPH_MARK, GLYPH_RULE, GLYPH_RUNNING, GLYPH_SELECTED,
    GLYPH_SEPARATOR, GLYPH_STOPPED, GLYPH_WARN,
};

/// The left column shared by every label/value row.
const LABEL_WIDTH: usize = 13;

/// How a dialog is framed.
#[derive(Clone, Copy)]
enum DialogLook {
    /// Rounded corners, which read as softer than square ones and suit a
    /// prompt.
    Prompt,
    /// A red, heavy, square frame, so the irreversible action does not look
    /// like the others.
    Danger,
}

impl Model {
    /// Draws the whole screen.
    pub fn render(&self, frame: &mut Frame<'_>) {
        if self.width == 0 {
            frame.render_widget(Paragraph::new("Loading…"), frame.area());
            return;
        }

        let area = Rect::new(0, 0, self.width, self.height).intersection(frame.area());
        match &self.mode {
            Mode::List => {
                self.render_list_screen(frame, area);
                return;
            }
            Mode::Create(form) => self.render_form(frame, area, form),
            Mode::ConfirmTrash(key) => self.render_trash_prompt(frame, area, key),
            Mode::ConfirmDelete(key) => self.render_delete_prompt(frame, area, key),
            Mode::Agent => self.render_agent_panel(frame, area),
        }

        // A dialog has no footer, but what it reports — a title that did not
        // match, an agent that would not start — still has to be seen.
        if self.status.is_some() && area.height > 0 {
            let bottom = Rect::new(area.x, area.bottom() - 1, area.width, 1);
            frame.render_widget(Paragraph::new(self.status_line()), bottom);
        }
    }

    /// The transient status message, or a blank line when there is none.
    fn status_line(&self) -> Line<'static> {
        let Some(status) = &self.status else {
            return Line::default();
        };
        let style = match status.tone {
            Tone::Muted => self.theme.muted(),
            Tone::Ok => self.theme.ok(),
            Tone::Warn => self.theme.warn(),
            Tone::Error => self.theme.error(),
        };
        let width = usize::from(self.width).saturating_sub(4).max(10);

        Line::from(vec![
            Span::raw("  "),
            Span::styled(text::truncate(&status.text, width).into_owned(), style),
        ])
    }

    fn render_list_screen(&self, frame: &mut Frame<'_>, area: Rect) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(HEADER_ROWS),
            Constraint::Fill(1),
            Constraint::Length(FOOTER_ROWS + self.help_rows()),
        ])
        .areas(area);

        frame.render_widget(Paragraph::new(self.header()), header);
        self.render_footer(frame, footer);

        if let Some(error) = &self.error {
            self.render_error_body(frame, body, error);
            return;
        }

        // With nothing in the list there is no selected key for a detail pane
        // to describe, so the loading and empty states take the full width
        // rather than sitting in one half beside an empty pane.
        if self.list.is_empty() {
            self.render_empty_body(frame, body);
            return;
        }

        // The detail pane sits beside the list when there is room, and is
        // dropped entirely when there is not: below that width, splitting
        // leaves neither side readable.
        let [list, detail] = Layout::horizontal([
            Constraint::Length(self.list_width()),
            Constraint::Length(self.detail_pane_width()),
        ])
        .areas(body);

        frame.render_widget(
            Paragraph::new(self.list_lines(usize::from(list.width))),
            list,
        );

        if self.has_detail_pane() {
            let pane = Block::new()
                .borders(Borders::LEFT)
                .border_style(self.theme.rule())
                .padding(Padding::left(2));
            frame.render_widget(Paragraph::new(self.detail_window()).block(pane), detail);
        }
    }

    /// The wordmark on the left and a count on the right, over a rule. There
    /// is no filled banner: a block of colour across the top reads as chrome,
    /// and the list is what deserves the attention.
    fn header(&self) -> Vec<Line<'static>> {
        let count = self.header_count();
        let width = usize::from(self.width);
        let gap = width
            .saturating_sub(text::width("◆ protui") + text::width(&count) + 4)
            .max(1);

        vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled(GLYPH_MARK, self.theme.accent()),
                Span::raw(" "),
                Span::styled("protui", self.theme.wordmark()),
                Span::raw(" ".repeat(gap)),
                Span::styled(count, self.theme.faint()),
            ]),
            self.rule_line(),
        ]
    }

    /// Reports where the cursor is as well as how many keys there are. Only a
    /// fraction of a long list fits on screen and nothing else indicates
    /// position, so a bare total would leave a user unable to tell whether
    /// more exists below.
    fn header_count(&self) -> String {
        let total = self.list.len();
        if total == 0 {
            return if self.busy() {
                format!("{} loading", self.spinner_frame())
            } else {
                "no keys".to_owned()
            };
        }

        let visible = self.list.visible_len();
        let position = (self.list.cursor() + 1).min(visible);

        // While a filter is set, both numbers matter: where you are in the
        // matches, and how much of the collection they represent.
        if !self.list.filter().value().is_empty() {
            return format!("{position}/{visible} of {total} keys");
        }
        if total == 1 {
            return "1 key".to_owned();
        }

        format!("{position}/{total} keys")
    }

    fn rule_line(&self) -> Line<'static> {
        let width = usize::from(self.width).saturating_sub(4).max(1);

        Line::from(vec![
            Span::raw("  "),
            Span::styled(GLYPH_RULE.repeat(width), self.theme.rule()),
        ])
    }

    /// The transient status line above the help bar, separated from the body
    /// by a rule.
    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let width = usize::from(self.width);
        let mut lines = vec![self.rule_line()];

        lines.push(self.status_line());

        // The filter owns the line while it is being typed.
        if self.list.filter_state() == FilterState::Typing {
            let (mut spans, cursor) = self.list.filter().render(
                self.theme.text(),
                self.theme.faint(),
                width.saturating_sub(4),
            );
            spans.insert(0, Span::styled("  /", self.theme.accent()));
            lines.push(Line::from(spans));

            let position = Position::new(area.x + 3 + cursor, area.y + 2);
            if area.contains(position) {
                frame.set_cursor_position(position);
            }
        } else if self.full_help {
            lines.extend(self.full_help_lines(width));
        } else {
            lines.push(self.short_help_line(width));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    /// The single help line: each binding's key and description, cut with an
    /// ellipsis where the terminal runs out.
    fn short_help_line(&self, width: usize) -> Line<'static> {
        const SEPARATOR: &str = " • ";

        let mut spans = vec![Span::raw("  ")];
        let mut used = 2;

        let bindings = keymap::SHORT_HELP.iter().enumerate();
        for (index, binding) in bindings {
            let separator = if index == 0 { "" } else { SEPARATOR };
            let needed = text::width(separator)
                + text::width(binding.help_key)
                + 1
                + text::width(binding.help);
            if used + needed > width {
                if used + 2 <= width {
                    spans.push(Span::styled(" …", self.theme.faint()));
                }
                break;
            }

            spans.push(Span::styled(separator, self.theme.rule()));
            spans.push(Span::styled(binding.help_key, self.theme.accent()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(binding.help, self.theme.faint()));
            used += needed;
        }

        Line::from(spans)
    }

    /// The expanded help: one column per group of related bindings.
    fn full_help_lines(&self, width: usize) -> Vec<Line<'static>> {
        const GAP: usize = 4;

        let rows = usize::from(self.help_rows());
        let mut lines: Vec<Vec<Span<'static>>> = vec![vec![Span::raw("  ")]; rows];
        let mut used = 2;

        let groups = keymap::FULL_HELP.iter().enumerate();
        for (index, group) in groups {
            let key_width = group
                .iter()
                .map(|b| text::width(b.help_key))
                .max()
                .unwrap_or(0);
            let help_width = group.iter().map(|b| text::width(b.help)).max().unwrap_or(0);
            let gap = if index == 0 { 0 } else { GAP };
            if used + gap + key_width + 1 + help_width > width {
                break;
            }

            let rows = lines.iter_mut().enumerate();
            for (row, line) in rows {
                line.push(Span::raw(" ".repeat(gap)));
                match group.get(row) {
                    Some(binding) => {
                        let key = format!("{:<key_width$} ", binding.help_key);
                        let help = format!("{:<help_width$}", binding.help);
                        line.push(Span::styled(key, self.theme.accent()));
                        line.push(Span::styled(help, self.theme.faint()));
                    }
                    None => line.push(Span::raw(" ".repeat(key_width + 1 + help_width))),
                }
            }
            used += gap + key_width + 1 + help_width;
        }

        lines.into_iter().map(Line::from).collect()
    }

    /// Centres a short message in the space the list would occupy, so a state
    /// with no list does not sit hard against the top-left corner.
    fn render_centred(frame: &mut Frame<'_>, area: Rect, lines: Vec<Line<'static>>) {
        let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        let centred = area.centered_vertically(Constraint::Length(height));

        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), centred);
    }

    /// Covers both states where the list has nothing in it: still loading, and
    /// genuinely empty.
    fn render_empty_body(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = if self.loading {
            vec![Line::styled(
                format!("{} Loading keys…", self.spinner_frame()),
                self.theme.faint(),
            )]
        } else {
            vec![
                Line::styled("No SSH keys found.", self.theme.muted()),
                Line::default(),
                Line::from(vec![
                    Span::styled("Press ", self.theme.faint()),
                    Span::styled("n", self.theme.accent()),
                    Span::styled(" to create one.", self.theme.faint()),
                ]),
            ]
        };

        Self::render_centred(frame, area, lines);
    }

    fn render_error_body(&self, frame: &mut Frame<'_>, area: Rect, error: &str) {
        let width = usize::from(area.width).saturating_sub(4).max(10);
        let mut lines: Vec<Line<'static>> = text::wrap(&format!("{GLYPH_ERROR} {error}"), width)
            .into_iter()
            .map(|line| Line::styled(line, self.theme.error()))
            .collect();
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("Press ", self.theme.faint()),
            Span::styled("r", self.theme.accent()),
            Span::styled(" to retry, ", self.theme.faint()),
            Span::styled("q", self.theme.accent()),
            Span::styled(" to quit.", self.theme.faint()),
        ]));

        Self::render_centred(frame, area, lines);
    }

    /// Draws each key as two lines: the title, then its metadata dimmed
    /// beneath it.
    ///
    /// Two lines rather than aligned columns, because a key list is scanned by
    /// name and columns force every field to the width of its longest value —
    /// one long vault name would push fingerprints off the edge of every other
    /// row.
    fn list_lines(&self, width: usize) -> Vec<Line<'static>> {
        // The gutter carries the selection bar; both lines indent past it so
        // the block of text stays aligned whether or not the row is selected.
        let text_width = width.saturating_sub(4).max(12);
        let mut lines = Vec::new();

        let on_screen = self.list.on_screen().take(self.visible_rows());
        for (position, key) in on_screen {
            let title = text::truncate(&key.title, text_width).into_owned();
            let summary = text::truncate(&Self::summary(key), text_width).into_owned();

            if position == self.list.cursor() {
                lines.push(Line::from(vec![
                    Span::styled(GLYPH_SELECTED, self.theme.accent()),
                    Span::raw(" "),
                    Span::styled(title, self.theme.selected()),
                ]));
                // The selected row's metadata is lifted one step out of the
                // background so it reads as part of the selection rather than
                // as greyed out.
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(summary, self.theme.muted()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(title, self.theme.text()),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(summary, self.theme.faint()),
                ]));
            }
            lines.push(Line::default());
        }

        lines
    }

    /// The second line of a row: where the key lives and what it is. Empty
    /// parts are dropped so the separator never dangles.
    ///
    /// Until the public key arrives the algorithm and fingerprint are unknown,
    /// and saying so beats printing a confident "unknown" for a key that is
    /// merely still loading.
    fn summary(key: &Key) -> String {
        let label = key.label();
        let parts: Vec<&str> = if key.public_key.is_empty() {
            vec![&key.vault_name, "loading…"]
        } else {
            vec![&key.vault_name, &label, key.short_fingerprint()]
        };

        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<&str>>()
            .join(&format!(" {GLYPH_SEPARATOR} "))
    }

    /// The slice of the detail pane currently on screen, with markers where
    /// there is more above or below.
    ///
    /// The pane is clamped rather than allowed to overflow: a
    /// taller-than-terminal block would be cut by the terminal itself,
    /// silently and from the bottom.
    #[must_use]
    pub(super) fn detail_window(&self) -> Vec<Line<'static>> {
        let lines = self.detail_lines();
        let height = self.body_height();
        if lines.len() <= height {
            return lines;
        }

        let offset = self.detail_offset.min(self.max_detail_offset());
        let mut window: Vec<Line<'static>> = lines[offset..offset + height].to_vec();

        // The markers replace a line rather than being added to one, so the
        // pane never grows past the height it was given.
        if offset > 0 {
            window[0] = Line::styled(format!("↑ {offset} more"), self.theme.faint());
        }
        let below = lines.len() - offset - height;
        if below > 0 {
            window[height - 1] = Line::styled(
                format!("↓ {below} more · ^e/^y scrolls"),
                self.theme.faint(),
            );
        }

        window
    }

    /// The detail pane for the highlighted key, one entry per line.
    ///
    /// It is a pure function of the model, so the update loop can measure it
    /// when clamping a scroll without the view having to record anything.
    #[must_use]
    pub(super) fn detail_lines(&self) -> Vec<Line<'static>> {
        let width = self.detail_inner_width();
        let Some(key) = self.list.selected() else {
            return vec![Line::styled("No key selected.", self.theme.faint())];
        };
        let pending = self.public_key_pending(key);

        let mut lines = vec![Line::styled(
            text::truncate(&key.title, width).into_owned(),
            self.theme.selected(),
        )];

        // The algorithm is a chip rather than another label/value row: it is
        // the one attribute worth recognising at a glance.
        let mut chip = vec![match &pending {
            Some(_) => Span::styled(
                format!("{} reading key", self.spinner_frame()),
                self.theme.faint(),
            ),
            None => Span::styled(format!(" {} ", key.label()), self.theme.badge()),
        }];
        if key.state == State::Trashed {
            chip.push(Span::raw(" "));
            chip.push(Span::styled("trashed", self.theme.warn()));
        }
        lines.push(Line::from(chip));
        lines.push(Line::default());

        lines.push(self.detail_row("Vault", &key.vault_name, Style::new(), width));

        // The fingerprint wraps rather than truncating. Comparing one against
        // a forge or `ssh-keygen -lf` is a main reason to open this pane, and a
        // fingerprint with its middle elided cannot be compared at all.
        let (fingerprint, fingerprint_style) = match &pending {
            Some(placeholder) => (placeholder.as_str(), self.theme.faint()),
            None => (key.fingerprint.as_str(), Style::new()),
        };
        lines.extend(self.detail_wrapped("Fingerprint", fingerprint, fingerprint_style, width));

        let (comment, comment_style) = match &pending {
            Some(placeholder) => (placeholder.as_str(), self.theme.faint()),
            None if key.comment.is_empty() => ("—", self.theme.faint()),
            None => (key.comment.as_str(), Style::new()),
        };
        lines.extend(self.detail_wrapped("Comment", comment, comment_style, width));

        // Upstream timestamps carry no zone, so they are shown as the
        // wall-clock values they are rather than converted.
        if let Some(created) = key.created_at {
            lines.push(self.detail_row("Created", &created.to_string(), Style::new(), width));
        }
        if let Some(modified) = key.modified_at {
            lines.push(self.detail_row("Modified", &modified.to_string(), Style::new(), width));
        }

        // Item IDs are long base64; truncating one makes it useless for
        // passing back to pass-cli, which is the only reason to show it.
        lines.extend(self.detail_wrapped("Item ID", key.id.as_str(), Style::new(), width));

        lines.push(Line::default());
        lines.push(Line::styled("PUBLIC KEY", self.theme.section()));

        if pending.is_some() {
            lines.push(Line::styled(
                format!("{} loading…", self.spinner_frame()),
                self.theme.faint(),
            ));
        } else {
            lines.extend(
                text::wrap(&key.public_key, width.saturating_sub(2).max(20))
                    .into_iter()
                    .map(|line| Line::styled(line, self.theme.muted())),
            );
        }

        lines
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
        Span::styled(format!("{label:<LABEL_WIDTH$}"), self.theme.label())
    }

    /// Frames a modal as title, rule, body, key hints, so every prompt has the
    /// same shape, and centres it. Returns where the body's first line starts,
    /// for placing a cursor inside it.
    fn render_dialog(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        look: DialogLook,
        title: Line<'static>,
        body: Vec<Line<'static>>,
        hints: &'static str,
    ) -> Position {
        // The rule spans the widest line so the header separator reaches the
        // edge of the content rather than stopping under the title.
        let width = body
            .iter()
            .map(Line::width)
            .chain([title.width(), text::width(hints)])
            .max()
            .unwrap_or(0);

        let mut lines = vec![
            title,
            Line::styled(GLYPH_RULE.repeat(width), self.theme.rule()),
            Line::default(),
        ];
        lines.extend(body);
        lines.push(Line::default());
        lines.push(Line::styled(hints, self.theme.faint()));

        let (border_type, border_style) = match look {
            DialogLook::Prompt => (BorderType::Rounded, self.theme.accent_dim()),
            DialogLook::Danger => (BorderType::Thick, self.theme.error()),
        };
        let block = Block::bordered()
            .border_type(border_type)
            .border_style(border_style)
            .padding(Padding::symmetric(3, 1));

        let outer_width = u16::try_from(width + 8).unwrap_or(u16::MAX);
        let outer_height = u16::try_from(lines.len() + 4).unwrap_or(u16::MAX);
        let rect = area
            .centered(
                Constraint::Length(outer_width),
                Constraint::Length(outer_height),
            )
            .intersection(area);
        let inner = block.inner(rect);

        frame.render_widget(Paragraph::new(lines).block(block), rect);

        Position::new(inner.x, inner.y + 3)
    }

    fn render_form(&self, frame: &mut Frame<'_>, area: Rect, form: &CreateForm) {
        let (body, cursor) = form.render(&self.theme);
        let origin = self.render_dialog(
            frame,
            area,
            DialogLook::Prompt,
            Line::styled("New SSH key", self.theme.title()),
            body,
            "tab  next    ←/→  change    enter  create    esc  cancel",
        );

        if let Some(cursor) = cursor {
            place_cursor(
                frame,
                area,
                Position::new(origin.x + cursor.x, origin.y + cursor.y),
            );
        }
    }

    fn render_trash_prompt(&self, frame: &mut Frame<'_>, area: Rect, key: &Key) {
        let body = vec![
            Line::styled(key.title.clone(), self.theme.selected()),
            Line::styled(format!("in {}", key.vault_name), self.theme.faint()),
            Line::default(),
            Line::styled("Restore it later with:", self.theme.muted()),
            Line::styled(format!("{BINARY} item untrash"), self.theme.accent()),
        ];

        self.render_dialog(
            frame,
            area,
            DialogLook::Prompt,
            Line::styled("Move to trash?", self.theme.title()),
            body,
            "y  trash    n  cancel",
        );
    }

    fn render_delete_prompt(&self, frame: &mut Frame<'_>, area: Rect, key: &Key) {
        // The input is the body's eighth line.
        const INPUT_ROW: u16 = 7;
        const PROMPT: &str = "› ";

        let input_width = usize::from(area.width).saturating_sub(16).max(10);
        let (mut input, cursor) =
            self.confirm
                .render(self.theme.text(), self.theme.faint(), input_width);
        input.insert(0, Span::styled(PROMPT, self.theme.error()));

        let body = vec![
            Line::styled(key.title.clone(), self.theme.selected()),
            Line::styled(format!("in {}", key.vault_name), self.theme.faint()),
            Line::default(),
            Line::styled(
                "This cannot be undone. The key is destroyed,",
                self.theme.error(),
            ),
            Line::styled("not moved to the trash.", self.theme.error()),
            Line::default(),
            Line::styled("Type the title to confirm:", self.theme.muted()),
            Line::from(input),
        ];

        let origin = self.render_dialog(
            frame,
            area,
            DialogLook::Danger,
            Line::styled(
                format!("{GLYPH_WARN}  Permanently delete?"),
                self.theme.danger_title(),
            ),
            body,
            "enter  delete    esc  cancel    d  trashes instead",
        );

        let column = origin.x + text::width(PROMPT) as u16 + cursor;
        place_cursor(frame, area, Position::new(column, origin.y + INPUT_ROW));
    }

    fn render_agent_panel(&self, frame: &mut Frame<'_>, area: Rect) {
        const ROW_WIDTH: usize = 60;

        let mut body = Vec::new();
        if self.agent_busy {
            body.push(Line::styled(
                format!("{} Checking…", self.spinner_frame()),
                self.theme.faint(),
            ));
        } else {
            body.push(self.agent_state_line(&self.agent));
            if !self.agent.pid.is_empty() {
                body.push(self.detail_row("PID", &self.agent.pid, Style::new(), ROW_WIDTH));
            }
            if !self.agent.socket.is_empty() {
                body.push(self.detail_row("Socket", &self.agent.socket, Style::new(), ROW_WIDTH));
            }
        }

        if self.agent.state == AgentState::Running && !self.agent.socket.is_empty() {
            body.push(Line::default());
            body.push(Line::styled("POINT SSH AT IT", self.theme.section()));
            body.push(Line::styled(
                format!("export SSH_AUTH_SOCK={}", self.agent.socket),
                self.theme.muted(),
            ));
        }

        self.render_dialog(
            frame,
            area,
            DialogLook::Prompt,
            Line::styled("SSH agent daemon", self.theme.title()),
            body,
            "s  start    x  stop    r  refresh    esc  back",
        );
    }

    /// The daemon state as a coloured dot plus its label, so the state is
    /// legible before the text is read.
    fn agent_state_line(&self, status: &AgentStatus) -> Line<'static> {
        let or = |fallback: &str| {
            if status.detail.is_empty() {
                fallback.to_owned()
            } else {
                status.detail.clone()
            }
        };
        let (glyph, detail, style) = match status.state {
            AgentState::Running => (GLYPH_RUNNING, "running".to_owned(), self.theme.ok()),
            AgentState::Degraded => (GLYPH_DEGRADED, status.detail.clone(), self.theme.warn()),
            AgentState::Stopped => (GLYPH_STOPPED, or("stopped"), self.theme.muted()),
            AgentState::Unknown => (GLYPH_STOPPED, or("unknown"), self.theme.warn()),
        };

        Line::from(vec![
            self.detail_label("Status"),
            Span::styled(format!("{glyph} {detail}"), style),
        ])
    }
}

fn value_width(width: usize) -> usize {
    width.saturating_sub(LABEL_WIDTH + 1).max(10)
}

/// Shows the terminal cursor at `position`, if it is on screen.
fn place_cursor(frame: &mut Frame<'_>, area: Rect, position: Position) {
    if area.contains(position) {
        frame.set_cursor_position(position);
    }
}
