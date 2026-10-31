//! Behaviour and render tests for the UI.
//!
//! These drive the model with synthetic messages and render it into an
//! in-memory buffer, so every screen is exercised without a terminal or a
//! pass-cli binary. Commands returned by `update` are plain values that are
//! never executed here, so nothing dials out.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};

use crate::keys::{ItemId, ItemRef, Key, ShareId, State, Vault};
use crate::passcli::{AgentState, AgentStatus, CommandError};
use crate::ui::create_form::CreateForm;
use crate::ui::key_list::FilterState;
use crate::ui::message::{Command, Message, Removal};
use crate::ui::model::{FOOTER_ROWS, Mode, Model, Tone};
use crate::ui::theme::{Background, ColorDepth, GLYPH_LOWER_HALF, GLYPH_UPPER_HALF, Theme};

const TEST_PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM+XgkeZS/lofu0u1xq0g4DQUZIOxGcdSqHhQbjKwVEQ laptop@example";

const PREVIEW_RSA: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDDMU3w/a9F5JytBm0CPnKCZ9XlyVxG7d3afM0dJg1+4yZp2dWifkADPAxWAxaLnY7gcJzyCDIC3j5klN1deFpspph4AX09detMoaunhNa/xpxFmWRHin36F+6EQWAKtshvBCoenF8PCcFYrnGsatPPxI2Dbyl0QNacQgfiG4C0YLS1/Ajx/JyY8MwoVqM6hIBmE/hVdAzH+EwVe7R7jLcLNAW63j71At/OYyWOCMeOkPN3JPzRHxadMl666XI/ML0tzmv2akTi3bohkQUtXtFS7rFmVhZF78kYACs4gWoyxNCPpHFh3omYLvGL5xk0qhCgs0O99d6vmNZ2BDb1MNgf deploy@ci";

// Throwaway keys, generated for these tests; only their public halves exist.
const CI_PUBLIC_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKYfN8FVoqYkl6+g63EchcFyQdtwRy7eeNd7oTeKCZoJ ci@deploy";
const LAPTOP_PUBLIC_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZ4+polYKbuiybA7H8XEH7wRapg93CEvEWIN77KJ0ap me@laptop";
const RSA4096_PUBLIC_KEY: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAACAQCsRQPBhN48muNNy971dmqmrTeL4PIo3ivnerKyUMvVtZx9g9bNbcrwjCly6vj2dHVN36TiFUwZtzmPFoTvEU+EubG1e/LpMyLEvm1crPh/0v5+CchZ+ebH7/edL1hm71k6/IjxaeNx3cYb0bRTseyNCRNqwMWOimgcPPoiinVNlWGscpVM0qWebm/4xXyH4znrK/dQocnUJ8LXkdKtJ3j4I2c42xINEYx5UdYEJuPI19mozE0T3Ot9GsM+68hqRzCoKz30CmS0bLmLuXY1nYnGfT8EZbzPaX2Rmrv16reULBuElAtCPtCXj6kabWkfq/idknowgLbt6ZK1zMLaHx7tFhiXZ1QcufXlY3iTAZTgDl0vTXot915V2jbAZbZnyRtZxh6aU2o/orjtBBXaCQI42awIq4On4628pFVIqZZoKlXLHnV5GkIEzvmUdyaJ/1kKHmlifRf4Oyr+r2Bbn5i4lUtY3IZ+p7f+gBP/sINBqJh4A70dnRiLvPBbXpgOqWhCdF34C3X6A1ASrdvIXSgyw9QmhpdHiyuqMrNeFi8sTGEhgTSOVGbJooXKb7XJsN36rNQRZ4q4RtfLTf54Qe5KVLF5Lo50WMzKycmgonh1bB9u7nghPcpMdVR4eRV1r8U2EmTYRxaljx57+8UriXIZ7xVvfhKzbWGDyWlvjcP7Xw== root@hetzner";

fn vault(name: &str, share_id: &str) -> Vault {
    Vault {
        name: name.to_owned(),
        share_id: ShareId::from(share_id),
        ..Vault::default()
    }
}

fn test_vaults() -> Vec<Vault> {
    vec![vault("Personal", "share-1"), vault("Work", "share-2")]
}

fn key(share_id: &str, vault_name: &str, id: &str, title: &str) -> Key {
    Key {
        id: ItemId::from(id),
        share_id: ShareId::from(share_id),
        vault_name: vault_name.to_owned(),
        title: title.to_owned(),
        created_at: "2026-01-23T19:48:23".parse().ok(),
        ..Key::default()
    }
}

fn test_keys() -> Vec<Key> {
    vec![key("share-1", "Personal", "item-1", "laptop")]
}

/// A model in the state it reaches after a successful startup.
fn loaded(width: u16, height: u16) -> Model {
    let mut model = Model::new(Theme::default());
    let vaults = test_vaults();

    model.update(Message::Resize { width, height });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vaults.clone()),
    });
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: vaults[0].clone(),
        result: Ok(test_keys()),
    });
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: vaults[1].clone(),
        result: Ok(Vec::new()),
    });
    model
}

/// A loaded model holding `count` keys, tall enough to paginate.
fn list_with(count: usize) -> Model {
    let personal = vault("Personal", "s1");
    // Titles sort in insertion order, so index maps to position.
    let keys = (0..count)
        .map(|i| {
            key(
                "s1",
                "Personal",
                &format!("item-{i:02}"),
                &format!("key-{i:02}"),
            )
        })
        .collect();

    let mut model = Model::new(Theme::default());
    model.update(Message::Resize {
        width: 100,
        height: 30,
    });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vec![personal.clone()]),
    });
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: personal,
        result: Ok(keys),
    });
    model
}

fn item(share_id: &str, item_id: &str) -> ItemRef {
    ItemRef {
        share_id: ShareId::from(share_id),
        item_id: ItemId::from(item_id),
    }
}

