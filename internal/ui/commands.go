package ui

import (
	"context"
	"time"

	"github.com/atotto/clipboard"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

// Every pass-cli call runs inside a tea.Cmd and reports back as a message, so
// the update loop itself stays free of I/O.

// commandTimeout bounds any single pass-cli invocation. Network-backed calls
// occasionally stall; without this the UI would hang with no way out.
const commandTimeout = 30 * time.Second

// Messages returned by the commands below.
type (
	vaultsLoadedMsg struct {
		vaults []keys.Vault
		err    error
	}

	keysLoadedMsg struct {
		vault keys.Vault
		keys  []keys.Key
		err   error
	}

	// publicKeyLoadedMsg carries the public key for one item. Algorithm,
	// fingerprint and comment are derived from it, since upstream stores none
	// of them. err is non-fatal: the row stays listed with unknown metadata.
	publicKeyLoadedMsg struct {
		shareID   string
		itemID    string
		publicKey string
		err       error
	}

	agentStatusMsg struct {
		status passcli.AgentStatus
		err    error
	}

	// agentActionMsg reports a completed start/stop. The caller re-reads the
	// status afterwards rather than assuming the action took effect.
	agentActionMsg struct {
		action string
		err    error
	}

	itemCreatedMsg struct {
		title string
		err   error
	}

	itemRemovedMsg struct {
		title     string
		permanent bool
		err       error
	}

	copiedMsg struct {
		title string
		err   error
	}

	// statusExpiredMsg clears a transient status line.
	statusExpiredMsg struct{ token int }
)

// loadVaults fetches the vault list, which is the entry point for everything
// else: pass-cli cannot list items without a vault selector.
func loadVaults(client *passcli.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		vaults, err := client.Vaults(ctx)

		return vaultsLoadedMsg{vaults: vaults, err: err}
	}
}

// loadKeys fetches one vault's SSH key items.
//
// Upstream has no cross-vault listing, so the caller fans out over vaults and
// each result is reported independently: one unreachable vault must not blank
// the whole list.
func loadKeys(client *passcli.Client, vault keys.Vault) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		found, err := client.SSHKeys(ctx, vault)

		return keysLoadedMsg{vault: vault, keys: found, err: err}
	}
}

// loadPublicKey fetches one item's public key so its algorithm and fingerprint
// can be derived. This is a separate call per item because the list output
// carries no key material at all.
func loadPublicKey(client *passcli.Client, key keys.Key) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		publicKey, err := client.PublicKey(ctx, key.ShareID, key.ID)

		return publicKeyLoadedMsg{
			shareID:   key.ShareID,
			itemID:    key.ID,
			publicKey: publicKey,
			err:       err,
		}
	}
}

func loadAgentStatus(client *passcli.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		status, err := client.AgentStatus(ctx)

		return agentStatusMsg{status: status, err: err}
	}
}

func startAgent(client *passcli.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		return agentActionMsg{action: "start", err: client.StartAgent(ctx)}
	}
}

func stopAgent(client *passcli.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		return agentActionMsg{action: "stop", err: client.StopAgent(ctx)}
	}
}

// generateKey creates a new SSH key item.
//
// request.Passphrase is wiped by the client once it has been placed in the
// child environment; nothing here retains it.
func generateKey(client *passcli.Client, request passcli.GenerateRequest) tea.Cmd {
	title := request.Title

	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		_, err := client.Generate(ctx, request)

		return itemCreatedMsg{title: title, err: err}
	}
}

// trashKey moves an item to the trash, where `pass-cli item untrash` can
// restore it.
func trashKey(client *passcli.Client, key keys.Key) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		return itemRemovedMsg{
			title:     key.Title,
			permanent: false,
			err:       client.Trash(ctx, key.ShareID, key.ID),
		}
	}
}

// deleteKey permanently destroys an item. There is no undo.
func deleteKey(client *passcli.Client, key keys.Key) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
		defer cancel()

		return itemRemovedMsg{
			title:     key.Title,
			permanent: true,
			err:       client.Delete(ctx, key.ShareID, key.ID),
		}
	}
}

// copyPublicKey puts a public key on the clipboard.
//
// Only public material is ever copied; protui has no path that reads a private
// key.
func copyPublicKey(key keys.Key) tea.Cmd {
	return func() tea.Msg {
		return copiedMsg{title: key.Title, err: clipboard.WriteAll(key.PublicKey)}
	}
}

// expireStatus clears a transient status line after a delay. The token guards
// against an older timer clearing a newer message.
func expireStatus(token int) tea.Cmd {
	return tea.Tick(4*time.Second, func(time.Time) tea.Msg {
		return statusExpiredMsg{token: token}
	})
}
