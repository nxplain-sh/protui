//! The update loop: every input becomes new state, plus the effects to run.
//!
//! Nothing here performs I/O. Every pass-cli call is returned as a
//! [`Command`] for the runtime to execute, so a slow call can never freeze the
//! interface — including the ability to quit.

use std::mem;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::clipboard;
use crate::keys::{ItemRef, Key, Vault};
use crate::passcli::{AgentState, AgentStatus, CommandError};
use crate::ui::create_form::CreateForm;
use crate::ui::key_list::FilterState;
use crate::ui::keymap::{self, Binding};
use crate::ui::message::{Command, Message, Removal};
use crate::ui::model::{self, Mode, Model, Quit, Tone};

/// What a key press in the list asks for.
#[derive(Clone, Copy, Debug)]
enum Action {
    Quit,
    Up,
    Down,
    Bottom,
    PageDown,
    PageUp,
    HalfDown,
    HalfUp,
    ScrollDown,
    ScrollUp,
    Help,
    Refresh,
    Agent,
    New,
    Copy,
    Trash,
    Delete,
    Filter,
}

/// The list's bindings, in priority order.
const LIST_ACTIONS: [(&Binding, Action); 18] = [
    (&keymap::QUIT, Action::Quit),
    (&keymap::UP, Action::Up),
    (&keymap::DOWN, Action::Down),
    (&keymap::BOTTOM, Action::Bottom),
    (&keymap::PAGE_DOWN, Action::PageDown),
    (&keymap::PAGE_UP, Action::PageUp),
    (&keymap::HALF_DOWN, Action::HalfDown),
    (&keymap::HALF_UP, Action::HalfUp),
    (&keymap::SCROLL_DOWN, Action::ScrollDown),
    (&keymap::SCROLL_UP, Action::ScrollUp),
    (&keymap::HELP, Action::Help),
    (&keymap::REFRESH, Action::Refresh),
    (&keymap::AGENT, Action::Agent),
    (&keymap::NEW, Action::New),
    (&keymap::COPY, Action::Copy),
    (&keymap::TRASH, Action::Trash),
    (&keymap::DELETE, Action::Delete),
    (&keymap::FILTER, Action::Filter),
];

impl Model {
    /// Applies one message and returns the effects it asks for.
    pub fn update(&mut self, message: Message) -> Vec<Command> {
        let commands = self.dispatch(message);
        self.changes_in_flight += commands
            .iter()
            .filter(|command| command.is_change())
            .count();

        // Done here rather than in each navigation branch, so every path that
        // can move the cursor is covered, including the filter's.
        self.sync_detail_scroll();
        self.list.scroll_into_view(self.visible_rows());

        commands
    }

    fn dispatch(&mut self, message: Message) -> Vec<Command> {
        match message {
            Message::Key(event) => self.handle_key(&event),
            Message::Paste(text) => {
                self.handle_paste(&text);
                Vec::new()
            }
            Message::Terminate => {
                self.quit = Quit::Now;
                Vec::new()
            }
            Message::Resize { width, height } => {
                self.width = width;
                self.height = height;
                self.detail_offset = self.detail_offset.min(self.max_detail_offset());
                Vec::new()
            }
            Message::Tick => {
                if self.busy() {
                    self.spinner = self.spinner.wrapping_add(1);
                }
                Vec::new()
            }
            Message::VaultsLoaded { generation, result } => {
                self.handle_vaults_loaded(generation, result)
            }
            Message::KeysLoaded {
                generation,
                vault,
                result,
            } => self.handle_keys_loaded(generation, &vault, result),
            Message::PublicKeyLoaded {
                generation,
                item,
                result,
            } => self.handle_public_key_loaded(generation, &item, result),
            Message::AgentStatusLoaded(result) => self.handle_agent_status(result),
            Message::AgentActionDone(Ok(())) => {
                self.changes_in_flight = self.changes_in_flight.saturating_sub(1);
                // Re-read the status rather than assuming the action took
                // effect.
                vec![Command::LoadAgentStatus]
            }
            Message::AgentActionDone(Err(error)) => {
                self.changes_in_flight = self.changes_in_flight.saturating_sub(1);
                self.agent_busy = false;
                vec![self.notify(error.to_string(), Tone::Error)]
            }
            Message::ItemCreated { title, result } => {
                self.handle_change(result, format!("Created \"{title}\"."))
            }
            Message::ItemRemoved {
                title,
                removal,
                result,
            } => {
                let verb = match removal {
                    Removal::Trash => "Moved to trash",
                    Removal::Delete => "Permanently deleted",
                };
                self.handle_change(result, format!("{verb} \"{title}\"."))
            }
            Message::Copied { title, result } => vec![self.handle_copied(&title, result)],
            Message::StatusExpired { token } => {
                // Ignore a stale timer belonging to a superseded message.
                if token == self.status_token {
                    self.status = None;
                }
                Vec::new()
            }
        }
    }