fn deliver_public_key(model: &mut Model, share_id: &str, item_id: &str, public_key: &str) {
    model.update(Message::PublicKeyLoaded {
        generation: model.generation,
        item: item(share_id, item_id),
        result: Ok(public_key.to_owned()),
    });
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Presses each key in turn and returns the commands from the last one.
fn press(model: &mut Model, keys: &[KeyEvent]) -> Vec<Command> {
    let mut commands = Vec::new();
    for &event in keys {
        commands = model.update(Message::Key(event));
    }
    commands
}

fn render_buffer(model: &Model) -> Buffer {
    // Before the first resize the model has no size; give it a screen anyway.
    let (width, height) = if model.width == 0 {
        (20, 2)
    } else {
        (model.width, model.height.max(1))
    };
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal.draw(|frame| model.render(frame)).expect("render");
    terminal.backend().buffer().clone()
}

/// Renders the model and returns the screen as text, one line per row.
fn render(model: &Model) -> String {
    let buffer = render_buffer(model);
    let area = buffer.area;

    (0..area.height)
        .map(|y| {
            let row: String = (0..area.width).map(|x| buffer[(x, y)].symbol()).collect();
            row.trim_end().to_owned()
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[test]
fn vim_navigation() {
    const COUNT: usize = 30;

    let cases: [(&str, usize, Vec<KeyEvent>, usize); 6] = [
        ("j moves down", 0, vec![char_key('j')], 1),
        ("k moves up", 5, vec![char_key('k')], 4),
        // vim's gg needs both presses.
        (
            "gg jumps to the top",
            20,
            vec![char_key('g'), char_key('g')],
            0,
        ),
        // A bare g is a prefix in vim and must not act on its own.
        ("a single g does nothing", 20, vec![char_key('g')], 20),
        // An unrelated key cancels the prefix rather than being swallowed.
        (
            "g then j cancels the prefix",
            20,
            vec![char_key('g'), char_key('j')],
            21,
        ),
        (
            "G jumps to the bottom",
            3,
            vec![KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)],
            COUNT - 1,
        ),
    ];

    for (name, start, keys, want) in cases {
        let mut model = list_with(COUNT);
        model.list.select(start);

        press(&mut model, &keys);

        assert_eq!(model.list.cursor(), want, "{name}");
    }
}

/// The step is derived from the model rather than hardcoded: it depends on
/// the row height, and pinning a literal here would only assert that two
/// constants still multiply the same way.
#[test]
fn vim_paging() {
    let page = list_with(40).visible_rows();
    let half = (page / 2).max(1);
    assert!(page >= 2, "the test needs a list taller than {page} rows");

    let cases = [
        ("ctrl+f pages down", 0, ctrl('f'), page),
        ("ctrl+b pages up", 30, ctrl('b'), 30 - page),
        ("ctrl+d half pages down", 0, ctrl('d'), half),
        ("ctrl+u half pages up", 30, ctrl('u'), 30 - half),
    ];

    for (name, start, event, want) in cases {
        let mut model = list_with(40);
        model.list.select(start);

        press(&mut model, &[event]);

        assert_eq!(model.list.cursor(), want, "{name} (page {page})");
    }
}

/// Paging past either end must clamp rather than wrap or run off the list.
#[test]
fn paging_stops_at_the_ends() {
    let mut top = list_with(5);
    press(&mut top, &[ctrl('b')]);
    assert_eq!(top.list.cursor(), 0);

    let mut bottom = list_with(5);
    bottom.list.select(4);
    press(&mut bottom, &[ctrl('f')]);
    assert_eq!(bottom.list.cursor(), 4);
}

/// Bare letters do not page: vim pages with Ctrl held, and nothing documents
/// these keys.
#[test]
fn bare_letter_paging_is_unbound() {
    for c in ['f', 'b', 'u', 'h', 'l'] {
        let mut model = list_with(30);
        model.list.select(10);

        press(&mut model, &[char_key(c)]);

        assert_eq!(
            model.list.cursor(),
            10,
            "{c:?} moved the cursor; it should be unbound"
        );
        assert!(matches!(model.mode, Mode::List), "{c:?} changed the mode");
    }
}

/// The mnemonic action keys follow lazygit and k9s rather than vim.
#[test]
fn action_bindings() {
    let mut new = list_with(5);
    press(&mut new, &[char_key('n')]);
    assert!(matches!(new.mode, Mode::Create(_)));

    let mut agent = list_with(5);
    assert!(matches!(
        press(&mut agent, &[char_key('a')])[..],
        [Command::LoadAgentStatus]
    ));
    assert!(matches!(agent.mode, Mode::Agent));

    let mut trash = list_with(5);
    press(&mut trash, &[char_key('d')]);
    assert!(matches!(trash.mode, Mode::ConfirmTrash(_)));

    let mut delete = list_with(5);
    press(
        &mut delete,
        &[KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)],
    );
    assert!(matches!(delete.mode, Mode::ConfirmDelete(_)));
}

/// c is the key the help bar advertises; y is the vim alias kept beside it.
#[test]
fn copy_aliases() {
    for c in ['c', 'y'] {
        let mut model = list_with(5);
        deliver_public_key(&mut model, "s1", "item-00", TEST_PUBLIC_KEY);

        let commands = press(&mut model, &[char_key(c)]);

        assert!(
            matches!(&commands[..], [Command::Copy { public_key, .. }] if public_key == TEST_PUBLIC_KEY),
            "{c:?} gave {commands:?}"
        );
    }
}

#[test]
fn copying_before_the_key_loads_warns_instead() {
    let mut model = list_with(5);

    let commands = press(&mut model, &[char_key('c')]);

    assert!(
        matches!(commands[..], [Command::ExpireStatus { .. }]),
        "{commands:?}"
    );
    assert!(render(&model).contains("No public key loaded"));
}

/// An armed g surviving into the filter would swallow the next keystroke
/// typed.
#[test]
fn the_g_prefix_is_cleared_by_filtering() {
    let mut model = list_with(10);
    model.list.select(5);

    press(&mut model, &[char_key('g')]);
    assert!(model.pending_g, "g did not arm the prefix");

    press(&mut model, &[char_key('/')]);
    assert!(!model.pending_g, "the g prefix survived into the filter");
}

#[test]
fn filtering_narrows_the_list_and_the_header_says_so() {
    let mut model = list_with(12);

    press(&mut model, &[char_key('/'), char_key('1'), char_key('1')]);
    assert_eq!(model.list.filter_state(), FilterState::Typing);
    assert_eq!(
        model.list.selected().map(|key| key.title.as_str()),
        Some("key-11")
    );
    assert!(
        render(&model).contains("1/1 OF 12 KEYS"),
        "{}",
        render(&model)
    );

    // Typing keys are text, not actions.
    assert!(matches!(model.mode, Mode::List));

    press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);
    assert_eq!(model.list.filter_state(), FilterState::Applied);

    press(&mut model, &[KeyEvent::from(KeyCode::Esc)]);
    assert_eq!(model.list.filter_state(), FilterState::Off);
    assert_eq!(model.list.visible_len(), 12);
}

#[test]
fn q_and_ctrl_c_quit_from_the_list() {
    let mut model = list_with(3);
    press(&mut model, &[char_key('q')]);
    assert!(model.is_quitting());

    let mut model = list_with(3);
    press(&mut model, &[ctrl('c')]);
    assert!(model.is_quitting());
}

/// No screen may trap the user: Ctrl-c quits from inside every dialog.
#[test]
fn ctrl_c_quits_from_every_dialog() {
    for opener in [char_key('n'), char_key('a'), char_key('d'), char_key('D')] {
        let mut model = list_with(3);
        press(&mut model, &[opener]);
        assert!(
            !matches!(model.mode, Mode::List),
            "{opener:?} opened nothing"
        );

        press(&mut model, &[ctrl('c')]);

        assert!(model.is_quitting(), "{opener:?} trapped ctrl+c");
    }
}

#[test]
fn view_renders_every_mode() {
    let mut base = loaded(120, 40);
    // A public key arriving must populate the derived metadata shown in both
    // the list row and the detail pane.
    deliver_public_key(&mut base, "share-1", "item-1", TEST_PUBLIC_KEY);
    assert!(render(&base).contains("ED25519"));

    let screens: Vec<(&str, Mode, &[&str])> = vec![
        (
            "list with detail pane",
            Mode::List,
            &["PROTUI", "laptop", "Personal", "ED25519", "FINGERPRINT"],
        ),
        (
            "create form",
            Mode::Create(CreateForm::new(test_vaults(), None)),
            &["NEW SSH KEY", "TITLE", "VAULT", "PASSPHRASE", "ED25519"],
        ),
        (
            "trash confirmation",
            Mode::ConfirmTrash(test_keys().remove(0)),
            &["MOVE TO TRASH", "laptop", "untrash"],
        ),
        (
            "permanent delete confirmation",
            Mode::ConfirmDelete(test_keys().remove(0)),
            &["PERMANENTLY DELETE", "CANNOT BE UNDONE", "TYPE THE TITLE"],
        ),
        (
            "agent panel",
            Mode::Agent,
            &["SSH AGENT DAEMON", "RUNNING", "4321", "SSH_AUTH_SOCK"],
        ),
    ];

    for (name, mode, wanted) in screens {
        base.mode = mode;
        base.agent = AgentStatus {
            state: AgentState::Running,
            detail: "running".to_owned(),
            pid: "4321".to_owned(),
            socket: "/tmp/agent.sock".to_owned(),
        };

        let screen = render(&base);

        for want in wanted {
            assert!(screen.contains(want), "{name}: no {want:?} in\n{screen}");
        }
    }
}

/// A belt-and-braces check on the invariant that only public material
/// reaches the terminal.
#[test]
fn the_view_never_renders_private_key_material() {
    let mut model = loaded(120, 40);
    deliver_public_key(&mut model, "share-1", "item-1", TEST_PUBLIC_KEY);

    let screen = render(&model);

    for forbidden in ["PRIVATE KEY", "private_key", "BEGIN OPENSSH"] {
        assert!(
            !screen.contains(forbidden),
            "the view contains {forbidden:?}"
        );
    }
}

/// The layout arithmetic is where a panic would most plausibly hide.
#[test]
fn the_view_survives_narrow_terminals() {
    for (width, height) in [
        (1, 1),
        (20, 10),
        (40, 12),
        (80, 24),
        (91, 30),
        (92, 30),
        (200, 60),
    ] {
        let mut model = loaded(width, height);
        deliver_public_key(&mut model, "share-1", "item-1", TEST_PUBLIC_KEY);

        for opener in [None, Some('n'), Some('a'), Some('d'), Some('D'), Some('?')] {
            let mut screen = loaded(width, height);
            if let Some(c) = opener {
                press(&mut screen, &[char_key(c)]);
            }
            render(&screen);
        }
        // At a single cell there is nothing legible to draw; only not
        // panicking matters there.
        if width > 1 {
            assert!(
                render(&model).contains("PROTUI"),
                "{width}x{height} lost the header"
            );
        }
    }
}

/// The two states a first run passes through.
#[test]
fn empty_and_loading_states() {
    let mut model = Model::new(Theme::default());
    assert_eq!(render(&model).trim(), "Loading…");

    model.update(Message::Resize {
        width: 100,
        height: 30,
    });
    assert!(render(&model).contains("Loading keys"));

    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(Vec::new()),
    });
    let screen = render(&model);
    assert!(
        screen.contains("NO SSH KEYS YET"),
        "the empty view should offer to create one:\n{screen}"
    );
    assert!(screen.contains("NO KEYS"));
}

