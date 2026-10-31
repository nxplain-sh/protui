//! The only module in protui that executes the pass-cli binary.
//!
//! Everything else depends on these typed wrappers rather than on
//! `std::process`, so the upstream command surface stays behind one seam.
//!
//! Security invariants, enforced here and nowhere else:
//!
//! - No secret is ever an argv element, because argv is world-readable via
//!   `ps`. The one secret protui can send — an SSH key passphrase on create —
//!   goes through the child environment.
//! - Every user-supplied value is attached to its flag as `--flag=value`, so a
//!   value beginning with `-` is read as a value and never as another flag.
//! - `item view --output json` is never used: it returns the private key
//!   unredacted. Public keys are read with `--field=public_key`, which prints a
//!   single field as bare text.
//! - `--show-secrets` is never passed.
//! - No private key material is read, stored, or logged by any function here.
//!
//! See `docs/schema.md` for the upstream output contract these wrappers
//! assume.

pub use agent::{AgentState, AgentStatus};
pub use client::{BINARY, Client, CommandError, StartupError};
pub use item::{GenerateRequest, KeyType};

mod agent;
mod client;
mod item;
mod parse;
mod wire;
