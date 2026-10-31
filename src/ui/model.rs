//! The application state.

use std::collections::VecDeque;

use crate::keys::{ItemRef, Key, Vault};
use crate::passcli::AgentStatus;
use crate::ui::create_form::CreateForm;
use crate::ui::key_list::{FilterState, KeyList};
use crate::ui::keymap;
use crate::ui::message::Command;
use crate::ui::text_input::{Echo, TextInput};
use crate::ui::theme::Theme;

/// Bounds how many `item view` calls run at once.
///
/// Deriving the algorithm and fingerprint needs one call per item, since the
/// list output carries no key material. Fetching them all at once would spawn
/// a process per key, so they are drained through a small window instead.
pub const MAX_PUBLIC_KEY_FETCHES: usize = 4;

/// The terminal width below which the detail pane is dropped and the list gets
/// the full width.
pub const DETAIL_PANE_MIN_WIDTH: u16 = 92;

/// What comes off the detail pane's slot before its text: the gap before the
/// box, both borders, a two-column margin inside each, and the shadow.
pub const DETAIL_CHROME: u16 = 9;

/// The filled band and the rule beneath it.
pub const HEADER_ROWS: u16 = 4;

/// Where the detail box starts: under the header and a blank row.
pub const DETAIL_TOP: u16 = 5;

/// Where the list starts: under the header, the section label and a blank row
/// either side of it.
pub const LIST_TOP: u16 = 7;

/// Between the list and the footer: the row that says more keys are below,
/// the status line, and a blank row.
pub const STATUS_ROWS: u16 = 3;

/// The footer's padding rows, above and below the keys.
pub const FOOTER_ROWS: u16 = 2;

/// The detail box's title band and the rule under it, which do not scroll.
pub const DETAIL_BAND_ROWS: usize = 4;

/// Each key is drawn as two lines, then a blank one.
pub const ROW_HEIGHT: usize = 3;

/// A braille dot that walks in place, so nothing beside it shifts.
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Which screen owns the keyboard.
#[derive(Debug)]
pub enum Mode {
    List,
    Create(CreateForm),
    /// Asks before moving the key to the trash.
    ConfirmTrash(Key),
    /// Asks for the key's title before destroying it.
    ConfirmDelete(Key),
    Agent,
}

/// How a status message reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Ok,
    Warn,
    Error,
}

/// A transient message above the help bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub tone: Tone,
}

/// How far along quitting is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    No,
    /// Asked for while a change was in flight: quit once it finishes.
    AfterChanges,
    Now,
}

/// The input that takes the typed title required to permanently delete. It
/// holds at least as many characters as the title, however long.
#[must_use]
pub(super) fn confirm_input(title_length: usize) -> TextInput {
    TextInput::new(
        "type the title to confirm",
        title_length.max(128),
        Echo::Normal,
    )
}

/// protui's root model.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags, not one state"
)]
pub struct Model {
    pub(super) theme: Theme,
    pub(super) mode: Mode,

    /// Takes the typed title required to permanently delete.
    pub(super) confirm: TextInput,

    pub(super) vaults: Vec<Vault>,
    pub(super) list: KeyList,

    /// Increments on every reload, so results of a superseded load are
    /// recognised and dropped.
    pub(super) generation: u64,
    /// Keys still awaiting a public key fetch; `inflight` counts those in
    /// progress.
    pub(super) pending: VecDeque<ItemRef>,
    pub(super) inflight: usize,
    /// Vaults whose item list has not come back yet.
    pub(super) vaults_loading: usize,

    pub(super) agent: AgentStatus,
    pub(super) agent_busy: bool,

    /// Set between the two presses of vim's gg. A bare g is a prefix in vim
    /// and never acts alone, so the first press only arms this.
    pub(super) pending_g: bool,

    /// Scrolls the detail pane, which on a short terminal holds more than it
    /// can show. `detail_for` is the key that offset belongs to, so moving to
    /// another key starts it at the top again.
    pub(super) detail_offset: usize,
    pub(super) detail_for: Option<ItemRef>,

    pub(super) spinner: usize,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) full_help: bool,

    pub(super) loading: bool,
    pub(super) error: Option<String>,

    pub(super) status: Option<Status>,
    pub(super) status_token: u64,

    /// Commands that change something and have not reported back yet.
    pub(super) changes_in_flight: usize,
    pub(super) quit: Quit,
}

