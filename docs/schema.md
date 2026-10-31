# `pass-cli` output schema (as consumed by protui)

> **Status:** verified against upstream source **and** a live binary.
> **Upstream:** `protonpass/pass-cli` @ `51a4c9b110a0ffe6e81f4f5d3877b9e5a0c24112`
> **Binary:** `Proton Pass CLI 2.3.3` (Homebrew, darwin/arm64)
> **Captured:** 2026-08-31

This is **not a stable public contract.** Upstream serialises its internal Rust
domain structs directly with `serde`, so any field rename in `pass-domain`
silently changes the JSON. Diff this file against a fresh capture when bumping
the supported `pass-cli` version.

Method: read the Rust structs (authoritative for field names, optionality and
serde attributes), then confirm the wire shape by running the real binary
against a live session. Where the two disagree, the binary wins and the
discrepancy is noted.

---

## 1. Findings that contradict the v1 brief

These invalidate assumptions in the original spec. **Read before implementing.**

### 1.1 There is no algorithm, fingerprint, or comment field

`SshKeyItem` (`pass-domain/src/models/item/mod.rs:1529-1534`) is, in full:

```rust
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SshKeyItem {
    pub private_key: String,
    pub public_key: String,
    pub sections: Vec<CustomSection>,
}
```

Three fields. That is the entire type.

