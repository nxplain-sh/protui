package passcli

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/nxplain-sh/protui/internal/keys"
)

// The parsers below are pure functions over bytes so they can be exercised
// against fixtures without running pass-cli. Every one of them tolerates
// unknown JSON keys (encoding/json ignores them by default; DisallowUnknownFields
// is deliberately not used) so that an upstream addition cannot break the list.
//
// The inverse risk — a renamed or removed field decoding silently to its zero
// value — is caught by validating that required fields are non-empty.

// parseVaultList decodes `vault list --output json`.
func parseVaultList(stdout []byte) ([]keys.Vault, error) {
	var response vaultListResponse
	if err := json.Unmarshal(stdout, &response); err != nil {
		return nil, fmt.Errorf("decode vault list: %w", err)
	}

	vaults := make([]keys.Vault, 0, len(response.Vaults))

	for index, entry := range response.Vaults {
		// share_id is the handle every item subcommand needs; without it the
		// vault is unusable, so an empty one is a schema break, not a quirk.
		if entry.ShareID == "" {
			return nil, fmt.Errorf("vault at index %d has no share_id: %w", index, ErrUnexpectedSchema)
		}

		vaults = append(vaults, keys.Vault{
			// Vault names are user-authored and can arrive from a shared
			// vault, so they are sanitised before anything renders them.
			Name:    keys.Sanitize(entry.Name),
			ShareID: entry.ShareID,
			VaultID: entry.VaultID,
		})
	}

	return vaults, nil
}

// parseItemList decodes `item list --output json` and keeps only SSH key items.
//
// --filter-type is applied client-side by upstream, so it is a bandwidth
// optimisation rather than a guarantee; the item_type check is repeated here.
//
// vault supplies the human-readable vault name, which the item output does not
// carry. Public key metadata is not available from this call and is filled in
// later by fetching the public key per item.
func parseItemList(stdout []byte, vault keys.Vault) ([]keys.Key, error) {
	var response itemListResponse
	if err := json.Unmarshal(stdout, &response); err != nil {
		return nil, fmt.Errorf("decode item list: %w", err)
	}

	result := make([]keys.Key, 0, len(response.Items))

	for index, item := range response.Items {
		if item.ItemType != itemTypeSSHKey {
			continue
		}

		if item.ID == "" {
			return nil, fmt.Errorf("item at index %d has no id: %w", index, ErrUnexpectedSchema)
		}
		if item.Title == "" {
			return nil, fmt.Errorf("item %s has no title: %w", item.ID, ErrUnexpectedSchema)
		}

		// Upstream echoes share_id per item, but it is the same vault we asked
		// about; prefer our own so a blank one cannot orphan the item.
		shareID := item.ShareID
		if shareID == "" {
			shareID = vault.ShareID
		}

		result = append(result, keys.Key{
			ID:        item.ID,
			ShareID:   shareID,
			VaultID:   item.VaultID,
			VaultName: vault.Name,
			// Titles are user-authored and shareable: untrusted display input.
			Title:      keys.Sanitize(item.Title),
			State:      parseState(item.State),
			Algorithm:  keys.AlgorithmUnknown,
			CreatedAt:  item.CreateTime.Time,
			ModifiedAt: item.ModifyTime.Time,
		})
	}

	return result, nil
}

// parseState maps upstream's PascalCase state onto the domain enum. An
// unrecognised state is reported as active so the item stays visible and
// actionable rather than silently vanishing.
func parseState(raw string) keys.State {
	switch raw {
	case stateTrashed:
		return keys.StateTrashed
	case stateActive:
		return keys.StateActive
	default:
		return keys.StateActive
	}
}

// parseAgentStatus reads `ssh-agent daemon status`, which has no JSON output
// and must be line-parsed. See docs/schema.md §7.
//
// The command exits 0 whether or not the daemon runs, so state comes only from
// stdout. Values are matched by prefix because the parenthetical suffixes are
// diagnostic prose and will change.
func parseAgentStatus(stdout []byte) AgentStatus {
	status := AgentStatus{State: AgentUnknown}

	for _, line := range strings.Split(string(stdout), "\n") {
		key, value, found := strings.Cut(line, ":")
		if !found {
			continue
		}

		// "PID file" must not match "PID": compare the whole trimmed key.
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)

		switch key {
		case "Status":
			// Classify on the raw value, display the sanitised one.
			status.State = agentStateOf(value)
			status.Detail = keys.Sanitize(value)
		case "PID":
			status.PID = keys.Sanitize(value)
		case "Socket":
			status.Socket = keys.Sanitize(value)
		}
	}

	return status
}

// agentStateOf classifies the Status: value by prefix.
func agentStateOf(value string) AgentState {
	switch {
	case strings.HasPrefix(value, "running"):
		return AgentRunning
	case strings.HasPrefix(value, "degraded"):
		return AgentDegraded
	case strings.HasPrefix(value, "stopped"):
		return AgentStopped
	default:
		return AgentUnknown
	}
}
