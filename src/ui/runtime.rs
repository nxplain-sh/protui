//! The terminal event loop, and the effects it runs off the loop.

use std::io;
use std::panic;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyEventKind,
};
use ratatui::DefaultTerminal;
use tokio::signal::unix::{self, SignalKind};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};

use crate::clipboard;
use crate::keys::Vault;
use crate::passcli::{Client, CommandError};
use crate::ui::message::{Command, Message, Removal};
use crate::ui::model::Model;
use crate::ui::theme::Theme;

/// The vault list fetched alongside the session check.
pub type VaultsInFlight = JoinHandle<Result<Vec<Vault>, CommandError>>;

/// Twelve frames a second: smooth, without redrawing more than the eye
/// notices.
const SPINNER_INTERVAL: Duration = Duration::from_millis(83);

/// How long a status message stays up.
const STATUS_LIFETIME: Duration = Duration::from_secs(4);

/// Takes over the terminal and runs the UI until the user quits.
///
/// The terminal is restored on every exit path, including a panic, which the
/// hook ratatui installs handles.
///
/// `vaults` is the vault list, already on its way: it is fetched alongside the
/// session check, before the terminal is taken over.
pub async fn run(client: Arc<Client>, vaults: VaultsInFlight) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    disable_paste_on_panic();

    // Keys typed while pass-cli was starting up are waiting by now. Asking the
    // terminal for its colours reads its reply from the same input and would
    // discard them, so a waiting key wins and the palette assumes dark.
    // Polling queues the keys for the event stream rather than consuming them.
    let theme = if event::poll(Duration::ZERO)? {
        Theme::detect_without_asking()
    } else {
        Theme::detect()
    };

    // Bracketed paste delivers a paste as one event. Without it each pasted
    // character arrives as a key press, and pasting "daily" would open the
    // trash prompt and confirm it.
    let result = match crossterm::execute!(io::stdout(), EnableBracketedPaste) {
        Ok(()) => event_loop(&mut terminal, client, vaults, theme).await,
        Err(error) => Err(error),
    };

    let paste_off = crossterm::execute!(io::stdout(), DisableBracketedPaste);
    let restored = ratatui::try_restore();

    result.and(paste_off).and(restored)
}

/// Extends ratatui's panic hook, which restores the terminal, to switch
/// bracketed paste back off as well.
fn disable_paste_on_panic() {
    let restore = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(io::stdout(), DisableBracketedPaste);
        restore(info);
    }));
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    client: Arc<Client>,
    vaults: VaultsInFlight,
    theme: Theme,
) -> io::Result<()> {
    use futures_util::StreamExt;

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let mut events = EventStream::new();
    let mut terminate = unix::signal(SignalKind::terminate())?;
    let mut spinner = time::interval(SPINNER_INTERVAL);
    spinner.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut model = Model::new(theme);

    // The first vault list joins the loop like any other result. A refresh
    // pressed before it lands moves the generation on, and it is dropped.
    let first_load = sender.clone();
    let generation = model.generation;
    tokio::spawn(async move {
        if let Ok(result) = vaults.await {
            let _ = first_load.send(Message::VaultsLoaded { generation, result });
        }
    });

    let size = terminal.size()?;
    let mut commands = Model::init();
    commands.extend(model.update(Message::Resize {
        width: size.width,
        height: size.height,
    }));

    loop {
        for command in commands.drain(..) {
            spawn(command, &client, &sender);
        }
        if model.is_quitting() {
            return Ok(());
        }

        terminal.draw(|frame| model.render(frame))?;

        let message = tokio::select! {
            event = events.next() => match event {
                Some(Ok(event)) => match message_for(event) {
                    Some(message) => message,
                    None => continue,
                },
                Some(Err(error)) => return Err(error),
                // The terminal's input closed; there is nobody left to drive the UI.
                None => return Ok(()),
            },
            Some(message) = receiver.recv() => message,
            // A polite kill still restores the terminal on the way out.
            _ = terminate.recv() => Message::Terminate,
            _ = spinner.tick(), if model.busy() => Message::Tick,
        };

        commands = model.update(message);
    }
}

/// Translates a terminal event into a message, if the model cares about it.
fn message_for(event: Event) -> Option<Message> {
    match event {
        // Some terminals report releases too; acting on them would double
        // every key press.
        Event::Key(key) if key.kind != KeyEventKind::Release => Some(Message::Key(key)),
        Event::Paste(text) => Some(Message::Paste(text)),
        Event::Resize(width, height) => Some(Message::Resize { width, height }),
        _ => None,
    }
}

/// Runs a command on its own task and posts the result back to the loop.
fn spawn(command: Command, client: &Arc<Client>, sender: &mpsc::UnboundedSender<Message>) {
    let client = Arc::clone(client);
    let sender = sender.clone();

    tokio::spawn(async move {
        let message = perform(command, &client).await;
        // The receiver only closes once the loop has exited, and then nobody
        // needs the result.
        let _ = sender.send(message);
    });
}

async fn perform(command: Command, client: &Client) -> Message {
    match command {
        Command::LoadVaults { generation } => Message::VaultsLoaded {
            generation,
            result: client.vaults().await,
        },
        Command::LoadKeys { generation, vault } => {
            let result = client.ssh_keys(&vault).await;
            Message::KeysLoaded {
                generation,
                vault,
                result,
            }
        }
        Command::LoadPublicKey { generation, item } => {
            let result = client.public_key(&item.share_id, &item.item_id).await;
            Message::PublicKeyLoaded {
                generation,
                item,
                result,
            }
        }
        Command::LoadAgentStatus => Message::AgentStatusLoaded(client.agent_status().await),
        Command::StartAgent => Message::AgentActionDone(client.start_agent().await),
        Command::StopAgent => Message::AgentActionDone(client.stop_agent().await),
        Command::Generate(request) => {
            let title = request.title.clone();
            let result = client.generate(request).await.map(drop);
            Message::ItemCreated { title, result }
        }
        Command::Remove {
            removal,
            item,
            title,
        } => {
            let result = match removal {
                Removal::Trash => client.trash(&item.share_id, &item.item_id).await,
                Removal::Delete => client.delete(&item.share_id, &item.item_id).await,
            };
            Message::ItemRemoved {
                title,
                removal,
                result,
            }
        }
        Command::Copy { title, public_key } => Message::Copied {
            title,
            result: clipboard::copy(&public_key).await,
        },
        Command::ExpireStatus { token } => {
            time::sleep(STATUS_LIFETIME).await;
            Message::StatusExpired { token }
        }
    }
}
