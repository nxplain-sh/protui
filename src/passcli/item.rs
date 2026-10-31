//! Vaults and SSH key items.

use std::fmt;

use zeroize::Zeroizing;

use crate::keys::{self, ItemId, Key, ShareId, Vault};
use crate::passcli::client::{Client, CommandError, Failure, PASSPHRASE_VARIABLE};
use crate::passcli::{parse, wire};

/// A generatable SSH key algorithm, matching `--key-type` upstream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyType {
    #[default]
    Ed25519,
    Rsa2048,
    Rsa4096,
}

impl KeyType {
    /// The generatable types in menu order, ed25519 first to match upstream's
    /// default.
    pub const ALL: [Self; 3] = [Self::Ed25519, Self::Rsa2048, Self::Rsa4096];

    /// Returns the name upstream accepts for `--key-type`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::Rsa2048 => "rsa2048",
            Self::Rsa4096 => "rsa4096",
        }
    }
}

/// A key to generate.
#[derive(Default)]
pub struct GenerateRequest {
    pub title: String,
    pub share_id: ShareId,
    /// Stored inside the public key line itself; empty for none.
    pub comment: String,
    pub key_type: KeyType,
    /// Empty for none. The only secret protui ever sends: it is never placed
    /// in argv, and the buffer is zeroed when the request is dropped.
    pub passphrase: Zeroizing<String>,
}

impl fmt::Debug for GenerateRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenerateRequest")
            .field("title", &self.title)
            .field("share_id", &self.share_id)
            .field("comment", &self.comment)
            .field("key_type", &self.key_type)
            .field(
                "passphrase",
                &if self.passphrase.is_empty() {
                    "<none>"
                } else {
                    "<redacted>"
                },
            )
            .finish()
    }
}

/// Attaches a value to its flag.
///
/// Joined with `=`, a value beginning with `-` — a title, say — is read as the
/// value rather than as another flag.
fn flag(name: &str, value: &str) -> String {
    format!("--{name}={value}")
}

impl Client {
    /// Lists every vault the session can see.
    pub async fn vaults(&self) -> Result<Vec<Vault>, CommandError> {
        const COMMAND: &str = "vault list";

        let stdout = self
            .call(COMMAND, &["vault", "list", "--output=json"], &[])
            .await?;

        parse::vault_list(&stdout).map_err(|failure| CommandError::new(COMMAND, failure))
    }

    /// Lists the active SSH key items in one vault.
    ///
    /// pass-cli cannot list across vaults — it errors without a vault
    /// selector — so callers fan out per vault. See `docs/schema.md` §1.2.
    pub async fn ssh_keys(&self, vault: &Vault) -> Result<Vec<Key>, CommandError> {
        const COMMAND: &str = "item list";

        let args = [
            "item",
            "list",
            &flag("share-id", vault.share_id.as_str()),
            &flag("filter-type", wire::FILTER_TYPE_SSH_KEY),
            "--filter-state=active",
            "--output=json",
        ];
        let stdout = self.call(COMMAND, &args, &[]).await?;

        parse::item_list(&stdout, vault).map_err(|failure| CommandError::new(COMMAND, failure))
    }

    /// Fetches one item's public key.
    ///
    /// This uses the field-scoped read, which prints a single field as bare
    /// text. The alternative — `item view --output json` — returns the private
    /// key unredacted and is never used. See `docs/schema.md` §1.4.
    pub async fn public_key(
        &self,
        share_id: &ShareId,
        item_id: &ItemId,
    ) -> Result<String, CommandError> {
        let args = [
            "item",
            "view",
            &flag("share-id", share_id.as_str()),
            &flag("item-id", item_id.as_str()),
            "--field=public_key",
        ];
        let stdout = self.call("item view", &args, &[]).await?;

        Ok(keys::sanitize(String::from_utf8_lossy(&stdout).trim()))
    }

    /// Creates a new SSH key item and returns its item id.
    ///
    /// On success upstream prints the bare item id on stdout with no JSON
    /// envelope.
    ///
    /// Passphrase handling deserves care. Upstream checks
    /// `PROTON_PASS_SSH_KEY_PASSWORD` before it consults `--password`, and
    /// falls back to an interactive TTY prompt if `--password` is passed with
    /// no variable set — which inside a TUI would contend with it for the
    /// terminal. Setting the variable and omitting the flag avoids the prompt
    /// entirely.
    ///
    /// The request, and with it protui's only copy of the passphrase, is
    /// consumed and zeroed on return. The standard library keeps its own copy
    /// of the child environment while spawning, which cannot be wiped; it is
    /// freed once the child has started. Nothing is written to argv, disk, or
    /// logs.
    pub async fn generate(&self, request: GenerateRequest) -> Result<ItemId, CommandError> {
        const COMMAND: &str = "item create ssh-key generate";

        if request.title.is_empty() {
            return Err(CommandError::new(
                COMMAND,
                Failure::Invalid("title is required"),
            ));
        }
        if request.share_id.is_empty() {
            return Err(CommandError::new(
                COMMAND,
                Failure::Invalid("vault is required"),
            ));
        }

        let title = flag("title", &request.title);
        let share_id = flag("share-id", request.share_id.as_str());
        let key_type = flag("key-type", request.key_type.as_str());
        let comment = flag("comment", &request.comment);

        let mut args = vec![
            "item", "create", "ssh-key", "generate", &title, &share_id, &key_type,
        ];
        if !request.comment.is_empty() {
            args.push(&comment);
        }

        let mut env = Vec::new();
        if !request.passphrase.is_empty() {
            env.push((PASSPHRASE_VARIABLE, request.passphrase.as_str()));
        }

        let stdout = self.call(COMMAND, &args, &env).await?;

        let item_id = String::from_utf8_lossy(&stdout).trim().to_owned();
        if item_id.is_empty() {
            return Err(CommandError::new(
                COMMAND,
                Failure::Schema("no item id on stdout".to_owned()),
            ));
        }

        Ok(ItemId::new(item_id))
    }

