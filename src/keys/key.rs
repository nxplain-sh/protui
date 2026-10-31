//! Vaults, SSH key items, and the metadata derived from a public key.

use std::fmt;

use ssh_key::public::KeyData;
use ssh_key::{EcdsaCurve, HashAlg, PublicKey};

use crate::keys::{self, CivilDateTime};

/// The handle every pass-cli item subcommand expects to name a vault.
///
/// A distinct type from [`ItemId`] so that the two can never be passed in each
/// other's place: on the permanent-delete path that mistake would destroy the
/// wrong item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShareId(String);

/// The identifier of one item within a vault.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ItemId(String);

impl ShareId {
    /// Wraps a share id as upstream reported it.
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Returns the id as upstream spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reports whether upstream left the id blank.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for ShareId {
    fn from(id: &str) -> Self {
        Self::new(id.to_owned())
    }
}

impl ItemId {
    /// Wraps an item id as upstream reported it.
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Returns the id as upstream spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ItemId {
    fn from(id: &str) -> Self {
        Self::new(id.to_owned())
    }
}

/// The key algorithm, derived from the public key blob.
///
/// [`Algorithm::Unknown`] is used when the public key is missing or cannot be
/// parsed, so a single malformed item never breaks the whole list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Algorithm {
    Ed25519,
    Rsa,
    Ecdsa,
    Dsa,
    SkEcdsa,
    SkEd25519,
    #[default]
    Unknown,
}

impl Algorithm {
    /// Returns the name protui shows for the algorithm.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Rsa => "rsa",
            Self::Ecdsa => "ecdsa",
            // pass-cli cannot generate DSA, but an old imported key could still
            // be listed, and labelling it beats showing "unknown".
            Self::Dsa => "dsa",
            Self::SkEcdsa => "ecdsa-sk",
            Self::SkEd25519 => "ed25519-sk",
            Self::Unknown => "unknown",
        }
    }

    fn of(key: &KeyData) -> Self {
        match key {
            KeyData::Ed25519(_) => Self::Ed25519,
            KeyData::SkEd25519(_) => Self::SkEd25519,
            KeyData::Rsa(_) => Self::Rsa,
            KeyData::Dsa(_) => Self::Dsa,
            KeyData::Ecdsa(_) => Self::Ecdsa,
            KeyData::SkEcdsaSha2NistP256(_) => Self::SkEcdsa,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An item's lifecycle state. Upstream spells these `Active` and `Trashed`;
/// the mapping lives in [`crate::passcli`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Active,
    Trashed,
}

/// A Proton Pass vault.
///
/// `id` is carried for reference only and is not accepted by any item
/// command; `share_id` is the handle they need.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vault {
    pub name: String,
    pub share_id: ShareId,
    pub id: String,
}

/// An SSH key item as protui presents it.
///
/// `public_key` is the only key material ever held. Algorithm, fingerprint,
/// comment and bits are derived from it locally rather than read from upstream
/// JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "`public_key` is the established name"
)]
pub struct Key {
    pub id: ItemId,
    pub share_id: ShareId,
    pub vault_id: String,
    pub vault_name: String,
    pub title: String,
    pub state: State,

    pub public_key: String,
    pub algorithm: Algorithm,
    pub fingerprint: String,
    pub comment: String,
    pub bits: u32,

    pub created_at: Option<CivilDateTime>,
    pub modified_at: Option<CivilDateTime>,
}

/// Names one item: the vault share it lives in, and its id within that share.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemRef {
    pub share_id: ShareId,
    pub item_id: ItemId,
}

impl Key {
    /// Returns the reference that names this item.
    #[must_use]
    pub fn item_ref(&self) -> ItemRef {
        ItemRef {
            share_id: self.share_id.clone(),
            item_id: self.id.clone(),
        }
    }

    /// Reports whether this is the item `item` names.
    #[must_use]
    pub fn is(&self, item: &ItemRef) -> bool {
        self.id == item.item_id && self.share_id == item.share_id
    }

    /// Stores the public key and the metadata derived from it.
    ///
    /// A parse failure is returned for the caller to surface but is never
    /// fatal: the key is still updated, with unknown metadata, so one
    /// malformed key cannot break a listing.
    pub fn set_public_key(&mut self, public_key: &str) -> Result<(), ssh_key::Error> {
        keys::sanitize(public_key)
            .trim()
            .clone_into(&mut self.public_key);

        let described = describe(&self.public_key);
        let info = described.as_ref().cloned().unwrap_or_default();
        self.algorithm = info.algorithm;
        self.fingerprint = info.fingerprint;
        self.comment = info.comment;
        self.bits = info.bits;

        described.map(|_| ())
    }

    /// Renders the algorithm for display, including the size where it adds
    /// information. RSA is the only algorithm whose size is variable in
    /// practice.
    #[must_use]
    pub fn label(&self) -> String {
        if self.algorithm == Algorithm::Rsa && self.bits > 0 {
            format!("rsa{}", self.bits)
        } else {
            self.algorithm.as_str().to_owned()
        }
    }

