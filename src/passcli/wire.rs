//! The pass-cli JSON shapes, exactly as upstream emits them.
//!
//! Every struct tolerates unknown keys (serde ignores them unless
//! `deny_unknown_fields` is set, and it deliberately is not), so an upstream
//! addition cannot break the list. Missing keys decode to their default and
//! required ones are validated in [`super::parse`].

use serde::{Deserialize, Deserializer};

use crate::keys::CivilDateTime;

// Upstream spells the same concept three different ways depending on where it
// appears, because each Rust type derives Serialize differently. Keeping
// separate constants rather than one prevents "helpfully" normalising them.
// See docs/schema.md §3.1.

/// The JSON value of `item_type`: `snake_case`, from a
/// `#[serde(rename_all = "snake_case")]` enum.
pub(super) const ITEM_TYPE_SSH_KEY: &str = "ssh_key";

/// The `--filter-type` flag value: `kebab-case`, parsed by a hand-written
/// `FromStr`.
pub(super) const FILTER_TYPE_SSH_KEY: &str = "ssh-key";

/// The JSON value of a trashed `state`: `PascalCase`, from a plain derive
/// with no rename attribute. The other value is `Active`.
pub(super) const STATE_TRASHED: &str = "Trashed";

/// The envelope of `vault list --output json`.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct VaultListResponse {
    pub vaults: Vec<VaultEntry>,
}

/// One vault in `vault list --output json`.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct VaultEntry {
    pub name: String,
    pub vault_id: String,
    pub share_id: String,
}

/// The envelope of `item list --output json`.
///
/// Upstream always wraps the array; it is never a bare list. An "internal"
/// build adds a sibling `folders` key, which is ignored here by design.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct ItemListResponse {
    pub items: Vec<ItemSummary>,
}

/// The redacted summary form of `item list`.
///
/// protui never passes `--show-secrets`, so no field here can carry key
/// material. Fields protui does not use, such as `flags` and `folder_id`, are
/// left undeclared rather than decoded and discarded.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct ItemSummary {
    pub id: String,
    pub share_id: String,
    pub vault_id: String,
    pub state: String,
    pub title: String,
    pub item_type: String,
    #[serde(deserialize_with = "civil_time")]
    pub create_time: Option<CivilDateTime>,
    #[serde(deserialize_with = "civil_time")]
    pub modify_time: Option<CivilDateTime>,
}

/// Decodes upstream's zoneless timestamps.
///
/// `null` and an empty string both yield `None` rather than an error: a
/// missing timestamp should not drop an otherwise valid item from the list. A
/// present but malformed one is an error, because it means the schema moved.
fn civil_time<'de, D>(deserializer: D) -> Result<Option<CivilDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;

    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(text) => text.parse().map(Some).map_err(serde::de::Error::custom),
    }
}