#[test]
fn a_failed_vault_listing_is_shown_with_a_retry_hint() {
    let mut model = Model::new(Theme::default());
    model.update(Message::Resize {
        width: 100,
        height: 30,
    });

    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Err(CommandError::with_stderr("vault list", "session expired")),
    });

    let screen = render(&model);
    assert!(
        screen.contains("pass-cli vault list: session expired"),
        "{screen}"
    );
    assert!(screen.contains("RETRY"), "{screen}");
    assert!(!model.busy());
}

/// The fan-out requirement: one unreachable vault must not discard what the
/// others returned.
#[test]
fn a_vault_error_does_not_blank_the_list() {
    let mut model = Model::new(Theme::default());
    let vaults = test_vaults();
    model.update(Message::Resize {
        width: 120,
        height: 40,
    });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vaults.clone()),
    });
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: vaults[0].clone(),
        result: Ok(test_keys()),
    });

    model.update(Message::KeysLoaded {
        generation: 0,
        vault: vaults[1].clone(),
        result: Err(CommandError::with_stderr("item list", "vault unreachable")),
    });

    assert_eq!(
        model.list.len(),
        1,
        "the key from the vault that succeeded is gone"
    );
    let screen = render(&model);
    assert!(screen.contains("laptop"), "{screen}");
    assert!(
        screen.contains("Vault \"Work\""),
        "the failing vault is not named:\n{screen}"
    );
}

