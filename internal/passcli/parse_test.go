package passcli

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"unicode"

	"github.com/nxplain-sh/protui/internal/keys"
)

// fixture reads a testdata file, failing the test if it is missing.
func fixture(t *testing.T, name string) []byte {
	t.Helper()

	data, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}

	return data
}

func TestParseVaultList(t *testing.T) {
	tests := []struct {
		name    string
		file    string
		want    []keys.Vault
		wantErr error
	}{
		{
			name: "two vaults",
			file: "vault_list.json",
			want: []keys.Vault{
				{Name: "Personal", ShareID: "c2hhcmUtcGVyc29uYWw=", VaultID: "dmF1bHQtcGVyc29uYWw="},
				{Name: "Work", ShareID: "c2hhcmUtd29yaw==", VaultID: "dmF1bHQtd29yaw=="},
			},
		},
		{
			name: "no vaults",
			file: "vault_list_empty.json",
			want: []keys.Vault{},
		},
		{
			// share_id is the handle every item command needs, so a blank one
			// is a schema break rather than something to skip past.
			name:    "missing share_id is a schema error",
			file:    "vault_list_missing_share_id.json",
			wantErr: ErrUnexpectedSchema,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := parseVaultList(fixture(t, test.file))

			if test.wantErr != nil {
				if !errors.Is(err, test.wantErr) {
					t.Fatalf("error = %v, want %v", err, test.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if len(got) != len(test.want) {
				t.Fatalf("got %d vaults, want %d", len(got), len(test.want))
			}
			for i := range test.want {
				if got[i] != test.want[i] {
					t.Errorf("vault %d = %+v, want %+v", i, got[i], test.want[i])
				}
			}
		})
	}
}

func TestParseVaultListMalformed(t *testing.T) {
	if _, err := parseVaultList([]byte("not json")); err == nil {
		t.Fatal("expected an error for malformed JSON")
	}
}

func TestParseItemList(t *testing.T) {
	vault := keys.Vault{Name: "Personal", ShareID: "c2hhcmUtcGVyc29uYWw="}

	type want struct {
		id        string
		title     string
		state     keys.State
		vaultName string
		created   time.Time
	}

	tests := []struct {
		name    string
		file    string
		want    []want
		wantErr error
	}{
		{
			name: "two ssh keys",
			file: "item_list_ssh_keys.json",
			want: []want{
				{
					id:        "aXRlbS1lZDI1NTE5",
					title:     "laptop",
					state:     keys.StateActive,
					vaultName: "Personal",
					created:   time.Date(2026, 1, 23, 19, 48, 23, 0, time.UTC),
				},
				{
					id:        "aXRlbS1yc2E=",
					title:     "deploy key",
					state:     keys.StateActive,
					vaultName: "Personal",
					created:   time.Date(2025, 11, 2, 11, 0, 9, 0, time.UTC),
				},
			},
		},
		{
			// --filter-type is applied client-side upstream, so it is an
			// optimisation and not a guarantee. Non-SSH items must be dropped
			// here too.
			name: "non-ssh items are filtered out",
			file: "item_list_mixed.json",
			want: []want{
				{
					id:        "aXRlbS1zc2g=",
					title:     "old key",
					state:     keys.StateTrashed,
					vaultName: "Personal",
					created:   time.Date(2026, 1, 23, 19, 48, 23, 0, time.UTC),
				},
			},
		},
		{
			name: "empty list",
			file: "item_list_empty.json",
			want: []want{},
		},
		{
			// A new upstream field must never break the list.
			name: "unknown fields are tolerated",
			file: "item_list_unknown_fields.json",
			want: []want{
				{
					id:        "aXRlbS1mdXR1cmU=",
					title:     "future key",
					state:     keys.StateActive,
					vaultName: "Personal",
					created:   time.Date(2026, 3, 1, 9, 30, 0, 0, time.UTC),
				},
			},
		},
		{
			// A renamed or removed field decodes to the zero value silently;
			// required fields are validated so that surfaces as an error.
			name:    "missing id is a schema error",
			file:    "item_list_missing_id.json",
			wantErr: ErrUnexpectedSchema,
		},
		{
			// Upstream timestamps are zoneless. If they ever gain an offset,
			// that is a schema change we want to hear about loudly.
			name:    "RFC3339 timestamp is rejected",
			file:    "item_list_bad_timestamp.json",
			wantErr: nil, // decode error, not a sentinel; checked below
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := parseItemList(fixture(t, test.file), vault)

			if test.file == "item_list_bad_timestamp.json" {
				if err == nil {
					t.Fatal("expected an error for a zoned timestamp")
				}
				return
			}

			if test.wantErr != nil {
				if !errors.Is(err, test.wantErr) {
					t.Fatalf("error = %v, want %v", err, test.wantErr)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if len(got) != len(test.want) {
				t.Fatalf("got %d keys, want %d", len(got), len(test.want))
			}

			for i, expected := range test.want {
				actual := got[i]

				if actual.ID != expected.id {
					t.Errorf("key %d id = %q, want %q", i, actual.ID, expected.id)
				}
				if actual.Title != expected.title {
					t.Errorf("key %d title = %q, want %q", i, actual.Title, expected.title)
				}
				if actual.State != expected.state {
					t.Errorf("key %d state = %q, want %q", i, actual.State, expected.state)
				}
				if actual.VaultName != expected.vaultName {
					t.Errorf("key %d vault = %q, want %q", i, actual.VaultName, expected.vaultName)
				}
				if !actual.CreatedAt.Equal(expected.created) {
					t.Errorf("key %d created = %v, want %v", i, actual.CreatedAt, expected.created)
				}
				// The list call carries no key material, so metadata stays
				// unknown until the public key is fetched separately.
				if actual.Algorithm != keys.AlgorithmUnknown {
					t.Errorf("key %d algorithm = %q, want %q", i, actual.Algorithm, keys.AlgorithmUnknown)
				}
				if actual.PublicKey != "" {
					t.Errorf("key %d unexpectedly carries a public key", i)
				}
			}
		})
	}
}

// TestParseItemListShareIDFallback covers upstream omitting the per-item
// share_id: the vault we queried is the correct fallback.
func TestParseItemListShareIDFallback(t *testing.T) {
	vault := keys.Vault{Name: "Personal", ShareID: "fallback-share"}
	input := []byte(`{"items":[{"id":"a","share_id":"","title":"k","item_type":"ssh_key",` +
		`"state":"Active","create_time":"2026-01-01T00:00:00","modify_time":"2026-01-01T00:00:00"}]}`)

	got, err := parseItemList(input, vault)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("got %d keys, want 1", len(got))
	}
	if got[0].ShareID != "fallback-share" {
		t.Errorf("share id = %q, want %q", got[0].ShareID, "fallback-share")
	}
}

// TestParseStateUnknown documents that an unrecognised state keeps the item
// visible rather than dropping it.
func TestParseStateUnknown(t *testing.T) {
	tests := []struct {
		raw  string
		want keys.State
	}{
		{stateActive, keys.StateActive},
		{stateTrashed, keys.StateTrashed},
		{"Archived", keys.StateActive},
		{"", keys.StateActive},
		// Casing matters: upstream emits PascalCase here, not snake_case.
		{"active", keys.StateActive},
		{"trashed", keys.StateActive},
	}

	for _, test := range tests {
		if got := parseState(test.raw); got != test.want {
			t.Errorf("parseState(%q) = %q, want %q", test.raw, got, test.want)
		}
	}
}

func TestParseAgentStatus(t *testing.T) {
	tests := []struct {
		name       string
		file       string
		wantState  AgentState
		wantPID    string
		wantSocket string
	}{
		{
			name:       "running",
			file:       "agent_running.txt",
			wantState:  AgentRunning,
			wantPID:    "54321",
			wantSocket: "/Users/example/.proton-pass/ssh-agent.sock",
		},
		{
			name:      "stopped with no pid file",
			file:      "agent_stopped_no_pidfile.txt",
			wantState: AgentStopped,
		},
		{
			// The parenthetical is diagnostic prose, so the state is matched
			// by prefix.
			name:       "degraded",
			file:       "agent_degraded.txt",
			wantState:  AgentDegraded,
			wantPID:    "54321",
			wantSocket: "/Users/example/.proton-pass/ssh-agent.sock (not found)",
		},
		{
			name:       "stopped with stale socket",
			file:       "agent_stopped_stale_socket.txt",
			wantState:  AgentStopped,
			wantPID:    "54321 (not running)",
			wantSocket: "/Users/example/.proton-pass/ssh-agent.sock (stale)",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got := parseAgentStatus(fixture(t, test.file))

			if got.State != test.wantState {
				t.Errorf("state = %q, want %q", got.State, test.wantState)
			}
			if got.PID != test.wantPID {
				t.Errorf("pid = %q, want %q", got.PID, test.wantPID)
			}
			if got.Socket != test.wantSocket {
				t.Errorf("socket = %q, want %q", got.Socket, test.wantSocket)
			}
		})
	}
}

// TestParseAgentStatusPIDFileNotConfusedWithPID guards the one genuinely
// ambiguous line: "PID file:" must not be read as "PID:".
func TestParseAgentStatusPIDFileNotConfusedWithPID(t *testing.T) {
	got := parseAgentStatus([]byte("Status:   stopped\nPID file: /tmp/x.pid (not found)\n"))

	if got.PID != "" {
		t.Errorf("pid = %q, want empty; \"PID file\" was misread as \"PID\"", got.PID)
	}
	if got.State != AgentStopped {
		t.Errorf("state = %q, want %q", got.State, AgentStopped)
	}
}

// TestParseAgentStatusUnrecognised covers upstream rewording the status line
// entirely: better to report unknown than to guess "stopped".
func TestParseAgentStatusUnrecognised(t *testing.T) {
	got := parseAgentStatus([]byte("Status:   hibernating\n"))

	if got.State != AgentUnknown {
		t.Errorf("state = %q, want %q", got.State, AgentUnknown)
	}
	if got.Detail != "hibernating" {
		t.Errorf("detail = %q, want %q", got.Detail, "hibernating")
	}
}

func TestParseAgentStatusEmpty(t *testing.T) {
	if got := parseAgentStatus(nil); got.State != AgentUnknown {
		t.Errorf("state = %q, want %q", got.State, AgentUnknown)
	}
}

func TestCleanStderr(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{
			name:  "strips the Error prefix",
			input: "Error: Please provide either --share-id, --vault-name\n",
			want:  "Please provide either --share-id, --vault-name",
		},
		{
			// Upstream writes this to stderr on the success path when a
			// passphrase comes from the environment; it is not a failure.
			name:  "drops the passphrase advisory",
			input: "Reading password from environment variable PROTON_PASS_SSH_KEY_PASSWORD\n",
			want:  "",
		},
		{
			name:  "keeps a real error alongside the advisory",
			input: "Reading password from environment variable X\nError: vault not found\n",
			want:  "vault not found",
		},
		{
			name:  "empty",
			input: "",
			want:  "",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := cleanStderr([]byte(test.input)); got != test.want {
				t.Errorf("cleanStderr() = %q, want %q", got, test.want)
			}
		})
	}
}

