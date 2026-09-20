package passcli

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/nxplain-sh/protui/internal/keys"
)

// KeyType is a generatable SSH key algorithm, matching --key-type upstream.
type KeyType string

// Key types accepted by `item create ssh-key generate`.
const (
	KeyTypeED25519 KeyType = "ed25519"
	KeyTypeRSA2048 KeyType = "rsa2048"
	KeyTypeRSA4096 KeyType = "rsa4096"
)

// KeyTypes lists the generatable types in menu order, ed25519 first to match
// upstream's default.
var KeyTypes = []KeyType{KeyTypeED25519, KeyTypeRSA2048, KeyTypeRSA4096}

// Vaults lists every vault the session can see.
func (c *Client) Vaults(ctx context.Context) ([]keys.Vault, error) {
	stdout, err := c.call(ctx, "vault list", nil, "vault", "list", "--output", "json")
	if err != nil {
		return nil, err
	}

	vaults, err := parseVaultList(stdout)
	if err != nil {
		return nil, &CommandError{Command: "vault list", ExitCode: 0, Err: err}
	}

	return vaults, nil
}

// SSHKeys lists the SSH key items in one vault.
//
// pass-cli cannot list across vaults — it errors without a vault selector — so
// callers fan out per vault. See docs/schema.md §1.2.
func (c *Client) SSHKeys(ctx context.Context, vault keys.Vault) ([]keys.Key, error) {
	stdout, err := c.call(ctx, "item list", nil,
		"item", "list",
		"--share-id", vault.ShareID,
		"--filter-type", filterTypeSSHKey,
		"--filter-state", "active",
		"--output", "json",
	)
	if err != nil {
		return nil, err
	}

	found, err := parseItemList(stdout, vault)
	if err != nil {
		return nil, &CommandError{Command: "item list", ExitCode: 0, Err: err}
	}

	return found, nil
}

// PublicKey fetches one item's public key.
//
// This uses the field-scoped read, which prints a single field as bare text.
// The alternative — `item view --output json` — returns the private key
// unredacted and is never used. See docs/schema.md §1.4.
func (c *Client) PublicKey(ctx context.Context, shareID, itemID string) (string, error) {
	stdout, err := c.call(ctx, "item view", nil,
		"item", "view",
		"--share-id", shareID,
		"--item-id", itemID,
		"--field", "public_key",
	)
	if err != nil {
		return "", err
	}

	return keys.Sanitize(strings.TrimSpace(string(stdout))), nil
}

// GenerateRequest describes a key to generate.
//
// Passphrase is optional and is the only secret protui ever sends. It is a
// []byte so the caller's copy can be wiped; it is never placed in argv.
type GenerateRequest struct {
	Title      string
	ShareID    string
	Comment    string
	KeyType    KeyType
	Passphrase []byte
}

// Generate creates a new SSH key item and returns its item id.
//
// On success upstream prints the bare item id on stdout with no JSON envelope.
//
// Passphrase handling deserves care. Upstream checks
// PROTON_PASS_SSH_KEY_PASSWORD before it consults --password, and falls back to
// an interactive TTY prompt if --password is passed with no env var set — which
// inside a TUI would contend with Bubble Tea for the terminal. Setting the
// variable and omitting the flag avoids the prompt entirely.
//
// The passphrase is zeroed here after the environment entry is built. Note that
// the entry itself is a Go string and so cannot be wiped; it lives until
// collected. It is never written to argv, disk, or logs.
func (c *Client) Generate(ctx context.Context, request GenerateRequest) (string, error) {
	if request.Title == "" {
		return "", errors.New("title is required")
	}
	if request.ShareID == "" {
		return "", errors.New("vault is required")
	}

	keyType := request.KeyType
	if keyType == "" {
		keyType = KeyTypeED25519
	}

	args := []string{
		"item", "create", "ssh-key", "generate",
		"--title", request.Title,
		"--share-id", request.ShareID,
		"--key-type", string(keyType),
	}
	if request.Comment != "" {
		args = append(args, "--comment", request.Comment)
	}

	var extraEnv []string
	if len(request.Passphrase) > 0 {
		extraEnv = append(extraEnv, "PROTON_PASS_SSH_KEY_PASSWORD="+string(request.Passphrase))
		clear(request.Passphrase)
	}

	stdout, err := c.call(ctx, "item create ssh-key generate", extraEnv, args...)
	if err != nil {
		return "", err
	}

	itemID := strings.TrimSpace(string(stdout))
	if itemID == "" {
		return "", &CommandError{
			Command: "item create ssh-key generate",
			Err:     fmt.Errorf("no item id on stdout: %w", ErrUnexpectedSchema),
		}
	}

	return itemID, nil
}

// Trash moves an item to the trash, where it can be restored with
// `pass-cli item untrash`.
func (c *Client) Trash(ctx context.Context, shareID, itemID string) error {
	_, err := c.call(ctx, "item trash", nil,
		"item", "trash",
		"--share-id", shareID,
		"--item-id", itemID,
	)

	return err
}

// Delete permanently destroys an item. There is no undo and upstream does not
// confirm; the confirmation is protui's responsibility.
func (c *Client) Delete(ctx context.Context, shareID, itemID string) error {
	_, err := c.call(ctx, "item delete", nil,
		"item", "delete",
		"--share-id", shareID,
		"--item-id", itemID,
	)

	return err
}
