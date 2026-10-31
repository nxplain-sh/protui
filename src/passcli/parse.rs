//! Translating pass-cli output into domain types.
//!
//! The parsers are pure functions over bytes so they can be exercised against
//! fixtures without running pass-cli. Unknown JSON keys are tolerated (see
//! [`super::wire`]); the inverse risk — a renamed or removed field decoding
//! silently to its default — is caught by validating that required fields are
//! non-empty.

use crate::keys::{self, ItemId, Key, ShareId, State, Vault};
use crate::passcli::agent::{AgentState, AgentStatus};
use crate::passcli::client::Failure;
use crate::passcli::wire;

/// Decodes `vault list --output json`.
pub(super) fn vault_list(stdout: &[u8]) -> Result<Vec<Vault>, Failure> {
    let response: wire::VaultListResponse =
        serde_json::from_slice(stdout).map_err(Failure::Decode)?;

    response
        .vaults
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            // share_id is the handle every item subcommand needs; without it
            // the vault is unusable, so an empty one is a schema break, not a
            // quirk.
            if entry.share_id.is_empty() {
                return Err(Failure::Schema(format!(
                    "vault at index {index} has no share_id"
                )));
            }

            Ok(Vault {
                // Vault names are user-authored and can arrive from a shared
                // vault, so they are sanitised before anything renders them.
                name: keys::sanitize(&entry.name),
                share_id: ShareId::new(entry.share_id),
                id: entry.vault_id,
            })
        })
        .collect()
}

/// Decodes `item list --output json` and keeps only SSH key items.
///
/// `--filter-type` is applied client-side by upstream, so it is a bandwidth
/// optimisation rather than a guarantee; the item type is checked again here.
///
/// `vault` supplies the human-readable vault name, which the item output does
/// not carry. Public key metadata is not available from this call and is
/// filled in later by fetching the public key per item.
pub(super) fn item_list(stdout: &[u8], vault: &Vault) -> Result<Vec<Key>, Failure> {
    let response: wire::ItemListResponse =
        serde_json::from_slice(stdout).map_err(Failure::Decode)?;

    let mut keys = Vec::with_capacity(response.items.len());

    let items = response.items.into_iter().enumerate();
    for (index, item) in items {
        if item.item_type != wire::ITEM_TYPE_SSH_KEY {
            continue;
        }

        if item.id.is_empty() {
            return Err(Failure::Schema(format!("item at index {index} has no id")));
        }
        if item.title.is_empty() {
            return Err(Failure::Schema(format!(
                "item {} has no title",
                keys::sanitize(&item.id)
            )));
        }

        // Upstream echoes share_id per item, but it is the same vault that was
        // asked about; fall back to that so a blank one cannot orphan the item.
        let share_id = if item.share_id.is_empty() {
            vault.share_id.clone()
        } else {
            ShareId::new(item.share_id)
        };

        keys.push(Key {
            id: ItemId::new(item.id),
            share_id,
            vault_id: item.vault_id,
            vault_name: vault.name.clone(),
            // Titles are user-authored and shareable: untrusted display input.
            title: keys::sanitize(&item.title),
            state: state(&item.state),
            created_at: item.create_time,
            modified_at: item.modify_time,
            ..Key::default()
        });
    }

    Ok(keys)
}

/// Maps upstream's `PascalCase` state onto the domain enum.
///
/// An unrecognised state is reported as active so the item stays visible and
/// actionable rather than silently vanishing.
fn state(raw: &str) -> State {
    match raw {
        wire::STATE_TRASHED => State::Trashed,
        // "Active", and anything unrecognised.
        _ => State::Active,
    }
}

/// Reads `ssh-agent daemon status`, which has no JSON output and must be
/// line-parsed. See `docs/schema.md` §7.
///
/// The command exits 0 whether or not the daemon runs, so state comes only
/// from stdout. Values are matched by prefix because the parenthetical
/// suffixes are diagnostic prose and will change.
#[must_use]
pub(super) fn agent_status(stdout: &[u8]) -> AgentStatus {
    let mut status = AgentStatus::default();

    let text = String::from_utf8_lossy(stdout);
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };

        // "PID file" must not match "PID": compare the whole trimmed key.
        let value = value.trim();
        match key.trim() {
            "Status" => {
                // Classify on the raw value, display the sanitised one.
                status.state = agent_state(value);
                status.detail = keys::sanitize(value);
            }
            "PID" => status.pid = keys::sanitize(value),
            "Socket" => status.socket = keys::sanitize(value),
            _ => {}
        }
    }

    status
}