/// Results from a load that a refresh has superseded must not leak into the
/// new list, or a quick double refresh would list every key twice.
#[test]
fn results_from_a_superseded_load_are_dropped() {
    let mut model = loaded(120, 40);
    let stale = model.generation;

    let commands = press(&mut model, &[char_key('r')]);
    assert!(matches!(commands[..], [Command::LoadVaults { generation }] if generation != stale));

    model.update(Message::KeysLoaded {
        generation: stale,
        vault: test_vaults()[0].clone(),
        result: Ok(test_keys()),
    });
    deliver_public_key(&mut model, "share-1", "item-1", TEST_PUBLIC_KEY);
    model.update(Message::PublicKeyLoaded {
        generation: stale,
        item: item("share-1", "item-1"),
        result: Ok(TEST_PUBLIC_KEY.to_owned()),
    });

    assert!(model.list.is_empty());
    assert_eq!(model.inflight, 0);
}

/// Deriving metadata costs one process per key, so the fetches drain through
/// a fixed window rather than all at once.
#[test]
fn public_key_fetches_are_bounded() {
    let commands = {
        let mut model = Model::new(Theme::default());
        let personal = vault("Personal", "s1");
        model.update(Message::VaultsLoaded {
            generation: 0,
            result: Ok(vec![personal.clone()]),
        });
        let keys = (0..10)
            .map(|i| key("s1", "Personal", &format!("i{i}"), &format!("k{i}")))
            .collect();
        model.update(Message::KeysLoaded {
            generation: 0,
            vault: personal,
            result: Ok(keys),
        })
    };

    let fetches = commands
        .iter()
        .filter(|command| matches!(command, Command::LoadPublicKey { .. }));
    assert_eq!(fetches.count(), crate::ui::model::MAX_PUBLIC_KEY_FETCHES);
}

/// The guard on the irreversible action.
#[test]
fn delete_requires_the_exact_title() {
    let mut model = loaded(120, 40);
    model.mode = Mode::ConfirmDelete(test_keys().remove(0));

    // A near miss must not delete.
    model.confirm.set_value("laptopp");
    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);
    assert!(
        matches!(model.mode, Mode::ConfirmDelete(_)),
        "a mismatch should keep the prompt open"
    );
    assert!(
        matches!(commands[..], [Command::ExpireStatus { .. }]),
        "expected a warning: {commands:?}"
    );

    // The exact title proceeds, against the right item.
    model.confirm.set_value("laptop");
    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);
    assert!(
        matches!(model.mode, Mode::List),
        "a match should close the prompt"
    );
    assert!(
        matches!(
            &commands[..],
            [Command::Remove { removal: Removal::Delete, item: target, .. }]
                if *target == item("share-1", "item-1")
        ),
        "{commands:?}"
    );
}

#[test]
fn trash_confirms_with_y_and_cancels_with_anything_else() {
    let mut model = loaded(120, 40);

    let commands = press(&mut model, &[char_key('d'), char_key('n')]);
    assert!(commands.is_empty() && matches!(model.mode, Mode::List));

    let commands = press(&mut model, &[char_key('d'), char_key('Y')]);
    assert!(
        matches!(
            &commands[..],
            [Command::Remove {
                removal: Removal::Trash,
                ..
            }]
        ),
        "{commands:?}"
    );
}

/// The form hands its passphrase over and is dropped, so the model keeps no
/// copy of the secret.
#[test]
fn creating_a_key_hands_the_request_over() {
    let mut model = loaded(120, 40);
    press(&mut model, &[char_key('n')]);
    for c in "deploy".chars() {
        press(&mut model, &[char_key(c)]);
    }

    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);

    assert!(matches!(model.mode, Mode::List));
    assert!(
        matches!(&commands[..], [Command::Generate(request), Command::ExpireStatus { .. }]
            if request.title == "deploy" && request.share_id == ShareId::from("share-1")),
        "{commands:?}"
    );
}

#[test]
fn the_agent_panel_refuses_redundant_actions() {
    let mut model = loaded(120, 40);
    press(&mut model, &[char_key('a')]);
    model.update(Message::AgentStatusLoaded(Ok(AgentStatus {
        state: AgentState::Running,
        ..AgentStatus::default()
    })));

    let commands = press(&mut model, &[char_key('s')]);
    assert!(
        matches!(commands[..], [Command::ExpireStatus { .. }]),
        "{commands:?}"
    );

    let commands = press(&mut model, &[char_key('x')]);
    assert!(matches!(commands[..], [Command::StopAgent]), "{commands:?}");

    // A completed action re-reads the status rather than assuming it.
    let commands = model.update(Message::AgentActionDone(Ok(())));
    assert!(
        matches!(commands[..], [Command::LoadAgentStatus]),
        "{commands:?}"
    );
}

#[test]
fn an_expired_status_only_clears_its_own_message() {
    let mut model = loaded(120, 40);
    model.update(Message::Copied {
        title: "laptop".to_owned(),
        result: Ok(()),
    });
    let first = model.status_token;
    model.update(Message::Copied {
        title: "laptop".to_owned(),
        result: Ok(()),
    });

    model.update(Message::StatusExpired { token: first });
    assert!(
        model.status.is_some(),
        "an old timer cleared a newer message"
    );

    model.update(Message::StatusExpired {
        token: model.status_token,
    });
    assert!(model.status.is_none());
}

/// The pane's main job. A fingerprint with its middle elided cannot be
/// compared against `ssh-keygen -lf` or a forge's UI, which is the reason to
/// display one at all.
#[test]
fn the_fingerprint_is_never_truncated() {
    for width in [92, 100, 118, 160] {
        let mut model = loaded(width, 40);
        deliver_public_key(&mut model, "share-1", "item-1", TEST_PUBLIC_KEY);
        let fingerprint = model
            .list
            .selected()
            .map(|key| key.fingerprint.clone())
            .unwrap_or_default();
        assert!(!fingerprint.is_empty(), "the fingerprint was not derived");

        // The value wraps across lines, so compare with all whitespace
        // removed. A fingerprint contains none of its own.
        let flattened: String = model
            .detail_lines()
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
            .collect::<String>()
            .split_whitespace()
            .collect();
        assert!(
            flattened.contains(&fingerprint),
            "at width {width} {fingerprint} is not shown in full"
        );
    }
}