- **Algorithm** — not stored. Must be derived by parsing the `public_key`
  string (the `ssh-ed25519` / `ssh-rsa` prefix, or properly via the
  [`ssh-key`](https://crates.io/crates/ssh-key) crate's
  `PublicKey::from_openssh` → `key_data()`).
- **Fingerprint** — not stored. Must be computed locally:
  `PublicKey::fingerprint(HashAlg::Sha256)` over the parsed public key.
- **Comment** — not stored as a field. `--comment` on `create ssh-key generate`
  is baked into the _trailing comment of the OpenSSH public key line itself_
  and round-trips only there. Recover it as the third whitespace-delimited
  field of `public_key` (`PublicKey::comment()` returns it).

**Consequence:** protui's list columns (algorithm, fingerprint) and the detail
pane's comment are all **locally derived from `public_key`**, not read from
JSON. `src/keys` owns that derivation. If `public_key` is empty or
unparseable, those columns must degrade to a placeholder rather than error the
whole row.

### 1.2 `item list` cannot list across vaults

`ListItemsQuery::new` (`pass-cli/src/commands/item/list.rs`) errors unless
exactly one of `--share-id` / `--vault-name` is given. Confirmed live:

```console
$ pass-cli item list --output json
Error: Please provide either --share-id, --vault-name, or set a default vault
with 'pass-cli settings set default-vault'
$ echo $?
1
```

**Consequence:** "all SSH keys across vaults" is a **fan-out in protui**:
`vault list` once, then one `item list --share-id <id> --filter-type ssh-key`
per vault. Partial failure must be per-vault (one unreachable vault should not
blank the list).

### 1.3 A trash _does_ exist

The brief states deletion is permanent with no trash. Half right:

- `item delete --share-id --item-id` — **permanent**, no confirmation prompt.
- `item trash` — **exists**, recoverable, and accepts `--item-title` too.
- `item untrash` — restores.

`item list --filter-state trashed` lists trashed items.

**Consequence:** v1 exposes both, on separate keys. `d` trashes, which is
recoverable; `D` permanently deletes and requires typing the item title to
confirm, since nothing can undo it.

### 1.4 `item view --output json` returns the private key

`view.rs:154-157` serialises the whole `Item` — including
`content.content.SshKey.private_key` — with no redaction.

**Consequence:** protui must **never** call `item view --output json` for SSH
key items. Use the field-scoped form, which prints one field as bare text:

```console
$ pass-cli item view --share-id <id> --item-id <id> --field public_key
ssh-ed25519 AAAAC3Nza... comment
```

Accepted field aliases (`pass-domain/src/models/item/field.rs:355-366`):
`private_key`, `private key`, `public_key`, `public key`. protui uses
**`public_key` only** and has no code path that names the private one.

### 1.5 `--show-secrets` is a footgun we do not use

`item list --show-secrets` swaps the redacted summary for full `Item` structs
(private keys included). It requires `--output json` and is rejected under an
agent session. protui never passes it. The default summary is safe by
construction — upstream comment at `list.rs:60`:

> `// Fields here must never carry user-provided secret material (no content, note, extra_fields).`

---

## 2. `vault list --output json`

Source: `pass-cli/src/commands/vault/list.rs:25-45`. Verified live.

```json
{
  "vaults": [
    {
      "name": "Personal",
      "vault_id": "1X7ux-Rbi...Q==",
      "share_id": "9U5eHxCQ5...vA=="
    }
  ]
}
```

| Field      | Type   | Optional | Notes                                   |
| ---------- | ------ | -------- | --------------------------------------- |
| `name`     | string | no       | user-supplied vault name, not unique    |
| `vault_id` | string | no       | base64-ish, contains `-` `_` `=`        |
| `share_id` | string | no       | **this is the id every item cmd wants** |

`share_id` is the vault handle for all `item` subcommands. `vault_id` is not
accepted anywhere in the item surface — carry it, but never pass it.

---

## 3. `item list --output json` (summary form — what protui uses)

Source: `ItemSummary`, `pass-cli/src/commands/item/list.rs:61-90`.

Verified live (values redacted, shape verbatim):

```json
{
  "items": [
    {
      "id": "<id>",
      "share_id": "<share>",
      "vault_id": "<vault>",
      "state": "Active",
      "flags": [],
      "create_time": "2026-01-23T19:48:23",
      "modify_time": "2026-01-23T19:48:23",
      "title": "<redacted>",
      "item_type": "login"
    }
  ]
}
```

| Field         | JSON type | Optional          | Notes                                      |
| ------------- | --------- | ----------------- | ------------------------------------------ |
| `id`          | string    | no                | item id; newtype `ItemId(String)`          |
| `share_id`    | string    | no                | newtype                                    |
| `vault_id`    | string    | no                | newtype                                    |
| `state`       | string    | no                | `"Active"` \| `"Trashed"` — **PascalCase** |
| `flags`       | [string]  | no (may be `[]`)  | PascalCase, see below                      |
| `create_time` | string    | no                | **naive local datetime, no zone**          |
| `modify_time` | string    | no                | same                                       |
| `folder_id`   | string    | **yes — omitted** | `skip_serializing_if = "Option::is_none"`  |
| `title`       | string    | no                | user-supplied                              |
| `item_type`   | string    | no                | **snake_case**, see below                  |

Note the envelope is `{"items": [...]}`, never a bare array. An `internal`
build feature can add a sibling `"folders"` key — the release binary omits it,
but the decoder must tolerate it (as it must tolerate any unknown key).

### 3.1 Casing is inconsistent between fields — this is the main parsing trap

Three different conventions in one object, because each Rust type derives
`Serialize` differently:

- `item_type` → **snake_case** (`#[serde(rename_all = "snake_case")]` on the
  local `ItemType` enum, `list.rs:34`): `note`, `login`, `alias`,
  `credit_card`, `identity`, **`ssh_key`**, `wifi`, `custom`.
- `state` → **PascalCase** (no rename attr on `ItemState`,
  `models/item/mod.rs:46`): `Active`, `Trashed`. The `= 1` / `= 2`
  discriminants do **not** reach the JSON; serde emits variant names.
- `flags` → **PascalCase** (no rename attr, `models/item/flags.rs:20`):
  `SkipHealthCheck`, `EmailBreached`, `AliasDisabled`, `ItemHasFiles`,
  `ItemHasHadFiles`. Serialised as an array of names, not the bitmask.

Do not normalise these by guessing. `src/passcli` decodes them literally and
maps them to the domain enums in `src/keys`.

### 3.2 Timestamps are civil (zoneless) — an RFC 3339 parser will not accept them

`create_time` / `modify_time` are `jiff::civil::DateTime`, which serialises
without offset or `Z`:

```
2026-01-23T19:48:23
```

The layout is `YYYY-MM-DDTHH:MM:SS`, parsed by `CivilDateTime` in `src/keys`;
an RFC 3339 parser rejects it for the missing offset. There is no zone
information at all, so the
value cannot be converted to an absolute instant — treat it as wall-clock and
render it as-is. protui uses these only for display/sort, never arithmetic.

### 3.3 Filtering

`--filter-type ssh-key` — note the CLI spelling is **kebab-case** (`ssh-key`)
while the JSON value is **snake_case** (`ssh_key`). Do not reuse one constant
for both.

Accepted `--filter-type`: `note`, `login`, `alias`, `credit-card`, `identity`,
`ssh-key`, `wifi`, `custom`. Accepted `--filter-state`: `active`, `trashed`.

Server-side filtering is a convenience only — it is applied client-side in
`list.rs` after fetching. protui still passes it (smaller output, less to
decode) but must re-check `item_type == "ssh_key"` defensively.

---

## 4. `item list --show-secrets` (full form — documented, **not used**)

Recorded so a future reader knows what we are declining to parse. With
`--show-secrets --output json`, `items[]` becomes full `Item` structs
(`models/item/mod.rs:63-75`, `ItemData` at `:100-109`):

```json
{
  "items": [
    {
      "id": "...",
      "share_id": "...",
      "vault_id": "...",
      "content": {
        "title": "...",
        "note": "",
        "item_uuid": "...",
        "content": {
          "SshKey": {
            "private_key": "...",
            "public_key": "...",
            "sections": []
          }
        },
        "extra_fields": [
          { "name": "Passphrase", "content": { "Hidden": "..." } }
        ]
      },
      "state": "Active",
      "flags": [],
      "create_time": "...",
      "modify_time": "..."
    }
  ]
}
```

`ItemContent` is an **externally tagged** enum (no `#[serde(tag)]`), so the
variant name is the wrapping key and it is **PascalCase**: `{"SshKey": {...}}`,
not `{"ssh_key": ...}`. Same for `ItemExtraFieldContent`: `{"Text": "..."}`,
`{"Hidden": "..."}`, `{"Totp": "..."}`, `{"Timestamp": 1737661703}`.

Note the passphrase, when set, is stored as a **hidden extra field literally
named `"Passphrase"`** (`pass/src/item/create/ssh_key.rs:46-51`) — it is not
part of `SshKeyItem`. protui never reads `extra_fields`.

---

## 5. `item view`

```
pass-cli item view [--share-id <id> --item-id <id> | --vault-name <n> --item-title <t>]
                   [--field <FIELD>] [--output human|json]
pass-cli item view pass://SHARE_ID/ITEM_ID[/FIELD]
```

Two mutually exclusive addressing modes; mixing a URI with any id flag is an
error. Resolution by `--item-title` is by exact title and ambiguous if titles
repeat — protui always addresses by `--share-id` + `--item-id`.

- **With `--field`** — prints the single field value as **bare text plus a
  newline**. Not JSON, not quoted. This is the only read path protui uses.
- **Without `--field`, `--output json`** — full `Item` including
  `private_key`. See §1.4. Not used.

---

## 6. `item create ssh-key`

```
pass-cli item create ssh-key generate --title <TITLE>
    [--comment <COMMENT>]
    [--key-type ed25519|rsa2048|rsa4096]   # default: ed25519
    [--password]
    [--share-id <ID> | --vault-name <NAME>]

pass-cli item create ssh-key import --from-private-key <FILE> --title <TITLE>
    [--password] [--share-id <ID> | --vault-name <NAME>]
```

**Output on success is the bare item id on stdout** (`create/ssh_key.rs:230`
and `:271`) — `println!("{item_id}")`. No JSON, no `--output` flag on this
subcommand. Trim the trailing newline and treat the whole line as the id.

### 6.1 Passphrase handling — env var wins, and it wins _before_ `--password`

`get_ssh_key_password` (`create/ssh_key.rs:274-313`) resolves in this order:

1. `PROTON_PASS_SSH_KEY_PASSWORD` — used if set, **regardless of whether
   `--password` was passed**.
2. `PROTON_PASS_SSH_KEY_PASSWORD_FILE` — file path; contents are `trim()`ed.
3. If `--password` was **not** passed → no passphrase.
4. Otherwise → **interactive TTY prompt** (twice, with confirmation, looping
   until they match, for `generate`).

Both env var names in the brief are confirmed to exist verbatim.

**Consequence for a TUI:** step 4 is fatal — a child process prompting on the
shared TTY will fight the UI for the terminal and hang. protui must
**never pass `--password` without also setting the env var** in the child's
environment. Setting the var and omitting the flag is sufficient and is the
path v1 takes. The passphrase is written to the child env only, is never an
argv element (§8), and protui's buffer is zeroed once the child has it.

Note upstream writes `Reading password from environment variable ...` to
**stderr** in that case — stderr is therefore not a reliable failure signal on
its own; use the exit code.

---

## 7. `ssh-agent` — no JSON anywhere

```
pass-cli ssh-agent start | load | debug | daemon
pass-cli ssh-agent daemon start | status | stop
```

`daemon status` (`ssh_agent/daemon.rs:169-240`) is **human-readable text only**
— there is no `--output` flag. It must be line-parsed. Exhaustive set of
first-line forms:

| Condition                      | First line                                                      |
| ------------------------------ | --------------------------------------------------------------- |
| no PID file                    | `Status:   stopped`                                             |
| process alive + socket present | `Status:   running`                                             |
| process alive + socket missing | `Status:   degraded (process is running but socket is missing)` |
| process dead + socket present  | `Status:   stopped (process died, stale socket file present)`   |
| process dead + socket missing  | `Status:   stopped`                                             |

Whitespace between `Status:` and the value is padding, not a single space.
Parse as: split on the first `:`, trim both sides, then match a **prefix** of
the value (`running` / `degraded` / `stopped`) — the parenthetical suffixes are
diagnostic prose and will change.

Subsequent lines, when present: `PID:`, `Socket:`, `PID file:`, `Log file:`,
then `Last N log line(s):` followed by indented log lines. protui surfaces
`PID` and `Socket` when parsed and ignores the rest.

**`daemon status` exits 0 whether or not the daemon is running.** Status must
be read from stdout, never inferred from the exit code.

`PROTON_PASS_SSH_DAEMON_PIDFILE` overrides the PID file location; if set in
protui's own environment it must be forwarded to the child unchanged.

---

## 8. Process-level contract

- **Exit codes** are the only reliable success signal: `0` ok, `1` on error
  (verified: missing vault selector → 1, nonexistent vault → 1, success → 0).
  There are no distinguishing non-zero codes; error _kind_ must come from
  stderr text, so protui treats any non-zero as a failure of the named command
  and surfaces the stderr tail verbatim.
- **Errors print to stderr** prefixed `Error: `, human-prose, unstructured, and
  are not stable. Never pattern-match them to drive control flow. The one
  exception is the startup auth probe (§9), which is advisory.
- **stdout is not pure JSON** even with `--output json` — see the stderr note
  in §6.1, and upstream may emit update-check chatter. Decode stdout only, and
  keep stderr separate for the error message.
- **Secrets never go in argv.** Anything in argv is world-readable via `ps`.
  Passphrases go through the child environment (§6.1). Titles, comments, ids
  and vault names are not secret and may be argv. Every argument is passed as a
  distinct argv element — no shell, no interpolation, so no quoting concerns —
  and every value is joined to its flag as `--flag=value`, so one beginning
  with `-` is never parsed as a flag. clap, which `pass-cli` uses, accepts that
  form for every long option (verified against 2.5.1:
  `--share-id=--bogus` is rejected by the server as a malformed id, not by the
  parser as an unknown flag).
- **`PROTON_PASS_NO_UPDATE_CHECK`** should be set on every child invocation to
  suppress the auto-update probe, which otherwise adds latency and can write
  unexpected lines.

---

## 9. Startup preflight

Two independent checks, in order:

1. **Binary present** — a search of the absolute `PATH` entries for
   `pass-cli`. If absent, tell the user to install it; do not start the TUI.
2. **Session valid** — run `pass-cli info`. Exit 0 with output means a live
   session. Verified shape (human, no JSON flag on this command):

   ```
   - Release track: stable
   - ID: <base64>
   - Username: <name>
   - Email: <email>
   - Install source: Homebrew
   - Session has lock: no
   ```

   protui only checks the exit code and does not parse these lines. On failure,
   exit with a message naming `pass-cli login`.

`Session has lock: yes` indicates a locked session; unlocking is interactive
and out of scope for v1 — such a session will surface as a command failure with
upstream's own stderr text.

---

## 10. Unknown fields

Every decoder tolerates unknown JSON keys — serde ignores them by default and
protui does **not** set `deny_unknown_fields`. A new upstream field must never
break the list. Conversely, a _removed_ or _renamed_ field decodes to its
default silently, which is the real risk: `src/passcli`
validates that required fields (`id`, `share_id`, `title`) are non-empty after
decode and returns a named error identifying the command that produced the
unexpected shape.

---

## 11. Re-verifying after an upstream bump

```sh
pass-cli --version
pass-cli vault list --output json | jq '.vaults[0]'
pass-cli item list --vault-name <V> --filter-type ssh-key --output json \
  | jq '.items[0] | .title="<redacted>"'
pass-cli ssh-agent daemon status
```

Compare against §2, §3 and §7. The fields most likely to drift are the casing
conventions in §3.1 and the `Status:` prose in §7.

## 12. Open / unverified

- **No SSH key items existed in the live account at capture time**, so §3's
  shape was confirmed against a `login` item. `ItemSummary` is built from
  `&Item` independently of the content variant (`list.rs:74-90`), so the only
  content-dependent field is `item_type`, whose `ssh_key` spelling is read
  from `list.rs:34-41` rather than observed. Re-confirm once a key exists.
- §4's full-item shape is derived from the structs only; deliberately not
  exercised, since doing so would print a private key.
- `item create ssh-key generate` output is read from source (`:230`, `:271`);
  not run, to avoid writing to the live account.
