//! protui's domain types for SSH key items.
//!
//! Nothing here mirrors the pass-cli JSON shape: [`crate::passcli`] is
//! responsible for translating upstream output into these types, so an
//! upstream field rename stays contained to that module. See `docs/schema.md`.
//!
//! Upstream stores only the public key blob, the private key and a list of
//! custom sections; there are no algorithm, fingerprint or comment fields.
//! Those three are derived here by parsing the public key. Private key
//! material never reaches this module.

pub use civil_date_time::CivilDateTime;
pub use key::{Algorithm, ItemId, ItemRef, Key, ShareId, State, Vault};
pub use sanitize::{is_displayable, sanitize};

mod civil_date_time;
mod key;
mod sanitize;