    /// Returns the fingerprint without its `SHA256:` prefix, for narrow
    /// columns.
    #[must_use]
    pub fn short_fingerprint(&self) -> &str {
        self.fingerprint
            .strip_prefix("SHA256:")
            .unwrap_or(&self.fingerprint)
    }
}

/// What can be recovered from an OpenSSH public key line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublicKeyInfo {
    pub algorithm: Algorithm,
    pub fingerprint: String,
    pub comment: String,
    pub bits: u32,
}

/// Parses an OpenSSH authorized-keys line and derives the metadata that
/// upstream does not store.
///
/// A blank key yields unknown metadata and no error, since an item
/// legitimately may not have one yet.
pub fn describe(public_key: &str) -> Result<PublicKeyInfo, ssh_key::Error> {
    // A valid key contains no control characters, so this is a no-op for real
    // input and neutralises a hostile blob before it is parsed or displayed.
    let cleaned = keys::sanitize(public_key);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return Ok(PublicKeyInfo::default());
    }

    let key = PublicKey::from_openssh(trimmed)?;

    Ok(PublicKeyInfo {
        algorithm: Algorithm::of(key.key_data()),
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        // The comment is whatever bytes the key file carried after the blob.
        comment: keys::sanitize(key.comment()),
        bits: bits_of(key.key_data()),
    })
}

/// Reports the key size where it is meaningful, or 0 when it is unknown.
///
/// Ed25519 is fixed at 256 bits, ECDSA curves are named in the key type, and
/// RSA needs the modulus.
fn bits_of(key: &KeyData) -> u32 {
    match key {
        KeyData::Ed25519(_) | KeyData::SkEd25519(_) | KeyData::SkEcdsaSha2NistP256(_) => 256,
        KeyData::Ecdsa(ecdsa) => match ecdsa.curve() {
            EcdsaCurve::NistP256 => 256,
            EcdsaCurve::NistP384 => 384,
            EcdsaCurve::NistP521 => 521,
        },
        KeyData::Rsa(rsa) => rsa.n.as_positive_bytes().map_or(0, bit_length),
        _ => 0,
    }
}