/// The only cue that a list continues past the bottom of the screen.
#[test]
fn the_header_shows_the_position() {
    let mut model = list_with(40);
    assert!(render(&model).contains("1/40 KEYS"));

    press(&mut model, &[ctrl('f')]);

    let screen = render(&model);
    assert!(
        !screen.contains("1/40 KEYS") && screen.contains("/40 KEYS"),
        "{screen}"
    );
}

/// Left unclamped, the terminal cuts content taller than the window from the
/// bottom, silently.
#[test]
fn the_detail_never_overflows_its_pane() {
    for height in [12, 16, 20, 30, 60] {
        let mut model = loaded(118, height);
        deliver_public_key(&mut model, "share-1", "item-1", TEST_PUBLIC_KEY);

        assert!(
            model.detail_window().len() <= model.detail_body_height(),
            "at height {height} the detail pane overflows"
        );
    }
}

/// The clamps at both ends, and the reset that stops one key's scroll
/// position leaking onto the next.
#[test]
fn detail_scrolling() {
    let mut model = list_with(3);
    model.update(Message::Resize {
        width: 118,
        height: 14,
    });
    deliver_public_key(&mut model, "s1", "item-00", TEST_PUBLIC_KEY);

    let limit = model.max_detail_offset();
    assert!(limit > 0, "the test needs content taller than the pane");

    press(&mut model, &[ctrl('y')]);
    assert_eq!(model.detail_offset, 0, "scrolling up from the top moved");

    press(&mut model, &vec![ctrl('e'); limit + 50]);
    assert_eq!(model.detail_offset, limit, "scrolling ran past the end");
    assert!(
        render(&model).contains('↑'),
        "no marker for content scrolled off the top"
    );

    // Moving to another key must start its detail at the top.
    press(&mut model, &[char_key('j')]);
    assert_eq!(model.list.cursor(), 1);
    assert_eq!(
        model.detail_offset, 0,
        "the offset carried over to another key"
    );
}

/// The injection path this module owns: a public key's trailing comment is
/// arbitrary bytes from whoever wrote the key, and `set_public_key` sanitises
/// it on the way in. Titles and vault names are sanitised where they enter,
/// in `passcli`.
#[test]
fn the_view_never_emits_escapes_from_key_material() {
    let payloads = [
        ("clear screen", "\u{1b}[2J"),
        ("cursor home", "\u{1b}[H"),
        ("window title", "\u{1b}]0;pwned\u{7}"),
        ("clipboard OSC 52", "\u{1b}]52;c;cHduZWQ=\u{7}"),
        ("bidi override", "\u{202e}reversed\u{202c}"),
        ("bell", "\u{7}"),
    ];

    for (name, payload) in payloads {
        let mut model = loaded(120, 40);
        deliver_public_key(
            &mut model,
            "share-1",
            "item-1",
            &format!("{TEST_PUBLIC_KEY}{payload}"),
        );

        let screen = render(&model);

        // The key itself must still render; only the instruction goes.
        assert!(
            screen.contains("laptop"),
            "{name}: sanitising ate the surrounding text"
        );
        for forbidden in ['\u{1b}', '\u{7}', '\u{202e}'] {
            assert!(
                !screen.contains(forbidden),
                "{name}: the view emits {forbidden:?}"
            );
        }
    }
}

#[test]
fn full_help_takes_its_rows_from_the_body() {
    let mut model = list_with(40);
    let short = model.body_height();

    press(&mut model, &[char_key('?')]);

    assert!(model.body_height() < short);
    assert!(render(&model).contains("half page down"));
}

#[test]
fn trashed_keys_are_marked() {
    let mut model = loaded(120, 40);
    model.list.update(&item("share-1", "item-1"), |key| {
        key.state = State::Trashed;
    });

    assert!(render(&model).contains("TRASHED"));
}

/// A paste is never a sequence of commands: in the list it is dropped, so
/// pasting "daily" cannot open the trash prompt and confirm it.
#[test]
fn pasting_into_the_list_does_nothing() {
    let mut model = list_with(3);

    let commands = model.update(Message::Paste("daily\nq".to_owned()));

    assert!(commands.is_empty(), "{commands:?}");
    assert!(matches!(model.mode, Mode::List));
    assert!(!model.is_quitting());
}

#[test]
fn pasting_reaches_the_focused_input_sanitised() {
    let mut model = list_with(12);
    press(&mut model, &[char_key('/')]);
    model.update(Message::Paste("key-11\u{1b}[2J".to_owned()));
    assert_eq!(model.list.filter().value(), "key-11");
    assert_eq!(model.list.visible_len(), 1);

    let mut model = loaded(120, 40);
    press(&mut model, &[char_key('n')]);
    model.update(Message::Paste("pasted title\n".to_owned()));
    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);
    assert!(
        matches!(&commands[..], [Command::Generate(request), ..] if request.title == "pasted title"),
        "{commands:?}"
    );
}

/// Pasting the title is the natural way to confirm a long one, and the line
/// break a copy usually carries must not spoil the match.
#[test]
fn a_pasted_title_confirms_a_delete() {
    let mut model = loaded(120, 40);
    press(
        &mut model,
        &[KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)],
    );

    model.update(Message::Paste("laptop\n".to_owned()));
    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);

    assert!(
        matches!(
            &commands[..],
            [Command::Remove {
                removal: Removal::Delete,
                ..
            }]
        ),
        "{commands:?}"
    );
}

/// A title longer than the input's usual limit can still be typed in full.
#[test]
fn a_long_title_can_still_be_deleted() {
    let title = "k".repeat(300);
    let personal = vault("Personal", "s1");
    let mut model = Model::new(Theme::default());
    model.update(Message::Resize {
        width: 120,
        height: 40,
    });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vec![personal.clone()]),
    });
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: personal,
        result: Ok(vec![key("s1", "Personal", "long", &title)]),
    });
    press(
        &mut model,
        &[KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)],
    );

    model.update(Message::Paste(title));
    let commands = press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);

    assert!(
        matches!(&commands[..], [Command::Remove { .. }]),
        "{commands:?}"
    );
}

