//! What flows into the update loop, and what it asks to have done.
//!
//! The update loop performs no I/O. It returns [`Command`]s, which the runtime
//! executes off the loop and reports back as [`Message`]s. Commands are plain
//! data, so tests assert exactly which effect a key press asked for without
//! anything running.

use crossterm::event::KeyEvent;

use crate::clipboard;
use crate::keys::{ItemRef, Key, Vault};
use crate::passcli::{AgentStatus, CommandError, GenerateRequest};

/// Which of the two removals a confirmation led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removal {
    /// Recoverable with `pass-cli item untrash`.
    Trash,
    /// Permanent. There is no undo.
    Delete,
}

/// An input to the update loop.
///
/// Results carry their own error, so a failure is handled where its result
/// would have been. `generation` names the load a result belongs to; results
/// from a load that a refresh has since superseded are dropped.
#[derive(Debug)]
pub enum Message {
    Key(KeyEvent),
    /// Text pasted into the terminal, delivered whole rather than as key
    /// presses so that it can never trigger an action.
    Paste(String),
    /// The process was asked to terminate.
    Terminate,
    Resize {
        width: u16,
        height: u16,
    },
    /// Advances the spinner while work is outstanding.
    Tick,
    VaultsLoaded {
        generation: u64,
        result: Result<Vec<Vault>, CommandError>,
    },
    KeysLoaded {
        generation: u64,
        vault: Vault,
        result: Result<Vec<Key>, CommandError>,
    },
    /// One item's public key. An error is not fatal: the row stays listed with
    /// unknown metadata.
    PublicKeyLoaded {
        generation: u64,
        item: ItemRef,
        result: Result<String, CommandError>,
    },
    AgentStatusLoaded(Result<AgentStatus, CommandError>),
    /// A start or stop finished. The status is re-read afterwards rather than
    /// assuming the action took effect.
    AgentActionDone(Result<(), CommandError>),
    ItemCreated {
        title: String,
        result: Result<(), CommandError>,
    },
    ItemRemoved {
        title: String,
        removal: Removal,
        result: Result<(), CommandError>,
    },
    Copied {
        title: String,
        result: Result<(), clipboard::Error>,
    },
    /// Clears the status line, unless a newer one has replaced it.
    StatusExpired {
        token: u64,
    },
}

/// An effect for the runtime to perform.
#[derive(Debug)]
pub enum Command {
    /// The entry point for everything else: pass-cli cannot list items
    /// without a vault selector.
    LoadVaults {
        generation: u64,
    },
    LoadKeys {
        generation: u64,
        vault: Vault,
    },
    LoadPublicKey {
        generation: u64,
        item: ItemRef,
    },
    LoadAgentStatus,
    StartAgent,
    StopAgent,
    /// The request owns the only copy of any passphrase, and is zeroed once
    /// the client is done with it.
    Generate(GenerateRequest),
    Remove {
        removal: Removal,
        item: ItemRef,
        title: String,
    },
    /// Only public material is ever copied; protui has no path that reads a
    /// private key.
    Copy {
        title: String,
        public_key: String,
    },
    ExpireStatus {
        token: u64,
    },
}

impl Command {
    /// Reports whether the command changes something in Proton Pass or the
    /// agent, as opposed to reading. Quitting waits for these: killing
    /// pass-cli halfway through one would leave its outcome unknown.
    #[must_use]
    pub fn is_change(&self) -> bool {
        matches!(
            self,
            Self::Generate(_) | Self::Remove { .. } | Self::StartAgent | Self::StopAgent
        )
    }
}