// TestParsersSanitizeHostileText covers the boundary at which text from Proton
// Pass becomes text protui will draw.
//
// Item titles and vault names are user-authored, and Proton Pass supports
// sharing both items and vaults, so either can have been written by somebody
// else. Drawn unfiltered into a terminal they stop being data: ESC [ 2 J
// clears the screen, and OSC 52 writes the system clipboard on xterm, kitty,
// iTerm2, WezTerm and foot \u2014 which is precisely what protui's copy action
// is for, so a hostile title could replace what a user believes they copied.
//
// This is the choke point, and it has to be: sanitising cannot be deferred to
// render time, because by then lipgloss has wrapped the text in its own SGR
// escapes and stripping those would remove protui's styling along with the
// attack.
func TestParsersSanitizeHostileText(t *testing.T) {
	t.Run("item titles", func(t *testing.T) {
		parsed, err := parseItemList(
			fixture(t, "item_list_hostile_title.json"),
			keys.Vault{Name: "Personal", ShareID: "share-1"},
		)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(parsed) != 1 {
			t.Fatalf("got %d keys, want 1", len(parsed))
		}

		assertNoEscapes(t, "title", parsed[0].Title)

		// The readable part survives; only the instruction is removed.
		if !strings.Contains(parsed[0].Title, "innocent") {
			t.Errorf("title = %q, want it to keep its visible text", parsed[0].Title)
		}
	})

	t.Run("vault names", func(t *testing.T) {
		parsed, err := parseVaultList(fixture(t, "vault_list_hostile_name.json"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(parsed) != 1 {
			t.Fatalf("got %d vaults, want 1", len(parsed))
		}

		assertNoEscapes(t, "vault name", parsed[0].Name)

		if !strings.Contains(parsed[0].Name, "Personal") {
			t.Errorf("name = %q, want it to keep its visible text", parsed[0].Name)
		}
	})

	t.Run("agent status", func(t *testing.T) {
		status := parseAgentStatus([]byte(
			"Status:   running\x1b[2J\nPID:      42\x07\nSocket:   /tmp/a\x1b]0;x\x07.sock\n",
		))

		// Classification still reads the raw value, so it is unaffected.
		if status.State != AgentRunning {
			t.Errorf("state = %q, want %q", status.State, AgentRunning)
		}

		assertNoEscapes(t, "detail", status.Detail)
		assertNoEscapes(t, "pid", status.PID)
		assertNoEscapes(t, "socket", status.Socket)
	})

	t.Run("stderr", func(t *testing.T) {
		// Upstream echoes item titles into some of its errors, so its stderr
		// is outside text like any other.
		assertNoEscapes(t, "stderr", cleanStderr(
			[]byte("Error: no such item \x1b[2J\x1b]52;c;cHduZWQ=\x07"),
		))
	})
}

// assertNoEscapes fails if value carries anything a terminal would act on.
func assertNoEscapes(t *testing.T, field, value string) {
	t.Helper()

	for _, r := range value {
		if unicode.IsControl(r) {
			t.Errorf("%s contains control rune %U: %q", field, r, value)
		}
		if r >= '\u202a' && r <= '\u202e' || r >= '\u2066' && r <= '\u2069' {
			t.Errorf("%s contains bidi override %U: %q", field, r, value)
		}
	}
}
