package passcli

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/nxplain-sh/protui/internal/keys"
)

// recorder captures what a wrapper would have executed, so argument
// construction can be asserted without a real pass-cli binary.
type recorder struct {
	args   []string
	env    []string
	stdout []byte
	err    error
}

func (r *recorder) runner() runner {
	return func(_ context.Context, extraEnv []string, args ...string) ([]byte, []byte, error) {
		r.args = args
		r.env = extraEnv

		return r.stdout, nil, r.err
	}
}

func newTestClient(rec *recorder) *Client {
	return &Client{binary: Binary, run: rec.runner()}
}

// TestGeneratePassphraseNeverReachesArgv is the load-bearing security test:
// anything in argv is readable by any user on the box via ps, so the passphrase
// must travel in the environment instead.
func TestGeneratePassphraseNeverReachesArgv(t *testing.T) {
	const secret = "correct-horse-battery-staple"

	rec := &recorder{stdout: []byte("new-item-id\n")}
	client := newTestClient(rec)

	itemID, err := client.Generate(context.Background(), GenerateRequest{
		Title:      "laptop",
		ShareID:    "share-1",
		Comment:    "laptop@example",
		KeyType:    KeyTypeED25519,
		Passphrase: []byte(secret),
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if itemID != "new-item-id" {
		t.Errorf("item id = %q, want %q", itemID, "new-item-id")
	}

	for i, arg := range rec.args {
		if strings.Contains(arg, secret) {
			t.Fatalf("passphrase leaked into argv at index %d: %q", i, arg)
		}
	}

	// It must instead be exactly one environment entry.
	var found int
	for _, entry := range rec.env {
		if entry == "PROTON_PASS_SSH_KEY_PASSWORD="+secret {
			found++
		}
	}
	if found != 1 {
		t.Errorf("found %d passphrase env entries, want exactly 1 (env=%v)", found, rec.env)
	}
}

// TestGenerateNeverPassesPasswordFlag guards the TUI against upstream's
// interactive prompt: --password with no env var set drops into a TTY read that
// would contend with Bubble Tea for the terminal. Setting the variable and
// omitting the flag avoids the prompt entirely. See docs/schema.md §6.1.
func TestGenerateNeverPassesPasswordFlag(t *testing.T) {
	for _, passphrase := range [][]byte{nil, []byte("secret")} {
		rec := &recorder{stdout: []byte("id\n")}
		client := newTestClient(rec)

		_, err := client.Generate(context.Background(), GenerateRequest{
			Title:      "t",
			ShareID:    "s",
			Passphrase: passphrase,
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		for _, arg := range rec.args {
			if arg == "--password" {
				t.Errorf("--password was passed (passphrase set: %v); it would prompt on the TTY", passphrase != nil)
			}
		}
	}
}

// TestGenerateWipesPassphrase checks that the caller's buffer is zeroed, so the
// secret does not linger in a slice the UI still holds.
func TestGenerateWipesPassphrase(t *testing.T) {
	passphrase := []byte("wipe-me")
	rec := &recorder{stdout: []byte("id\n")}

	if _, err := newTestClient(rec).Generate(context.Background(), GenerateRequest{
		Title: "t", ShareID: "s", Passphrase: passphrase,
	}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	for i, b := range passphrase {
		if b != 0 {
			t.Fatalf("passphrase buffer not wiped at index %d: %q", i, passphrase)
		}
	}
}

func TestGenerateDefaultsToED25519(t *testing.T) {
	rec := &recorder{stdout: []byte("id\n")}

	if _, err := newTestClient(rec).Generate(context.Background(), GenerateRequest{
		Title: "t", ShareID: "s",
	}); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !hasFlagValue(rec.args, "--key-type", string(KeyTypeED25519)) {
		t.Errorf("args = %v, want --key-type %s", rec.args, KeyTypeED25519)
	}
	// An empty comment must be omitted rather than passed as "".
	if hasFlag(rec.args, "--comment") {
		t.Errorf("args = %v, want no --comment when it is empty", rec.args)
	}
}

func TestGenerateRejectsEmptyRequiredFields(t *testing.T) {
	client := newTestClient(&recorder{})

	if _, err := client.Generate(context.Background(), GenerateRequest{ShareID: "s"}); err == nil {
		t.Error("expected an error for a missing title")
	}
	if _, err := client.Generate(context.Background(), GenerateRequest{Title: "t"}); err == nil {
		t.Error("expected an error for a missing vault")
	}
}

func TestGenerateEmptyStdoutIsSchemaError(t *testing.T) {
	rec := &recorder{stdout: []byte("  \n")}

	_, err := newTestClient(rec).Generate(context.Background(), GenerateRequest{Title: "t", ShareID: "s"})
	if !errors.Is(err, ErrUnexpectedSchema) {
		t.Errorf("error = %v, want %v", err, ErrUnexpectedSchema)
	}
}

// TestPublicKeyUsesFieldScopedRead pins the read path away from
// `item view --output json`, which returns the private key unredacted.
// See docs/schema.md §1.4.
func TestPublicKeyUsesFieldScopedRead(t *testing.T) {
	rec := &recorder{stdout: []byte("ssh-ed25519 AAAAC3Nz... laptop@example\n")}

	got, err := newTestClient(rec).PublicKey(context.Background(), "share-1", "item-1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != "ssh-ed25519 AAAAC3Nz... laptop@example" {
		t.Errorf("public key = %q; trailing newline should be trimmed", got)
	}

	if !hasFlagValue(rec.args, "--field", "public_key") {
		t.Fatalf("args = %v, want --field public_key", rec.args)
	}
	if hasFlag(rec.args, "--output") {
		t.Errorf("args = %v, want no --output: the JSON form returns the private key", rec.args)
	}
	for _, arg := range rec.args {
		if strings.Contains(arg, "private") {
			t.Errorf("args = %v, must never name the private key", rec.args)
		}
	}
}

// TestNoCommandRequestsSecrets sweeps every read wrapper for --show-secrets,
// which swaps the redacted summary for full items including private keys.
func TestNoCommandRequestsSecrets(t *testing.T) {
	ctx := context.Background()
	vault := keys.Vault{Name: "Personal", ShareID: "share-1"}

	calls := map[string]func(*Client) error{
		"vault list": func(c *Client) error { _, err := c.Vaults(ctx); return err },
		"item list":  func(c *Client) error { _, err := c.SSHKeys(ctx, vault); return err },
		"item view":  func(c *Client) error { _, err := c.PublicKey(ctx, "s", "i"); return err },
		"agent status": func(c *Client) error {
			_, err := c.AgentStatus(ctx)
			return err
		},
	}

	for name, call := range calls {
		rec := &recorder{stdout: []byte(`{"vaults":[],"items":[]}`)}
		if err := call(newTestClient(rec)); err != nil {
			t.Fatalf("%s: unexpected error: %v", name, err)
		}

		for _, arg := range rec.args {
			if arg == "--show-secrets" {
				t.Errorf("%s passed --show-secrets", name)
			}
		}
	}
}

func TestSSHKeysFiltersServerSide(t *testing.T) {
	rec := &recorder{stdout: []byte(`{"items":[]}`)}
	vault := keys.Vault{Name: "Personal", ShareID: "share-1"}

	if _, err := newTestClient(rec).SSHKeys(context.Background(), vault); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Note the CLI flag spelling is kebab-case while the JSON value is
	// snake_case; they are deliberately separate constants.
	if !hasFlagValue(rec.args, "--filter-type", "ssh-key") {
		t.Errorf("args = %v, want --filter-type ssh-key", rec.args)
	}
	if !hasFlagValue(rec.args, "--share-id", "share-1") {
		t.Errorf("args = %v, want --share-id share-1", rec.args)
	}
}

// TestCommandErrorNamesTheCommand covers the requirement that a failure says
// which pass-cli call broke, rather than surfacing an empty list.
func TestCommandErrorNamesTheCommand(t *testing.T) {
	rec := &recorder{err: errors.New("exit status 1")}

	_, err := newTestClient(rec).Vaults(context.Background())
	if err == nil {
		t.Fatal("expected an error")
	}

	var commandErr *CommandError
	if !errors.As(err, &commandErr) {
		t.Fatalf("error %T is not a *CommandError", err)
	}
	if commandErr.Command != "vault list" {
		t.Errorf("command = %q, want %q", commandErr.Command, "vault list")
	}
	if !strings.Contains(err.Error(), "vault list") {
		t.Errorf("message %q does not name the failing command", err.Error())
	}
}

// TestDecodeFailureIsReportedAsCommandError covers upstream returning success
// with output we cannot parse: the UI still needs to know which call produced it.
func TestDecodeFailureIsReportedAsCommandError(t *testing.T) {
	rec := &recorder{stdout: []byte("this is not json")}

	_, err := newTestClient(rec).Vaults(context.Background())

	var commandErr *CommandError
	if !errors.As(err, &commandErr) {
		t.Fatalf("error %T is not a *CommandError", err)
	}
	if commandErr.Command != "vault list" {
		t.Errorf("command = %q, want %q", commandErr.Command, "vault list")
	}
}

func TestTrashAndDeleteAreDistinctCommands(t *testing.T) {
	trashRec := &recorder{}
	if err := newTestClient(trashRec).Trash(context.Background(), "s", "i"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if trashRec.args[1] != "trash" {
		t.Errorf("Trash ran %v, want `item trash`", trashRec.args)
	}

	deleteRec := &recorder{}
	if err := newTestClient(deleteRec).Delete(context.Background(), "s", "i"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if deleteRec.args[1] != "delete" {
		t.Errorf("Delete ran %v, want `item delete`", deleteRec.args)
	}
}

// TestPreflightRunsInfo pins the subcommand actually reaching argv. The error
// label and the argument list are separate parameters, and passing only the
// label runs the bare binary, which exits non-zero and looks exactly like a
// missing session.
func TestPreflightRunsInfo(t *testing.T) {
	rec := &recorder{}

	if err := newTestClient(rec).Preflight(context.Background()); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(rec.args) != 1 || rec.args[0] != "info" {
		t.Errorf("args = %v, want [info]", rec.args)
	}
}

func TestPreflightReportsNoSession(t *testing.T) {
	rec := &recorder{err: errors.New("exit status 1")}

	err := newTestClient(rec).Preflight(context.Background())
	if !errors.Is(err, ErrNoSession) {
		t.Fatalf("error = %v, want %v", err, ErrNoSession)
	}
	if !strings.Contains(err.Error(), "login") {
		t.Errorf("message %q should tell the user to run `pass-cli login`", err.Error())
	}
}

// TestEverySubcommandReachesArgv sweeps the wrappers for the same mistake:
// each must put its subcommand into the argument list, not only into the
// error label.
func TestEverySubcommandReachesArgv(t *testing.T) {
	ctx := context.Background()
	vault := keys.Vault{Name: "Personal", ShareID: "share-1"}

	cases := []struct {
		name string
		want []string
		call func(*Client) error
	}{
		{"vaults", []string{"vault", "list"}, func(c *Client) error { _, err := c.Vaults(ctx); return err }},
		{"ssh keys", []string{"item", "list"}, func(c *Client) error { _, err := c.SSHKeys(ctx, vault); return err }},
		{"public key", []string{"item", "view"}, func(c *Client) error { _, err := c.PublicKey(ctx, "s", "i"); return err }},
		{"trash", []string{"item", "trash"}, func(c *Client) error { return c.Trash(ctx, "s", "i") }},
		{"delete", []string{"item", "delete"}, func(c *Client) error { return c.Delete(ctx, "s", "i") }},
		{"agent status", []string{"ssh-agent", "daemon", "status"}, func(c *Client) error {
			_, err := c.AgentStatus(ctx)
			return err
		}},
		{"agent start", []string{"ssh-agent", "daemon", "start"}, func(c *Client) error { return c.StartAgent(ctx) }},
		{"agent stop", []string{"ssh-agent", "daemon", "stop"}, func(c *Client) error { return c.StopAgent(ctx) }},
		{"generate", []string{"item", "create", "ssh-key", "generate"}, func(c *Client) error {
			_, err := c.Generate(ctx, GenerateRequest{Title: "t", ShareID: "s"})
			return err
		}},
	}

	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			rec := &recorder{stdout: []byte(`{"vaults":[],"items":[]}`)}
			if err := test.call(newTestClient(rec)); err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if len(rec.args) < len(test.want) {
				t.Fatalf("args = %v, want it to start with %v", rec.args, test.want)
			}
			for i, want := range test.want {
				if rec.args[i] != want {
					t.Fatalf("args = %v, want it to start with %v", rec.args, test.want)
				}
			}
		})
	}
}

func hasFlag(args []string, flag string) bool {
	for _, arg := range args {
		if arg == flag {
			return true
		}
	}
	return false
}

func hasFlagValue(args []string, flag, value string) bool {
	for i, arg := range args {
		if arg == flag && i+1 < len(args) && args[i+1] == value {
			return true
		}
	}
	return false
}