/// The warning for a mistyped title is shown inside the dialog that caused
/// it, not on a list the user cannot see.
#[test]
fn dialogs_show_the_status_line() {
    let mut model = loaded(120, 40);
    press(
        &mut model,
        &[KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)],
    );
    model.confirm.set_value("laptopp");

    press(&mut model, &[KeyEvent::from(KeyCode::Enter)]);

    assert!(
        render(&model).contains("Title does not match"),
        "{}",
        render(&model)
    );
}

/// Quitting while pass-cli is changing something would kill it halfway, and
/// leave the user not knowing whether the key was deleted.
#[test]
fn quitting_waits_for_a_change_in_flight() {
    let mut model = loaded(120, 40);
    let commands = press(&mut model, &[char_key('d'), char_key('y')]);
    assert!(matches!(&commands[..], [Command::Remove { .. }]));

    press(&mut model, &[char_key('q')]);
    assert!(!model.is_quitting(), "quit while the trash was in flight");
    assert!(render(&model).contains("Waiting for pass-cli"));

    model.update(Message::ItemRemoved {
        title: "laptop".to_owned(),
        removal: Removal::Trash,
        result: Ok(()),
    });
    assert!(model.is_quitting(), "did not quit once the trash finished");
}

#[test]
fn quitting_twice_does_not_wait() {
    let mut model = loaded(120, 40);
    press(&mut model, &[char_key('d'), char_key('y')]);

    press(&mut model, &[ctrl('c'), ctrl('c')]);

    assert!(model.is_quitting());
}

#[test]
fn termination_quits_at_once() {
    let mut model = loaded(120, 40);
    press(&mut model, &[char_key('d'), char_key('y')]);

    model.update(Message::Terminate);

    assert!(model.is_quitting());
}

/// Titles are quoted for display, not escaped: combining marks are part of
/// the text.
#[test]
fn status_messages_keep_titles_as_written() {
    let mut model = loaded(120, 40);

    model.update(Message::Copied {
        title: "cle\u{301}".to_owned(),
        result: Ok(()),
    });

    assert_eq!(
        model.status.as_ref().map(|status| status.text.as_str()),
        Some("Copied public key for \"cle\u{301}\".")
    );
}

/// Measures the hot paths at a size far beyond any real account, to check
/// they stay well inside a 16 ms frame. Run it optimised:
///
/// ```sh
/// cargo test --release large_lists -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measures timings; asserts nothing"]
fn large_lists() {
    const COUNT: usize = 10_000;
    const FRAMES: u32 = 200;

    let personal = vault("Personal", "s1");
    let keys: Vec<Key> = (0..COUNT)
        .map(|i| {
            let mut key = key(
                "s1",
                "Personal",
                &format!("item-{i:05}"),
                &format!("key {i:05}"),
            );
            key.set_public_key(TEST_PUBLIC_KEY).expect("valid test key");
            key
        })
        .collect();
    let mut model = Model::new(Theme::default());
    model.update(Message::Resize {
        width: 200,
        height: 60,
    });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vec![personal.clone()]),
    });

    let started = Instant::now();
    model.update(Message::KeysLoaded {
        generation: 0,
        vault: personal,
        result: Ok(keys),
    });
    let load = started.elapsed();

    let mut terminal = Terminal::new(TestBackend::new(200, 60)).expect("test terminal");
    let started = Instant::now();
    for _ in 0..FRAMES {
        model.update(Message::Key(char_key('j')));
        terminal.draw(|frame| model.render(frame)).expect("render");
    }
    let frame = started.elapsed() / FRAMES;

    model.update(Message::Key(char_key('/')));
    let typed = "key 0999";
    let started = Instant::now();
    for c in typed.chars() {
        model.update(Message::Key(char_key(c)));
        terminal.draw(|frame| model.render(frame)).expect("render");
    }
    let keystroke = started.elapsed() / u32::try_from(typed.len()).expect("short");

    let hostile = "title \u{1b}]52;c;cHduZWQ=\u{7} \u{202e}x ".repeat(100_000);
    let started = Instant::now();
    let clean = crate::keys::sanitize(&hostile);
    let sanitize = started.elapsed();

    println!(
        "{COUNT} keys: load and sort {load:?} | move and redraw {frame:?} per frame | \
         filter keystroke and redraw {keystroke:?} | sanitise {} MB in {sanitize:?}",
        hostile.len() / 1_000_000
    );
    assert!(!clean.contains('\u{1b}'));
}

/// Text from outside protui reaches the screen only through the paths that
/// sanitise it: a key's comment through `set_public_key`, and anything typed or
/// pasted through `TextInput`. Titles, vault names and the agent's fields are
/// sanitised where they enter, in `passcli`, and tested there.
#[test]
fn no_screen_draws_control_or_bidi_characters() {
    const PAYLOAD: &str = "\u{1b}]52;c;cHduZWQ=\u{7}\u{202e}\u{2066}\u{1b}[2J\u{2028}\u{85}";

    let type_payload = |model: &mut Model| {
        for c in PAYLOAD.chars() {
            press(model, &[char_key(c)]);
        }
        model.update(Message::Paste(PAYLOAD.to_owned()));
    };
    let tab = KeyEvent::from(KeyCode::Tab);

    let mut screens: Vec<(&str, Model)> = Vec::new();
    let base = || {
        let mut model = loaded(120, 40);
        let hostile = format!("{TEST_PUBLIC_KEY} {PAYLOAD}");
        deliver_public_key(&mut model, "share-1", "item-1", &hostile);
        model
    };
    screens.push(("list", base()));

    let mut filter = base();
    press(&mut filter, &[char_key('/')]);
    type_payload(&mut filter);
    screens.push(("filter", filter));

    let mut form = base();
    press(&mut form, &[char_key('n')]);
    type_payload(&mut form);
    press(&mut form, &[tab, tab, tab]);
    type_payload(&mut form);
    press(&mut form, &[tab]);
    type_payload(&mut form);
    screens.push(("new key", form));

    let mut trash = base();
    press(&mut trash, &[char_key('d')]);
    screens.push(("trash", trash));

    let mut delete = base();
    press(
        &mut delete,
        &[KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)],
    );
    type_payload(&mut delete);
    screens.push(("delete", delete));

    let mut agent = base();
    press(&mut agent, &[char_key('a')]);
    screens.push(("agent", agent));

    for (name, model) in screens {
        let screen = render(&model);
        let found = screen
            .chars()
            .find(|&c| c != '\n' && !crate::keys::is_displayable(c));
        assert_eq!(found, None, "{name} draws {found:?}:\n{screen}");
    }
}