    /// Moves an item to the trash, where `pass-cli item untrash` can restore
    /// it.
    pub async fn trash(&self, share_id: &ShareId, item_id: &ItemId) -> Result<(), CommandError> {
        let args = [
            "item",
            "trash",
            &flag("share-id", share_id.as_str()),
            &flag("item-id", item_id.as_str()),
        ];
        self.call("item trash", &args, &[]).await?;

        Ok(())
    }

    /// Permanently destroys an item.
    ///
    /// There is no undo and upstream does not confirm; the confirmation is
    /// protui's responsibility.
    pub async fn delete(&self, share_id: &ShareId, item_id: &ItemId) -> Result<(), CommandError> {
        let args = [
            "item",
            "delete",
            &flag("share-id", share_id.as_str()),
            &flag("item-id", item_id.as_str()),
        ];
        self.call("item delete", &args, &[]).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(passphrase: &str) -> GenerateRequest {
        GenerateRequest {
            title: "laptop".to_owned(),
            share_id: ShareId::from("share-1"),
            passphrase: Zeroizing::new(passphrase.to_owned()),
            ..GenerateRequest::default()
        }
    }

    /// The load-bearing security test: anything in argv is readable by any
    /// user on the box via `ps`, so the passphrase must travel in the
    /// environment instead.
    #[tokio::test]
    async fn the_passphrase_never_reaches_argv() {
        const SECRET: &str = "correct-horse-battery-staple";
        let client = Client::fake("new-item-id\n");

        let item_id = client
            .generate(request(SECRET))
            .await
            .expect("generate succeeds");

        assert_eq!(item_id, ItemId::from("new-item-id"));
        let (args, env) = client.recorded();
        for arg in &args {
            assert!(
                !arg.contains(SECRET),
                "passphrase leaked into argv: {arg:?}"
            );
        }
        // It must instead be exactly one environment entry.
        assert_eq!(env, [(PASSPHRASE_VARIABLE.to_owned(), SECRET.to_owned())]);
    }

    /// `--password` with no variable set drops into a TTY read that would
    /// contend with the UI for the terminal. Setting the variable and
    /// omitting the flag avoids the prompt entirely. See `docs/schema.md`
    /// §6.1.
    #[tokio::test]
    async fn the_password_flag_is_never_passed() {
        for passphrase in ["", "secret"] {
            let client = Client::fake("id\n");

            client
                .generate(request(passphrase))
                .await
                .expect("generate succeeds");

            let (args, _) = client.recorded();
            assert!(
                !args.iter().any(|arg| arg.starts_with("--password")),
                "--password passed (passphrase {passphrase:?}); it would prompt on the TTY"
            );
        }
    }

    #[tokio::test]
    async fn no_passphrase_means_no_environment() {
        let client = Client::fake("id\n");

        client
            .generate(request(""))
            .await
            .expect("generate succeeds");

        assert_eq!(client.recorded().1, []);
    }

    #[test]
    fn the_passphrase_is_redacted_from_debug_output() {
        let debug = format!("{:?}", request("hunter2"));

        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    #[tokio::test]
    async fn generate_defaults_to_ed25519_and_omits_an_empty_comment() {
        let client = Client::fake("id\n");

        client
            .generate(request(""))
            .await
            .expect("generate succeeds");

        let (args, _) = client.recorded();
        assert!(args.contains(&"--key-type=ed25519".to_owned()), "{args:?}");
        assert!(
            !args.iter().any(|arg| arg.starts_with("--comment")),
            "{args:?}"
        );
    }

    /// A title beginning with `-` must arrive as the title, not as a flag.
    #[tokio::test]
    async fn values_are_attached_to_their_flags() {
        let client = Client::fake("id\n");
        let hostile = GenerateRequest {
            title: "--share-id=other".to_owned(),
            comment: "-x".to_owned(),
            ..request("")
        };

        client.generate(hostile).await.expect("generate succeeds");

        let (args, _) = client.recorded();
        assert!(
            args.contains(&"--title=--share-id=other".to_owned()),
            "{args:?}"
        );
        assert!(args.contains(&"--comment=-x".to_owned()), "{args:?}");
        assert!(
            !args.iter().any(|arg| arg == "--share-id=other"),
            "{args:?}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_empty_required_fields() {
        let client = Client::fake("id\n");

        let untitled = GenerateRequest {
            title: String::new(),
            ..request("")
        };
        assert!(
            client.generate(untitled).await.is_err(),
            "a missing title was accepted"
        );

        let homeless = GenerateRequest {
            share_id: ShareId::default(),
            ..request("")
        };
        assert!(
            client.generate(homeless).await.is_err(),
            "a missing vault was accepted"
        );
    }

    #[tokio::test]
    async fn empty_stdout_from_generate_is_a_schema_error() {
        let error = Client::fake("  \n")
            .generate(request(""))
            .await
            .expect_err("no id");

        assert!(matches!(error.failure(), Failure::Schema(_)), "{error:?}");
    }

    /// Pins the read path away from `item view --output json`, which returns
    /// the private key unredacted. See `docs/schema.md` §1.4.
    #[tokio::test]
    async fn public_keys_use_the_field_scoped_read() {
        let client = Client::fake("ssh-ed25519 AAAAC3Nz... laptop@example\n");

        let public_key = client
            .public_key(&ShareId::from("share-1"), &ItemId::from("item-1"))
            .await
            .expect("read succeeds");

        assert_eq!(public_key, "ssh-ed25519 AAAAC3Nz... laptop@example");
        let (args, _) = client.recorded();
        assert!(args.contains(&"--field=public_key".to_owned()), "{args:?}");
        assert!(
            !args.iter().any(|arg| arg.starts_with("--output")),
            "{args:?}"
        );
        assert!(!args.iter().any(|arg| arg.contains("private")), "{args:?}");
    }

    #[tokio::test]
    async fn ssh_keys_filter_server_side() {
        let client = Client::fake(r#"{"items":[]}"#);
        let vault = Vault {
            share_id: ShareId::from("share-1"),
            ..Vault::default()
        };

        client.ssh_keys(&vault).await.expect("listing succeeds");

        // The CLI flag spelling is kebab-case while the JSON value is
        // snake_case; they are deliberately separate constants.
        let (args, _) = client.recorded();
        assert!(
            args.contains(&"--filter-type=ssh-key".to_owned()),
            "{args:?}"
        );
        assert!(args.contains(&"--share-id=share-1".to_owned()), "{args:?}");
    }

    /// Sweeps every wrapper: each must put its subcommand into argv, not only
    /// into the error label, and none may ask for secrets.
    #[tokio::test]
    async fn every_subcommand_reaches_argv_and_none_requests_secrets() {
        let vault = Vault {
            share_id: ShareId::from("share-1"),
            ..Vault::default()
        };
        let share = ShareId::from("s");
        let item = ItemId::from("i");

        let client = Client::fake(r#"{"vaults":[],"items":[]}"#);
        let mut cases: Vec<(&[&str], Vec<String>)> = Vec::new();

        client.vaults().await.expect("vaults");
        cases.push((&["vault", "list"], client.recorded().0));
        client.ssh_keys(&vault).await.expect("ssh keys");
        cases.push((&["item", "list"], client.recorded().0));
        client.public_key(&share, &item).await.expect("public key");
        cases.push((&["item", "view"], client.recorded().0));
        client.trash(&share, &item).await.expect("trash");
        cases.push((&["item", "trash"], client.recorded().0));
        client.delete(&share, &item).await.expect("delete");
        cases.push((&["item", "delete"], client.recorded().0));
        client.agent_status().await.expect("agent status");
        cases.push((&["ssh-agent", "daemon", "status"], client.recorded().0));
        client.start_agent().await.expect("agent start");
        cases.push((&["ssh-agent", "daemon", "start"], client.recorded().0));
        client.stop_agent().await.expect("agent stop");
        cases.push((&["ssh-agent", "daemon", "stop"], client.recorded().0));
        client.generate(request("")).await.expect("generate");
        cases.push((
            &["item", "create", "ssh-key", "generate"],
            client.recorded().0,
        ));

        for (want, args) in cases {
            let want: Vec<String> = want.iter().map(|&arg| arg.to_owned()).collect();
            assert!(args.starts_with(&want), "{args:?} should start {want:?}");
            assert!(!args.iter().any(|arg| arg == "--show-secrets"), "{args:?}");
        }
    }

    #[tokio::test]
    async fn trash_and_delete_are_distinct_commands() {
        let client = Client::fake("");
        let (share, item) = (ShareId::from("s"), ItemId::from("i"));

        client.trash(&share, &item).await.expect("trash");
        assert_eq!(client.recorded().0[1], "trash");

        client.delete(&share, &item).await.expect("delete");
        assert_eq!(client.recorded().0[1], "delete");
    }
}