    fn handle_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        // Ctrl-c quits from anywhere, so no screen can trap the user.
        if keymap::is_interrupt(event) {
            return self.request_quit();
        }

        match self.mode {
            Mode::List => self.handle_list_key(event),
            Mode::Create(_) => self.handle_create_key(event),
            Mode::ConfirmTrash(_) => self.handle_confirm_trash_key(event),
            Mode::ConfirmDelete(_) => self.handle_confirm_delete_key(event),
            Mode::Agent => self.handle_agent_key(event),
        }
    }

    fn handle_list_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        // While a filter is being typed, every key belongs to it.
        if self.list.filter_state() == FilterState::Typing {
            self.pending_g = false;
            self.list.handle_filter_key(event);
            return Vec::new();
        }

        // vim's gg. The first g only arms the prefix; the second completes it,
        // and anything else cancels it and is handled normally.
        if mem::take(&mut self.pending_g) {
            if keymap::TOP.matches(event) {
                self.list.select(0);
                return Vec::new();
            }
        } else if keymap::TOP.matches(event) {
            self.pending_g = true;
            return Vec::new();
        }

        let action = LIST_ACTIONS
            .iter()
            .find(|(binding, _)| binding.matches(event))
            .map(|&(_, action)| action);
        let Some(action) = action else {
            if event.code == KeyCode::Esc && self.list.filter_state() == FilterState::Applied {
                self.list.clear_filter();
            }
            return Vec::new();
        };

        let page = self.visible_rows();
        let half = (page / 2).max(1);

        match action {
            Action::Quit => return self.request_quit(),
            Action::Up => self.list.move_up(1),
            Action::Down => self.list.move_down(1),
            Action::Bottom => self.list.select(usize::MAX),
            Action::PageDown => self.list.move_down(page),
            Action::PageUp => self.list.move_up(page),
            Action::HalfDown => self.list.move_down(half),
            Action::HalfUp => self.list.move_up(half),
            Action::ScrollDown => {
                if self.has_detail_pane() {
                    self.detail_offset = (self.detail_offset + 1).min(self.max_detail_offset());
                }
            }
            Action::ScrollUp => self.detail_offset = self.detail_offset.saturating_sub(1),
            Action::Help => self.full_help = !self.full_help,
            Action::Refresh => return vec![self.reload()],
            Action::Agent => {
                self.mode = Mode::Agent;
                self.agent_busy = true;
                return vec![Command::LoadAgentStatus];
            }
            Action::New => {
                if self.vaults.is_empty() {
                    return vec![self.notify("No vaults available yet.".to_owned(), Tone::Warn)];
                }
                let selected = self.list.selected().map(|key| &key.share_id);
                self.mode = Mode::Create(CreateForm::new(self.vaults.clone(), selected));
            }
            Action::Copy => return self.copy_selected(),
            Action::Trash => {
                if let Some(key) = self.list.selected() {
                    self.mode = Mode::ConfirmTrash(key.clone());
                }
            }
            Action::Delete => {
                if let Some(key) = self.list.selected() {
                    self.confirm = model::confirm_input(key.title.chars().count());
                    self.mode = Mode::ConfirmDelete(key.clone());
                }
            }
            Action::Filter => self.list.start_filtering(),
        }

        Vec::new()
    }

    fn copy_selected(&mut self) -> Vec<Command> {
        let Some(key) = self.list.selected() else {
            return Vec::new();
        };
        if key.public_key.is_empty() {
            return vec![self.notify(
                "No public key loaded for this item yet.".to_owned(),
                Tone::Warn,
            )];
        }

        vec![Command::Copy {
            title: key.title.clone(),
            public_key: key.public_key.clone(),
        }]
    }

    fn handle_create_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        let Mode::Create(form) = &mut self.mode else {
            return Vec::new();
        };
        if event.code == KeyCode::Esc {
            self.mode = Mode::List;
            return Vec::new();
        }
        let Some(request) = form.handle_key(event) else {
            return Vec::new();
        };

        // Drop the whole form, not just the passphrase field: from here the
        // command owns the secret and the model keeps no copy.
        self.mode = Mode::List;
        let notice = self.notify(format!("Creating \"{}\"…", request.title), Tone::Muted);

        vec![Command::Generate(request), notice]
    }

    fn handle_confirm_trash_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        let bare = !event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        match event.code {
            KeyCode::Char('y' | 'Y') if bare => {
                if let Mode::ConfirmTrash(key) = mem::replace(&mut self.mode, Mode::List) {
                    return vec![removal_of(Removal::Trash, key)];
                }
            }
            KeyCode::Char('n' | 'N' | 'q' | 'Q') if bare => self.mode = Mode::List,
            KeyCode::Esc => self.mode = Mode::List,
            _ => {}
        }

        Vec::new()
    }

    /// Guards the permanent delete. Upstream offers no undo for it, so the
    /// exact title must be typed rather than a single key pressed.
    fn handle_confirm_delete_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        match event.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                self.confirm.reset();
            }
            KeyCode::Enter => {
                let Mode::ConfirmDelete(key) = &self.mode else {
                    return Vec::new();
                };
                if self.confirm.value() != key.title {
                    let warning = "Title does not match; nothing was deleted.".to_owned();
                    return vec![self.notify(warning, Tone::Warn)];
                }

                self.confirm.reset();
                if let Mode::ConfirmDelete(key) = mem::replace(&mut self.mode, Mode::List) {
                    return vec![removal_of(Removal::Delete, key)];
                }
            }
            _ => {
                self.confirm.handle_key(event);
            }
        }

        Vec::new()
    }

    fn handle_agent_key(&mut self, event: &KeyEvent) -> Vec<Command> {
        if event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Vec::new();
        }

        match event.code {
            KeyCode::Esc | KeyCode::Char('q' | 'a') => self.mode = Mode::List,
            KeyCode::Char('s') => {
                if self.agent.state == AgentState::Running {
                    return vec![
                        self.notify("The agent is already running.".to_owned(), Tone::Warn),
                    ];
                }
                self.agent_busy = true;
                return vec![Command::StartAgent];
            }
            KeyCode::Char('x') => {
                if self.agent.state == AgentState::Stopped {
                    return vec![self.notify("The agent is not running.".to_owned(), Tone::Warn)];
                }
                self.agent_busy = true;
                return vec![Command::StopAgent];
            }
            KeyCode::Char('r') => {
                self.agent_busy = true;
                return vec![Command::LoadAgentStatus];
            }
            _ => {}
        }

        Vec::new()
    }

    fn handle_vaults_loaded(
        &mut self,
        generation: u64,
        result: Result<Vec<Vault>, CommandError>,
    ) -> Vec<Command> {
        if generation != self.generation {
            return Vec::new();
        }

        let vaults = match result {
            Ok(vaults) => vaults,
            Err(error) => {
                self.loading = false;
                self.error = Some(error.to_string());
                return Vec::new();
            }
        };

        self.error = None;
        self.vaults = vaults;
        if self.vaults.is_empty() {
            self.loading = false;
            return Vec::new();
        }

        // pass-cli cannot list across vaults, so fan out and let each report
        // independently.
        self.vaults_loading = self.vaults.len();
        self.vaults
            .iter()
            .map(|vault| Command::LoadKeys {
                generation,
                vault: vault.clone(),
            })
            .collect()
    }

    fn handle_keys_loaded(
        &mut self,
        generation: u64,
        vault: &Vault,
        result: Result<Vec<Key>, CommandError>,
    ) -> Vec<Command> {
        if generation != self.generation {
            return Vec::new();
        }

        self.vaults_loading = self.vaults_loading.saturating_sub(1);
        if self.vaults_loading == 0 {
            self.loading = false;
        }

        match result {
            // One unreachable vault must not blank the list, so this is
            // surfaced without discarding what the others returned.
            Err(error) => {
                vec![self.notify(format!("Vault \"{}\": {error}", vault.name), Tone::Error)]
            }
            Ok(keys) => {
                // Algorithm and fingerprint need the public key, which the list
                // output does not carry; queue one fetch per item.
                self.pending.extend(keys.iter().map(Key::item_ref));
                self.list.extend(keys);
                self.next_public_key_fetches()
            }
        }
    }

    fn handle_public_key_loaded(
        &mut self,
        generation: u64,
        item: &ItemRef,
        result: Result<String, CommandError>,
    ) -> Vec<Command> {
        if generation != self.generation {
            return Vec::new();
        }

        self.inflight = self.inflight.saturating_sub(1);
        if let Ok(public_key) = result {
            self.list.update(item, |key| {
                // A malformed key still leaves a usable row with unknown
                // metadata, so the error is deliberately not escalated.
                let _ = key.set_public_key(&public_key);
            });
        }

        self.next_public_key_fetches()
    }

    fn handle_agent_status(&mut self, result: Result<AgentStatus, CommandError>) -> Vec<Command> {
        self.agent_busy = false;

        match result {
            Ok(status) => {
                self.agent = status;
                Vec::new()
            }
            Err(error) => vec![self.notify(error.to_string(), Tone::Error)],
        }
    }

    /// Routes pasted text to whichever input has focus.
    fn handle_paste(&mut self, text: &str) {
        match &mut self.mode {
            Mode::Create(form) => form.paste(text),
            Mode::ConfirmDelete(_) => {
                self.confirm.paste(text);
            }
            Mode::List if self.list.filter_state() == FilterState::Typing => {
                self.list.paste_into_filter(text);
            }
            // Pasted text is never a command. Anywhere else it is dropped, so
            // a paste cannot trash a key one letter at a time.
            _ => {}
        }
    }

    /// Reports a create or a removal, and reloads after a successful one.
    fn handle_change(&mut self, result: Result<(), CommandError>, success: String) -> Vec<Command> {
        self.changes_in_flight = self.changes_in_flight.saturating_sub(1);
        match result {
            Ok(()) => vec![self.notify(success, Tone::Ok), self.reload()],
            Err(error) => vec![self.notify(error.to_string(), Tone::Error)],
        }
    }

    fn handle_copied(&mut self, title: &str, result: Result<(), clipboard::Error>) -> Command {
        match result {
            Ok(()) => self.notify(format!("Copied public key for \"{title}\"."), Tone::Ok),
            Err(error) => self.notify(format!("Clipboard unavailable: {error}"), Tone::Error),
        }
    }

    /// Returns the detail pane to the top when the cursor lands on a different
    /// key, so a scrolled position never carries over to another item.
    fn sync_detail_scroll(&mut self) {
        let unchanged = match (self.list.selected(), &self.detail_for) {
            (Some(key), Some(item)) => key.is(item),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.detail_for = self.list.selected().map(Key::item_ref);
            self.detail_offset = 0;
        }
    }

    /// How far the detail pane can scroll before its last line is in view.
    /// Zero when everything already fits.
    #[must_use]
    pub(super) fn max_detail_offset(&self) -> usize {
        self.detail_lines().len().saturating_sub(self.body_height())
    }
}

fn removal_of(removal: Removal, key: Key) -> Command {
    Command::Remove {
        removal,
        item: key.item_ref(),
        title: key.title,
    }
}