/// The agent chip uses the status protui already has, and only says the agent
/// is up when it is.
#[test]
fn the_header_shows_the_agent_only_while_it_runs() {
    let mut model = loaded(120, 40);
    assert!(!render(&model).contains("AGENT UP"));

    model.update(Message::AgentStatusLoaded(Ok(running_agent())));

    assert!(render(&model).contains("AGENT UP"));
}

/// Red belongs to the permanent delete alone. Outside its dialog, the only red
/// on screen is the `D` keycap in the footer.
#[test]
fn red_appears_only_for_the_permanent_delete() {
    for theme in themes() {
        let screens = screens(theme, 120, 38);
        for (name, model) in screens
            .iter()
            .filter(|(name, _)| !name.starts_with("delete"))
        {
            let buffer = render_buffer(model);
            let footer = model.height - FOOTER_ROWS - model.help_rows();

            for (index, cell) in buffer.content.iter().enumerate() {
                let (x, y) = buffer.pos_of(index);
                let red = [cell.fg, cell.bg]
                    .iter()
                    .any(|&color| color == theme.red || color == theme.danger);
                let keycap = y >= footer && matches!(cell.symbol(), " " | "D");
                assert!(!red || keycap, "{name}: red at ({x}, {y}) {cell:?}");
            }
        }
    }
}

/// A coloured fill always carries `#111111` text, in either theme. Shadows are
/// not text, and flip to cream on a dark terminal like the borders do.
#[test]
fn coloured_fills_carry_ink_text() {
    for theme in themes() {
        let fills = [
            theme.yellow,
            theme.violet,
            theme.mint,
            theme.pink,
            theme.orange,
            theme.red,
        ];
        for (width, height) in [(120, 38), (80, 24)] {
            for (name, model) in screens(theme, width, height) {
                let buffer = render_buffer(&model);
                for (index, cell) in buffer.content.iter().enumerate() {
                    let shadow = [" ", GLYPH_LOWER_HALF, GLYPH_UPPER_HALF].contains(&cell.symbol());
                    if fills.contains(&cell.bg) && !shadow {
                        assert_eq!(
                            cell.fg,
                            Color::Rgb(0x11, 0x11, 0x11),
                            "{name} {width}x{height}: {cell:?} at {:?}",
                            buffer.pos_of(index)
                        );
                    }
                }
            }
        }
    }
}

/// Every screen at the design's size and at a common small one. Each snapshot
/// holds the characters once, then the colours of both themes, so a change to
/// either shows up.
#[test]
fn screens_match_their_snapshots() {
    let [light, dark] = themes();
    for (width, height) in [(120, 38), (80, 24)] {
        let pairs = screens(light, width, height)
            .into_iter()
            .zip(screens(dark, width, height));
        for ((name, light), (_, dark)) in pairs {
            insta::assert_snapshot!(format!("{name}_{width}x{height}"), snapshot(&light, &dark));
        }
    }
}

fn themes() -> [Theme; 2] {
    [
        Theme::new(Background::Light, ColorDepth::TrueColor),
        Theme::new(Background::Dark, ColorDepth::TrueColor),
    ]
}

fn running_agent() -> AgentStatus {
    AgentStatus {
        state: AgentState::Running,
        detail: "running".to_owned(),
        pid: "48213".to_owned(),
        socket: "/Users/me/.proton-pass/ssh-agent.sock".to_owned(),
    }
}

/// A model like the design's reference screens: keys of each algorithm
/// across three vaults, every public key read, the agent running, and the
/// second key selected.
fn showcase(theme: Theme, width: u16, height: u16) -> Model {
    let vaults = vec![
        vault("Infra", "s-infra"),
        vault("Personal", "s-personal"),
        vault("Work", "s-work"),
    ];
    let keys = [
        ("s-work", "Work", "deploy-ci", CI_PUBLIC_KEY),
        (
            "s-personal",
            "Personal",
            "github-personal",
            LAPTOP_PUBLIC_KEY,
        ),
        ("s-work", "Work", "gitlab-work", TEST_PUBLIC_KEY),
        ("s-infra", "Infra", "hetzner-vps", RSA4096_PUBLIC_KEY),
        ("s-personal", "Personal", "old-macbook", PREVIEW_RSA),
    ];

    let mut model = Model::new(theme);
    model.update(Message::Resize { width, height });
    model.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(vaults.clone()),
    });
    for vault in vaults {
        let listed = keys
            .iter()
            .filter(|(share_id, ..)| *share_id == vault.share_id.as_str())
            .map(|&(share_id, name, title, _)| key(share_id, name, title, title))
            .collect();
        model.update(Message::KeysLoaded {
            generation: 0,
            vault,
            result: Ok(listed),
        });
    }
    for (share_id, _, title, public_key) in keys {
        deliver_public_key(&mut model, share_id, title, public_key);
    }
    model.update(Message::AgentStatusLoaded(Ok(running_agent())));
    press(&mut model, &[char_key('j')]);
    model
}