/// Counts the significant bits of a big-endian unsigned integer.
fn bit_length(bytes: &[u8]) -> u32 {
    let Some(start) = bytes.iter().position(|&byte| byte != 0) else {
        return 0;
    };
    let significant = &bytes[start..];
    let whole_bytes = u32::try_from(significant.len() - 1).unwrap_or(u32::MAX / 8);

    whole_bytes.saturating_mul(8) + (u8::BITS - significant[0].leading_zeros())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Throwaway keys generated with ssh-keygen purely as test vectors. They
    // guard nothing and correspond to no private key that exists anywhere.
    const ED25519_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM+XgkeZS/lofu0u1xq0g4DQUZIOxGcdSqHhQbjKwVEQ laptop@example";
    const ED25519_FP: &str = "SHA256:jgNTr/lOf7SWNNgIkhV2xOfVZI2zdWn7WremFIOCmaI";

    const RSA2048_KEY: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDDMU3w/a9F5JytBm0CPnKCZ9XlyVxG7d3afM0dJg1+4yZp2dWifkADPAxWAxaLnY7gcJzyCDIC3j5klN1deFpspph4AX09detMoaunhNa/xpxFmWRHin36F+6EQWAKtshvBCoenF8PCcFYrnGsatPPxI2Dbyl0QNacQgfiG4C0YLS1/Ajx/JyY8MwoVqM6hIBmE/hVdAzH+EwVe7R7jLcLNAW63j71At/OYyWOCMeOkPN3JPzRHxadMl666XI/ML0tzmv2akTi3bohkQUtXtFS7rFmVhZF78kYACs4gWoyxNCPpHFh3omYLvGL5xk0qhCgs0O99d6vmNZ2BDb1MNgf";
    const RSA2048_FP: &str = "SHA256:QLsAuTpG7EF7VFz0rGC7Jy34NWCKPWprfrJBdoABz/k";

    const ECDSA_KEY: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBMtPkKX8SOszgwcAyzKEYDjyg5xeCmZGv9aQa11v7RY8zn043Sb6mB6B15l+kUfNtOkffJ8tUtaqoj4bP1Hn6pk= ci@example";
    const ECDSA_FP: &str = "SHA256:6F9nCBAirUvYGJodR4JYpiVk6ciHhU6Z+tBUrNi3U98";

    /// Upstream stores none of algorithm, fingerprint or comment, so every
    /// field asserted here is derived locally from the public key. See
    /// `docs/schema.md` §1.1.
    #[test]
    fn describe_derives_metadata() {
        let ed25519 = || PublicKeyInfo {
            algorithm: Algorithm::Ed25519,
            fingerprint: ED25519_FP.to_owned(),
            comment: "laptop@example".to_owned(),
            bits: 256,
        };

        let cases = [
            ("ed25519 with comment", ED25519_KEY.to_owned(), ed25519()),
            (
                // ssh-keygen writes no trailing field when -C is empty.
                "rsa2048 without comment",
                RSA2048_KEY.to_owned(),
                PublicKeyInfo {
                    algorithm: Algorithm::Rsa,
                    fingerprint: RSA2048_FP.to_owned(),
                    comment: String::new(),
                    bits: 2048,
                },
            ),
            (
                "ecdsa nistp256",
                ECDSA_KEY.to_owned(),
                PublicKeyInfo {
                    algorithm: Algorithm::Ecdsa,
                    fingerprint: ECDSA_FP.to_owned(),
                    comment: "ci@example".to_owned(),
                    bits: 256,
                },
            ),
            (
                "surrounding whitespace is tolerated",
                format!("  {ED25519_KEY}\n"),
                ed25519(),
            ),
            // An item may legitimately have no public key yet; that is not an
            // error, it just yields unknown metadata.
            (
                "empty key is not an error",
                String::new(),
                PublicKeyInfo::default(),
            ),
            (
                "whitespace only is not an error",
                "   \n\t ".to_owned(),
                PublicKeyInfo::default(),
            ),
        ];

        for (name, public_key, want) in cases {
            assert_eq!(describe(&public_key).expect(name), want, "{name}");
        }
    }

    #[test]
    fn describe_rejects_malformed_keys() {
        for public_key in ["this is not a key", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA"] {
            assert!(describe(public_key).is_err(), "{public_key:?} parsed");
        }
    }

    /// Pins the fingerprints against the values `ssh-keygen -lf` produced for
    /// the same keys, so a change in the hashing or encoding would be caught.
    #[test]
    fn fingerprints_match_ssh_keygen() {
        for (public_key, want) in [
            (ED25519_KEY, ED25519_FP),
            (RSA2048_KEY, RSA2048_FP),
            (ECDSA_KEY, ECDSA_FP),
        ] {
            let info = describe(public_key).expect("valid test vector");
            assert_eq!(info.fingerprint, want);
        }
    }

    /// The comment is whatever bytes followed the key blob in the file it came
    /// from.
    #[test]
    fn describe_sanitizes_the_comment() {
        let body = ED25519_KEY.trim_end_matches(" laptop@example");

        let info = describe(&format!("{body} evil\u{1b}[2Jcomment")).expect("valid key");

        assert!(
            !info.comment.contains('\u{1b}'),
            "comment carries an escape: {:?}",
            info.comment
        );
        assert_eq!(info.algorithm, Algorithm::Ed25519);
    }

    #[test]
    fn set_public_key_populates_derived_metadata() {
        let mut key = Key {
            title: "laptop".to_owned(),
            ..Key::default()
        };

        key.set_public_key(ED25519_KEY).expect("valid key");

        assert_eq!(key.algorithm, Algorithm::Ed25519);
        assert_eq!(key.fingerprint, ED25519_FP);
        assert_eq!(key.comment, "laptop@example");
        assert_eq!(
            key.public_key, ED25519_KEY,
            "public key was not stored verbatim"
        );
    }

    #[test]
    fn a_bad_key_still_leaves_a_usable_row() {
        let mut key = Key {
            title: "broken".to_owned(),
            ..Key::default()
        };
        key.set_public_key(ED25519_KEY).expect("valid key");

        assert!(key.set_public_key("nonsense").is_err());

        // The error is reported, but the item must remain listable, and stale
        // metadata from the previous key must not survive.
        assert_eq!(key.title, "broken");
        assert_eq!(key.algorithm, Algorithm::Unknown);
        assert_eq!(key.fingerprint, "");
    }

    #[test]
    fn label() {
        let cases = [
            // RSA is the only algorithm whose size varies in practice, so it is
            // the only one that carries the bit count.
            (Algorithm::Rsa, 4096, "rsa4096"),
            (Algorithm::Rsa, 0, "rsa"),
            (Algorithm::Ed25519, 256, "ed25519"),
            (Algorithm::Unknown, 0, "unknown"),
        ];

        for (algorithm, bits, want) in cases {
            let key = Key {
                algorithm,
                bits,
                ..Key::default()
            };
            assert_eq!(key.label(), want);
        }
    }

    #[test]
    fn short_fingerprint() {
        let key = Key {
            fingerprint: ED25519_FP.to_owned(),
            ..Key::default()
        };
        assert_eq!(
            key.short_fingerprint(),
            ED25519_FP.trim_start_matches("SHA256:")
        );

        // A missing fingerprint must not gain a stray prefix.
        assert_eq!(Key::default().short_fingerprint(), "");
    }

    #[test]
    fn bit_length_counts_significant_bits() {
        assert_eq!(bit_length(&[]), 0);
        assert_eq!(bit_length(&[0, 0]), 0);
        assert_eq!(bit_length(&[0x01]), 1);
        assert_eq!(bit_length(&[0x00, 0x80, 0x00]), 16);
        assert_eq!(bit_length(&[0xff; 256]), 2048);
    }
}
