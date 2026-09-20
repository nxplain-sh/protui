package ui

import (
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

// These are render smoke tests. They drive the model with synthetic messages
// and assert that every screen renders, which catches panics in layout
// arithmetic without needing a terminal or a pass-cli binary.
//
// The client is nil throughout: commands returned by Update are values that are
// never executed here, so nothing dials out.

const testPublicKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM+XgkeZS/lofu0u1xq0g4DQUZIOxGcdSqHhQbjKwVEQ laptop@example"

func testVaults() []keys.Vault {
	return []keys.Vault{
		{Name: "Personal", ShareID: "share-1", VaultID: "vault-1"},
		{Name: "Work", ShareID: "share-2", VaultID: "vault-2"},
	}
}

func testKeys() []keys.Key {
	return []keys.Key{
		{
			ID:        "item-1",
			ShareID:   "share-1",
			VaultName: "Personal",
			Title:     "laptop",
			State:     keys.StateActive,
			Algorithm: keys.AlgorithmUnknown,
			CreatedAt: time.Date(2026, 1, 23, 19, 48, 23, 0, time.UTC),
		},
	}
}

// loaded returns a model in the state it reaches after a successful startup.
func loaded(t *testing.T, width, height int) Model {
	t.Helper()

	model := New(nil)

	next, _ := model.Update(tea.WindowSizeMsg{Width: width, Height: height})
	next, _ = next.(Model).Update(vaultsLoadedMsg{vaults: testVaults()})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: testVaults()[0], keys: testKeys()})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: testVaults()[1]})

	return next.(Model)
}

func TestViewRendersEveryMode(t *testing.T) {
	base := loaded(t, 120, 40)

	// A public key arriving must populate the derived metadata shown in both
	// the list row and the detail pane.
	updated, _ := base.Update(publicKeyLoadedMsg{
		shareID:   "share-1",
		itemID:    "item-1",
		publicKey: testPublicKey,
	})
	base = updated.(Model)

	tests := []struct {
		name     string
		model    Model
		contains []string
	}{
		{
			name:     "list with detail pane",
			model:    base,
			contains: []string{"protui", "laptop", "Personal", "ed25519", "Fingerprint"},
		},
		{
			name: "create form",
			model: func() Model {
				m := base
				m.mode = modeCreate
				m.form = newCreateForm(testVaults(), testVaults()[0])
				return m
			}(),
			contains: []string{"New SSH key", "Title", "Vault", "Passphrase", "ed25519"},
		},
		{
			name: "trash confirmation",
			model: func() Model {
				m := base
				m.mode = modeConfirmTrash
				m.target = testKeys()[0]
				return m
			}(),
			contains: []string{"Move to trash", "laptop", "untrash"},
		},
		{
			name: "permanent delete confirmation",
			model: func() Model {
				m := base
				m.mode = modeConfirmDelete
				m.target = testKeys()[0]
				return m
			}(),
			contains: []string{"Permanently delete", "cannot be undone", "Type the title"},
		},
		{
			name: "agent panel",
			model: func() Model {
				m := base
				m.mode = modeAgent
				m.agent = passcli.AgentStatus{
					State:  passcli.AgentRunning,
					Detail: "running",
					PID:    "4321",
					Socket: "/tmp/agent.sock",
				}
				return m
			}(),
			contains: []string{"SSH agent daemon", "running", "4321", "SSH_AUTH_SOCK"},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			out := test.model.View()

			for _, want := range test.contains {
				if !strings.Contains(out, want) {
					t.Errorf("view does not contain %q\n---\n%s", want, out)
				}
			}
		})
	}
}

// TestViewNeverRendersPrivateKeyMaterial is a belt-and-braces check on the
// invariant that only public material reaches the terminal.
func TestViewNeverRendersPrivateKeyMaterial(t *testing.T) {
	model := loaded(t, 120, 40)

	updated, _ := model.Update(publicKeyLoadedMsg{
		shareID: "share-1", itemID: "item-1", publicKey: testPublicKey,
	})

	out := updated.(Model).View()

	for _, forbidden := range []string{"PRIVATE KEY", "private_key", "BEGIN OPENSSH"} {
		if strings.Contains(out, forbidden) {
			t.Errorf("view contains %q", forbidden)
		}
	}
}