/// Every screen and state worth looking at, built the way a user reaches it.
fn screens(theme: Theme, width: u16, height: u16) -> Vec<(&'static str, Model)> {
    let list = || showcase(theme, width, height);
    let typed = |model: &mut Model, text: &str| {
        for c in text.chars() {
            press(model, &[char_key(c)]);
        }
    };
    let fresh = || {
        let mut model = Model::new(theme);
        model.update(Message::Resize { width, height });
        model
    };
    let mut screens = Vec::new();

    let mut copied = list();
    copied.update(Message::Copied {
        title: "github-personal".to_owned(),
        result: Ok(()),
    });
    screens.push(("list", copied));

    let mut filtering = list();
    press(&mut filtering, &[char_key('/')]);
    typed(&mut filtering, "git");
    screens.push(("filter_typing", filtering));

    let mut filtered = list();
    press(&mut filtered, &[char_key('/')]);
    typed(&mut filtered, "git");
    press(&mut filtered, &[KeyEvent::from(KeyCode::Enter)]);
    screens.push(("filter_applied", filtered));

    let mut help = list();
    press(&mut help, &[char_key('?')]);
    screens.push(("help", help));

    let mut warned = list();
    let _ = warned.notify(
        "No public key loaded for this item yet.".to_owned(),
        Tone::Warn,
    );
    screens.push(("status_warn", warned));

    let mut failed = list();
    let _ = failed.notify(
        "Clipboard unavailable: no clipboard tool found".to_owned(),
        Tone::Error,
    );
    screens.push(("status_error", failed));

    let mut new = list();
    press(&mut new, &[char_key('n')]);
    typed(&mut new, "staging-bastion");
    screens.push(("new_key", new));

    let mut trash = list();
    press(&mut trash, &[char_key('d')]);
    screens.push(("trash", trash));

    let shift_d = KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT);
    let mut delete = list();
    press(&mut delete, &[shift_d]);
    typed(&mut delete, "github-pers");
    screens.push(("delete", delete));

    let mut matched = list();
    press(&mut matched, &[shift_d]);
    typed(&mut matched, "github-personal");
    screens.push(("delete_matched", matched));

    let mut running = list();
    press(&mut running, &[char_key('a')]);
    running.update(Message::AgentStatusLoaded(Ok(running_agent())));
    screens.push(("agent_running", running));

    let mut stopped = list();
    press(&mut stopped, &[char_key('a')]);
    stopped.update(Message::AgentStatusLoaded(Ok(AgentStatus {
        state: AgentState::Stopped,
        ..AgentStatus::default()
    })));
    screens.push(("agent_stopped", stopped));

    screens.push(("loading", fresh()));

    let mut empty = fresh();
    empty.update(Message::VaultsLoaded {
        generation: 0,
        result: Ok(Vec::new()),
    });
    screens.push(("empty", empty));

    let mut error = fresh();
    error.update(Message::VaultsLoaded {
        generation: 0,
        result: Err(CommandError::with_stderr(
            "vault list",
            "session expired; run pass-cli login",
        )),
    });
    screens.push(("error", error));

    screens
}

/// The screen as text, then each theme's colours as runs along each row.
fn snapshot(light: &Model, dark: &Model) -> String {
    let light = render_buffer(light);
    let dark = render_buffer(dark);
    let text = |buffer: &Buffer| -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                row.trim_end().to_owned()
            })
            .collect()
    };
    assert_eq!(
        text(&light),
        text(&dark),
        "the themes differ in more than colour"
    );

    let mut sections = vec![text(&light).join("\n")];
    for (name, buffer) in [("light", &light), ("dark", &dark)] {
        let mut lines = vec![name.to_owned()];
        lines.extend((0..buffer.area.height).filter_map(|y| {
            let runs = style_runs(buffer, y);
            (!runs.is_empty()).then(|| format!("{y:>2} {runs}"))
        }));
        sections.push(lines.join("\n"));
    }
    sections.join("\n\n")
}

/// One row's styled stretches, as `first-last fg/bg` plus `b` for bold and `r`
/// for reverse. Unstyled stretches are left out.
fn style_runs(buffer: &Buffer, y: u16) -> String {
    let style = |cell: &Cell| (cell.fg, cell.bg, cell.modifier);
    let width = buffer.area.width;
    let mut runs = Vec::new();
    let mut start = 0;

    for x in 1..=width {
        if x < width && style(&buffer[(x, y)]) == style(&buffer[(start, y)]) {
            continue;
        }
        let cell = &buffer[(start, y)];
        if style(cell) != (Color::Reset, Color::Reset, Modifier::empty()) {
            runs.push(format!("{start}-{} {}", x - 1, describe(cell)));
        }
        start = x;
    }
    runs.join(" | ")
}

fn describe(cell: &Cell) -> String {
    let color = |color: Color| match color {
        Color::Rgb(red, green, blue) => format!("{red:02x}{green:02x}{blue:02x}"),
        Color::Reset => "-".to_owned(),
        other => format!("{other:?}"),
    };
    let mut description = format!("{}/{}", color(cell.fg), color(cell.bg));
    if cell.modifier.contains(Modifier::BOLD) {
        description.push_str(" b");
    }
    if cell.modifier.contains(Modifier::REVERSED) {
        description.push_str(" r");
    }
    description
}

/// Prints every screen with colour, for eyeballing the layout against a light
/// and a dark terminal, at the design's size and at 80×24:
///
/// ```sh
/// cargo test preview -- --ignored --nocapture
/// PROTUI_PREVIEW_LIGHT=1 cargo test preview -- --ignored --nocapture
/// ```
#[test]
#[ignore = "renders every screen for eyeballing; asserts nothing"]
fn preview() {
    let background = if std::env::var_os("PROTUI_PREVIEW_LIGHT").is_some() {
        Background::Light
    } else {
        Background::Dark
    };
    let theme = Theme::new(background, ColorDepth::TrueColor);

    for (width, height) in [(120, 38), (80, 24)] {
        for (name, model) in screens(theme, width, height) {
            show(&format!("{name} {width}x{height}"), &model);
        }
    }
}

/// Prints a rendered screen with its colours as ANSI escapes.
fn show(name: &str, model: &Model) {
    let buffer = render_buffer(model);
    let mut screen = String::new();

    for y in 0..buffer.area.height {
        let mut last = None;
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            let style = (cell.fg, cell.bg, cell.modifier);
            if last != Some(style) {
                screen.push_str("\u{1b}[0m");
                screen.push_str(&sgr(cell.fg, 38));
                screen.push_str(&sgr(cell.bg, 48));
                if cell.modifier.contains(Modifier::BOLD) {
                    screen.push_str("\u{1b}[1m");
                }
                if cell.modifier.contains(Modifier::REVERSED) {
                    screen.push_str("\u{1b}[7m");
                }
                last = Some(style);
            }
            screen.push_str(cell.symbol());
        }
        screen.push_str("\u{1b}[0m\n");
    }

    println!("\n\u{1b}[7m {name} \u{1b}[0m\n\n{screen}");
}

fn sgr(color: Color, base: u8) -> String {
    match color {
        Color::Rgb(red, green, blue) => format!("\u{1b}[{base};2;{red};{green};{blue}m"),
        Color::Indexed(index) => format!("\u{1b}[{base};5;{index}m"),
        _ => String::new(),
    }
}