/// Classifies the `Status:` value by prefix.
fn agent_state(value: &str) -> AgentState {
    if value.starts_with("running") {
        AgentState::Running
    } else if value.starts_with("degraded") {
        AgentState::Degraded
    } else if value.starts_with("stopped") {
        AgentState::Stopped
    } else {
        AgentState::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_bytes!(concat!("testdata/", $name)).as_slice()
        };
    }

    fn personal() -> Vault {
        Vault {
            name: "Personal".to_owned(),
            share_id: ShareId::from("c2hhcmUtcGVyc29uYWw="),
            ..Vault::default()
        }
    }

    #[test]
    fn vault_list_decodes_every_vault() {
        let vaults = vault_list(fixture!("vault_list.json")).expect("valid fixture");

        assert_eq!(
            vaults,
            [
                Vault {
                    name: "Personal".to_owned(),
                    share_id: ShareId::from("c2hhcmUtcGVyc29uYWw="),
                    id: "dmF1bHQtcGVyc29uYWw=".to_owned(),
                },
                Vault {
                    name: "Work".to_owned(),
                    share_id: ShareId::from("c2hhcmUtd29yaw=="),
                    id: "dmF1bHQtd29yaw==".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn vault_list_may_be_empty() {
        assert_eq!(
            vault_list(fixture!("vault_list_empty.json")).expect("valid fixture"),
            []
        );
    }

    /// `share_id` is the handle every item command needs, so a blank one is a
    /// schema break rather than something to skip past.
    #[test]
    fn vault_list_without_share_id_is_a_schema_error() {
        let failure = vault_list(fixture!("vault_list_missing_share_id.json"));

        assert!(matches!(failure, Err(Failure::Schema(_))), "{failure:?}");
    }

    #[test]
    fn vault_list_rejects_malformed_json() {
        assert!(matches!(vault_list(b"not json"), Err(Failure::Decode(_))));
    }

    #[test]
    fn item_list_decodes_ssh_keys() {
        let keys = item_list(fixture!("item_list_ssh_keys.json"), &personal()).expect("valid");

        let summary: Vec<(&str, &str, State, &str, String)> = keys
            .iter()
            .map(|key| {
                let created = key.created_at.map(|at| at.to_string()).unwrap_or_default();
                (
                    key.id.as_str(),
                    key.title.as_str(),
                    key.state,
                    key.vault_name.as_str(),
                    created,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "aXRlbS1lZDI1NTE5",
                    "laptop",
                    State::Active,
                    "Personal",
                    "2026-01-23 19:48".into()
                ),
                (
                    "aXRlbS1yc2E=",
                    "deploy key",
                    State::Active,
                    "Personal",
                    "2025-11-02 11:00".into()
                ),
            ]
        );

        for key in &keys {
            // The list call carries no key material, so metadata stays unknown
            // until the public key is fetched separately.
            assert_eq!(key.label(), "unknown");
            assert_eq!(key.public_key, "");
        }
    }

    /// `--filter-type` is applied client-side upstream, so it is an
    /// optimisation and not a guarantee. Non-SSH items must be dropped here
    /// too.
    #[test]
    fn item_list_drops_other_item_types() {
        let keys = item_list(fixture!("item_list_mixed.json"), &personal()).expect("valid");

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].title, "old key");
        assert_eq!(keys[0].state, State::Trashed);
    }

    #[test]
    fn item_list_may_be_empty() {
        let keys = item_list(fixture!("item_list_empty.json"), &personal()).expect("valid");

        assert_eq!(keys, Vec::new());
    }

    /// A new upstream field must never break the list.
    #[test]
    fn item_list_tolerates_unknown_fields() {
        let keys =
            item_list(fixture!("item_list_unknown_fields.json"), &personal()).expect("valid");

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].title, "future key");
    }

    /// A renamed or removed field decodes to its default silently; required
    /// fields are validated so that surfaces as an error.
    #[test]
    fn item_list_without_id_is_a_schema_error() {
        let failure = item_list(fixture!("item_list_missing_id.json"), &personal());

        assert!(matches!(failure, Err(Failure::Schema(_))), "{failure:?}");
    }

    /// Upstream timestamps are zoneless. If they ever gain an offset, that is
    /// a schema change worth hearing about loudly.
    #[test]
    fn item_list_rejects_a_zoned_timestamp() {
        let failure = item_list(fixture!("item_list_bad_timestamp.json"), &personal());

        assert!(matches!(failure, Err(Failure::Decode(_))), "{failure:?}");
    }

    /// Upstream omitting the per-item `share_id`: the vault that was queried is
    /// the correct fallback.
    #[test]
    fn item_list_falls_back_to_the_queried_share_id() {
        let vault = Vault {
            share_id: ShareId::from("fallback-share"),
            ..personal()
        };
        let input = br#"{"items":[{"id":"a","share_id":"","title":"k","item_type":"ssh_key",
            "state":"Active","create_time":null,"modify_time":""}]}"#;

        let keys = item_list(input, &vault).expect("valid");

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].share_id, ShareId::from("fallback-share"));
        assert_eq!(keys[0].created_at, None);
    }

    /// An unrecognised state keeps the item visible rather than dropping it.
    #[test]
    fn unknown_states_stay_active() {
        let cases = [
            ("Active", State::Active),
            ("Trashed", State::Trashed),
            ("Archived", State::Active),
            ("", State::Active),
            // Casing matters: upstream emits PascalCase here, not snake_case.
            ("active", State::Active),
            ("trashed", State::Active),
        ];

        for (raw, want) in cases {
            assert_eq!(state(raw), want, "state({raw:?})");
        }
    }

    #[test]
    fn agent_status_fixtures() {
        let cases = [
            (
                fixture!("agent_running.txt"),
                AgentState::Running,
                "54321",
                "/Users/example/.proton-pass/ssh-agent.sock",
            ),
            (
                fixture!("agent_stopped_no_pidfile.txt"),
                AgentState::Stopped,
                "",
                "",
            ),
            (
                // The parenthetical is diagnostic prose, so the state is
                // matched by prefix.
                fixture!("agent_degraded.txt"),
                AgentState::Degraded,
                "54321",
                "/Users/example/.proton-pass/ssh-agent.sock (not found)",
            ),
            (
                fixture!("agent_stopped_stale_socket.txt"),
                AgentState::Stopped,
                "54321 (not running)",
                "/Users/example/.proton-pass/ssh-agent.sock (stale)",
            ),
        ];

        for (stdout, state, pid, socket) in cases {
            let status = agent_status(stdout);

            assert_eq!(
                (status.state, status.pid.as_str(), status.socket.as_str()),
                (state, pid, socket)
            );
        }
    }

    /// The one genuinely ambiguous line: "PID file:" must not be read as
    /// "PID:".
    #[test]
    fn agent_status_does_not_confuse_pid_file_with_pid() {
        let status = agent_status(b"Status:   stopped\nPID file: /tmp/x.pid (not found)\n");

        assert_eq!(status.pid, "");
        assert_eq!(status.state, AgentState::Stopped);
    }

    /// Upstream rewording the status line entirely: better to report unknown
    /// than to guess "stopped".
    #[test]
    fn agent_status_reports_unknown_wording_as_unknown() {
        let status = agent_status(b"Status:   hibernating\n");

        assert_eq!(status.state, AgentState::Unknown);
        assert_eq!(status.detail, "hibernating");
    }

    #[test]
    fn agent_status_of_nothing_is_unknown() {
        assert_eq!(agent_status(b"").state, AgentState::Unknown);
    }

    /// The boundary at which text from Proton Pass becomes text protui will
    /// draw.
    ///
    /// Item titles and vault names are user-authored, and Proton Pass supports
    /// sharing both items and vaults, so either can have been written by
    /// somebody else. This is the choke point; see
    /// `docs/adr/0011-sanitize-text-before-drawing-it.md`.
    #[test]
    fn parsers_sanitize_hostile_text() {
        let keys = item_list(fixture!("item_list_hostile_title.json"), &personal()).expect("valid");
        assert_eq!(keys.len(), 1);
        assert_no_escapes("title", &keys[0].title);
        // The readable part survives; only the instruction is removed.
        assert!(keys[0].title.contains("innocent"), "{:?}", keys[0].title);

        let vaults = vault_list(fixture!("vault_list_hostile_name.json")).expect("valid");
        assert_eq!(vaults.len(), 1);
        assert_no_escapes("vault name", &vaults[0].name);
        assert!(vaults[0].name.contains("Personal"), "{:?}", vaults[0].name);

        let status = agent_status(
            b"Status:   running\x1b[2J\nPID:      42\x07\nSocket:   /tmp/a\x1b]0;x\x07.sock\n",
        );
        // Classification still reads the raw value, so it is unaffected.
        assert_eq!(status.state, AgentState::Running);
        assert_no_escapes("detail", &status.detail);
        assert_no_escapes("pid", &status.pid);
        assert_no_escapes("socket", &status.socket);
    }

    /// Fails if `value` carries anything a terminal would act on.
    fn assert_no_escapes(field: &str, value: &str) {
        for c in value.chars() {
            assert!(
                !c.is_control(),
                "{field} contains control character {c:?}: {value:?}"
            );
            assert!(
                !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
                "{field} contains bidi override {c:?}: {value:?}"
            );
        }
    }
}