// TestViewSurvivesNarrowTerminals covers the layout arithmetic, which is where
// a panic would most plausibly hide.
func TestViewSurvivesNarrowTerminals(t *testing.T) {
	for _, size := range []struct{ w, h int }{
		{20, 10}, {40, 12}, {80, 24}, {91, 30}, {92, 30}, {200, 60},
	} {
		model := loaded(t, size.w, size.h)

		updated, _ := model.Update(publicKeyLoadedMsg{
			shareID: "share-1", itemID: "item-1", publicKey: testPublicKey,
		})

		// A panic here fails the test; the assertion is that output exists.
		if out := updated.(Model).View(); out == "" {
			t.Errorf("%dx%d rendered nothing", size.w, size.h)
		}
	}
}

// TestEmptyAndLoadingStates covers the two states a first run passes through.
func TestEmptyAndLoadingStates(t *testing.T) {
	model := New(nil)

	if out := model.View(); out != "Loading…" {
		t.Errorf("pre-resize view = %q, want %q", out, "Loading…")
	}

	sized, _ := model.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	loadedEmpty, _ := sized.(Model).Update(vaultsLoadedMsg{vaults: nil})

	if out := loadedEmpty.(Model).View(); !strings.Contains(out, "No SSH keys found") {
		t.Errorf("empty view should prompt the user to create one:\n%s", out)
	}
}

// TestVaultErrorDoesNotBlankTheList covers the fan-out requirement: one
// unreachable vault must not discard what the others returned.
func TestVaultErrorDoesNotBlankTheList(t *testing.T) {
	model := New(nil)

	next, _ := model.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	next, _ = next.(Model).Update(vaultsLoadedMsg{vaults: testVaults()})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: testVaults()[0], keys: testKeys()})
	next, _ = next.(Model).Update(keysLoadedMsg{
		vault: testVaults()[1],
		err:   &passcli.CommandError{Command: "item list", Stderr: "vault unreachable"},
	})

	final := next.(Model)

	if len(final.list.Items()) != 1 {
		t.Fatalf("got %d items, want the 1 from the vault that succeeded", len(final.list.Items()))
	}

	out := final.View()
	if !strings.Contains(out, "laptop") {
		t.Errorf("the successful vault's key is missing:\n%s", out)
	}
	if !strings.Contains(out, "Work") {
		t.Errorf("the failing vault should be named in the status line:\n%s", out)
	}
}

// TestDeleteRequiresExactTitle covers the guard on the irreversible action.
func TestDeleteRequiresExactTitle(t *testing.T) {
	model := loaded(t, 120, 40)
	model.mode = modeConfirmDelete
	model.target = testKeys()[0]

	// A near miss must not delete.
	model.confirmInput.SetValue("laptopp")

	next, cmd := model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if next.(Model).mode != modeConfirmDelete {
		t.Error("a mismatched title should keep the prompt open")
	}
	if cmd == nil {
		t.Error("expected a warning to be shown")
	}

	// The exact title proceeds.
	model.confirmInput.SetValue("laptop")

	next, cmd = model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if next.(Model).mode != modeList {
		t.Error("a matching title should close the prompt")
	}
	if cmd == nil {
		t.Error("expected the delete command to be dispatched")
	}
}

// TestFingerprintIsNeverTruncated covers the pane's main job. A fingerprint
// with its middle elided cannot be compared against `ssh-keygen -lf` or a
// forge's UI, which is the reason to display one at all.
func TestFingerprintIsNeverTruncated(t *testing.T) {
	for _, width := range []int{92, 100, 118, 160} {
		model := loaded(t, width, 40)

		updated, _ := model.Update(publicKeyLoadedMsg{
			shareID: "share-1", itemID: "item-1", publicKey: testPublicKey,
		})
		model = updated.(Model)

		key, ok := model.selected()
		if !ok {
			t.Fatal("no key selected")
		}
		if key.Fingerprint == "" {
			t.Fatal("fingerprint was not derived")
		}

		// The value wraps across lines, so compare with all whitespace
		// removed. A fingerprint contains none of its own.
		flattened := strings.Join(strings.Fields(model.detail(model.detailInnerWidth())), "")
		if !strings.Contains(flattened, key.Fingerprint) {
			t.Errorf("at width %d the fingerprint %q is not shown in full", width, key.Fingerprint)
		}
	}
}