impl Model {
    /// Builds the model in its starting state: loading, with nothing listed.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            mode: Mode::List,
            confirm: confirm_input(0),
            vaults: Vec::new(),
            list: KeyList::default(),
            generation: 0,
            pending: VecDeque::new(),
            inflight: 0,
            vaults_loading: 0,
            agent: AgentStatus::default(),
            agent_busy: false,
            pending_g: false,
            detail_offset: 0,
            detail_for: None,
            spinner: 0,
            width: 0,
            height: 0,
            full_help: false,
            loading: true,
            error: None,
            status: None,
            status_token: 0,
            changes_in_flight: 0,
            quit: Quit::No,
        }
    }

    /// The commands that start the first load. The vault list is not among
    /// them: it is fetched alongside the session check, before the UI starts,
    /// and arrives as a [`crate::ui::message::Message::VaultsLoaded`].
    #[must_use = "the commands start the first load"]
    pub fn init() -> Vec<Command> {
        vec![Command::LoadAgentStatus]
    }

    /// Reports whether anything is outstanding, which is what the spinner
    /// animates.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.loading || self.agent_busy || self.inflight > 0
    }

    /// Reports whether the user has asked to quit.
    #[must_use]
    pub fn is_quitting(&self) -> bool {
        match self.quit {
            Quit::No => false,
            Quit::AfterChanges => self.changes_in_flight == 0,
            Quit::Now => true,
        }
    }

    /// Quits, unless a change is still in flight; then quits once it has
    /// finished. Asking a second time quits regardless.
    pub(super) fn request_quit(&mut self) -> Vec<Command> {
        if self.changes_in_flight == 0 || self.quit == Quit::AfterChanges {
            self.quit = Quit::Now;
            return Vec::new();
        }

        self.quit = Quit::AfterChanges;
        let notice = "Waiting for pass-cli to finish the change; quit again to stop it.";
        vec![self.notify(notice.to_owned(), Tone::Warn)]
    }

    /// The spinner glyph for the current tick.
    #[must_use]
    pub(super) fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner % SPINNER_FRAMES.len()]
    }

    /// Reports whether the terminal is wide enough for the detail pane.
    #[must_use]
    pub(super) fn has_detail_pane(&self) -> bool {
        self.width >= DETAIL_PANE_MIN_WIDTH
    }

    /// The columns the key list occupies: half the screen beside the detail
    /// pane, all of it otherwise.
    #[must_use]
    pub(super) fn list_width(&self) -> u16 {
        if self.has_detail_pane() {
            self.width / 2
        } else {
            self.width
        }
    }

    /// The horizontal slot the detail pane occupies, border and padding
    /// included. The last two columns stay empty, mirroring the header's
    /// left margin.
    #[must_use]
    pub(super) fn detail_pane_width(&self) -> u16 {
        self.width
            .saturating_sub(self.list_width())
            .saturating_sub(2)
    }

    /// The room the detail pane's text actually has.
    #[must_use]
    pub(super) fn detail_inner_width(&self) -> usize {
        usize::from(self.detail_pane_width().saturating_sub(DETAIL_CHROME))
    }

    /// How many lines the help area takes.
    #[must_use]
    pub(super) fn help_rows(&self) -> u16 {
        if self.full_help && self.list.filter_state() != FilterState::Typing {
            keymap::FULL_HELP
                .iter()
                .map(|group| group.len())
                .max()
                .unwrap_or(1) as u16
        } else {
            1
        }
    }

    /// The rows the list has, which are also the rows inside the detail box.
    #[must_use]
    pub(super) fn body_height(&self) -> usize {
        let chrome = LIST_TOP + STATUS_ROWS + FOOTER_ROWS + self.help_rows();
        usize::from(self.height.saturating_sub(chrome)).max(1)
    }

    /// Reports whether the detail box has room for its title band and still
    /// shows a few rows beneath it. On a very short terminal the band is
    /// dropped so the details themselves stay visible.
    #[must_use]
    pub(super) fn has_detail_band(&self) -> bool {
        self.body_height() >= DETAIL_BAND_ROWS * 2
    }

    /// The rows the detail pane's scrolling part has.
    #[must_use]
    pub(super) fn detail_body_height(&self) -> usize {
        if self.has_detail_band() {
            self.body_height() - DETAIL_BAND_ROWS
        } else {
            self.body_height()
        }
    }

    /// How many keys fit on screen, used to size a page jump.
    #[must_use]
    pub(super) fn visible_rows(&self) -> usize {
        (self.body_height() / ROW_HEIGHT).max(1)
    }

    /// Sets a transient status line and schedules its expiry.
    pub(super) fn notify(&mut self, text: String, tone: Tone) -> Command {
        self.status_token += 1;
        self.status = Some(Status { text, tone });

        Command::ExpireStatus {
            token: self.status_token,
        }
    }

    /// Discards the current list and re-reads everything from pass-cli.
    pub(super) fn reload(&mut self) -> Command {
        self.generation += 1;
        self.list.clear();
        self.pending.clear();
        self.inflight = 0;
        self.vaults_loading = 0;
        self.loading = true;
        self.error = None;

        Command::LoadVaults {
            generation: self.generation,
        }
    }

    /// Drains the pending queue up to the concurrency window.
    pub(super) fn next_public_key_fetches(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();

        while self.inflight < MAX_PUBLIC_KEY_FETCHES {
            let Some(item) = self.pending.pop_front() else {
                break;
            };
            self.inflight += 1;
            commands.push(Command::LoadPublicKey {
                generation: self.generation,
                item,
            });
        }

        commands
    }
}