// TestHeaderShowsPosition covers the only cue that a list continues past the
// bottom of the screen.
func TestHeaderShowsPosition(t *testing.T) {
	model := listWith(t, 40)

	if got := model.header(); !strings.Contains(got, "1/40 keys") {
		t.Errorf("header = %q, want it to contain %q", got, "1/40 keys")
	}

	moved := press(model, tea.KeyMsg{Type: tea.KeyCtrlF})
	if got := moved.header(); !strings.Contains(got, "keys") || strings.Contains(got, "1/40") {
		t.Errorf("header did not follow the cursor: %q", got)
	}
}

// TestDetailNeverOverflowsTheTerminal covers content taller than the window.
// Left unclamped the terminal cuts it from the bottom, silently.
func TestDetailNeverOverflowsTheTerminal(t *testing.T) {
	for _, height := range []int{12, 16, 20, 30, 60} {
		model := loaded(t, 118, height)

		updated, _ := model.Update(publicKeyLoadedMsg{
			shareID: "share-1", itemID: "item-1", publicKey: testPublicKey,
		})

		lines := strings.Split(updated.(Model).View(), "\n")
		if len(lines) > height {
			t.Errorf("at height %d the view rendered %d lines", height, len(lines))
		}
	}
}

// TestDetailScrolling covers the clamps at both ends and the reset that stops
// one key's scroll position leaking onto the next.
func TestDetailScrolling(t *testing.T) {
	// Several keys, so the reset below has somewhere else to move to.
	model := listWith(t, 3).resize(tea.WindowSizeMsg{Width: 118, Height: 14})

	updated, _ := model.Update(publicKeyLoadedMsg{
		shareID: "s1", itemID: "item-00", publicKey: testPublicKey,
	})
	model = updated.(Model)

	limit := model.maxDetailOffset()
	if limit == 0 {
		t.Fatal("test needs content taller than the pane")
	}

	if got := model.scrollDetail(-1).detailOffset; got != 0 {
		t.Errorf("scrolling up from the top gave %d, want 0", got)
	}
	if got := model.scrollDetail(limit + 50).detailOffset; got != limit {
		t.Errorf("scrolling past the end gave %d, want %d", got, limit)
	}

	scrolled := model.scrollDetail(limit)
	if !strings.Contains(scrolled.View(), "↑") {
		t.Error("no marker shown for content scrolled off the top")
	}

	// Moving to another key must start its detail at the top.
	moved := press(scrolled, runes("j"))
	if moved.list.Index() == scrolled.list.Index() {
		t.Fatal("the cursor did not move to another key")
	}
	if moved.detailOffset != 0 {
		t.Errorf("offset %d carried over to another key", moved.detailOffset)
	}
}

// TestViewNeverEmitsEscapesFromKeyMaterial covers the injection path that this
// package owns. A public key's trailing comment is arbitrary bytes from
// whoever wrote the key, and WithPublicKey sanitises it on the way in.
//
// Titles and vault names are sanitised where they enter, in internal/passcli —
// see TestParsersSanitizeHostileText there. Sanitising cannot happen at render
// time: by then lipgloss has wrapped the text in its own SGR escapes, and
// stripping those would remove protui's own styling along with the attack.
func TestViewNeverEmitsEscapesFromKeyMaterial(t *testing.T) {
	payloads := map[string]string{
		"clear screen":     "\x1b[2J",
		"cursor home":      "\x1b[H",
		"window title":     "\x1b]0;pwned\x07",
		"clipboard OSC 52": "\x1b]52;c;cHduZWQ=\x07",
		"bidi override":    "\u202ereversed\u202c",
		"bell":             "\x07",
	}

	for name, payload := range payloads {
		t.Run(name, func(t *testing.T) {
			model := loaded(t, 120, 40)

			next, _ := model.Update(publicKeyLoadedMsg{
				shareID:   "share-1",
				itemID:    "item-1",
				publicKey: testPublicKey + payload,
			})

			out := next.(Model).View()

			// The key itself must still render; only the instruction goes.
			if !strings.Contains(out, "laptop") {
				t.Error("sanitising ate the surrounding text as well as the escape")
			}

			for _, forbidden := range []string{"\x1b]", "\x1b[2J", "\x1b[H", "\x07", "\u202e"} {
				if strings.Contains(out, forbidden) {
					t.Errorf("view emits %q from injected key material", forbidden)
				}
			}
		})
	}
}
