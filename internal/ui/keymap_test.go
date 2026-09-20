package ui

import (
	"fmt"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/nxplain-sh/protui/internal/keys"
)

// listWith builds a loaded model holding count keys, tall enough to paginate.
func listWith(t *testing.T, count int) Model {
	t.Helper()

	vault := keys.Vault{Name: "Personal", ShareID: "s1"}

	items := make([]keys.Key, 0, count)
	for i := range count {
		items = append(items, keys.Key{
			ID:      fmt.Sprintf("item-%02d", i),
			ShareID: "s1",
			// Titles sort in insertion order so index maps to position.
			Title:     fmt.Sprintf("key-%02d", i),
			VaultName: vault.Name,
			State:     keys.StateActive,
		})
	}

	model := New(nil)

	next, _ := model.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	next, _ = next.(Model).Update(vaultsLoadedMsg{vaults: []keys.Vault{vault}})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: vault, keys: items})

	return next.(Model)
}

func press(m Model, keystrokes ...tea.KeyMsg) Model {
	for _, stroke := range keystrokes {
		updated, _ := m.Update(stroke)
		m = updated.(Model)
	}

	return m
}

func runes(s string) tea.KeyMsg {
	return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(s)}
}

// TestVimNavigation covers the bindings that must behave the way vim's do.
func TestVimNavigation(t *testing.T) {
	const count = 30

	tests := []struct {
		name  string
		start int
		keys  []tea.KeyMsg
		want  int
	}{
		{
			name: "j moves down",
			keys: []tea.KeyMsg{runes("j")},
			want: 1,
		},
		{
			name:  "k moves up",
			start: 5,
			keys:  []tea.KeyMsg{runes("k")},
			want:  4,
		},
		{
			// vim's gg needs both presses.
			name:  "gg jumps to the top",
			start: 20,
			keys:  []tea.KeyMsg{runes("g"), runes("g")},
			want:  0,
		},
		{
			// A bare g is a prefix in vim and must not act on its own.
			name:  "a single g does nothing",
			start: 20,
			keys:  []tea.KeyMsg{runes("g")},
			want:  20,
		},
		{
			// An unrelated key cancels the prefix rather than being swallowed.
			name:  "g then j cancels the prefix and moves down",
			start: 20,
			keys:  []tea.KeyMsg{runes("g"), runes("j")},
			want:  21,
		},
		{
			name:  "G jumps to the bottom",
			start: 3,
			keys:  []tea.KeyMsg{runes("G")},
			want:  count - 1,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			model := listWith(t, count)
			model.list.Select(test.start)

			got := press(model, test.keys...).list.Index()
			if got != test.want {
				t.Errorf("cursor = %d, want %d", got, test.want)
			}
		})
	}
}

// TestVimPaging covers Ctrl-f/b/d/u. The step is derived from the model rather
// than hardcoded: it depends on the row height of the delegate, and pinning a
// literal here would only assert that two constants still multiply the same way.
func TestVimPaging(t *testing.T) {
	model := listWith(t, 40)
	page := model.visibleRows()
	half := max(1, page/2)

	if page < 2 {
		t.Fatalf("test needs a list taller than %d rows", page)
	}

	tests := []struct {
		name  string
		start int
		key   tea.KeyMsg
		want  int
	}{
		{"ctrl+f pages down", 0, tea.KeyMsg{Type: tea.KeyCtrlF}, page},
		{"ctrl+b pages up", 30, tea.KeyMsg{Type: tea.KeyCtrlB}, 30 - page},
		{"ctrl+d half pages down", 0, tea.KeyMsg{Type: tea.KeyCtrlD}, half},
		{"ctrl+u half pages up", 30, tea.KeyMsg{Type: tea.KeyCtrlU}, 30 - half},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			start := listWith(t, 40)
			start.list.Select(test.start)

			if got := press(start, test.key).list.Index(); got != test.want {
				t.Errorf("cursor = %d, want %d (page %d)", got, test.want, page)
			}
		})
	}
}

// TestPagingStopsAtTheEnds covers paging past either end, which must clamp
// rather than wrap or run off the list.
func TestPagingStopsAtTheEnds(t *testing.T) {
	const count = 5

	top := listWith(t, count)
	if got := press(top, tea.KeyMsg{Type: tea.KeyCtrlB}).list.Index(); got != 0 {
		t.Errorf("paging up from the top gave %d, want 0", got)
	}

	bottom := listWith(t, count)
	bottom.list.Select(count - 1)

	if got := press(bottom, tea.KeyMsg{Type: tea.KeyCtrlF}).list.Index(); got != count-1 {
		t.Errorf("paging down from the bottom gave %d, want %d", got, count-1)
	}
}

// TestBareLetterPagingIsUnbound guards the bindings bubbles ships by default.
// They page the list with no modifier, which is not how vim pages and was not
// documented anywhere in protui.
func TestBareLetterPagingIsUnbound(t *testing.T) {
	for _, keystroke := range []string{"f", "b", "u", "h", "l"} {
		model := listWith(t, 30)
		model.list.Select(10)

		after := press(model, runes(keystroke))

		if got := after.list.Index(); got != 10 {
			t.Errorf("%q moved the cursor to %d; it should be unbound", keystroke, got)
		}
		if after.mode != modeList {
			t.Errorf("%q changed the mode to %d", keystroke, after.mode)
		}
	}
}

// TestActionBindings pins the mnemonic action keys, which follow lazygit and
// k9s rather than vim. n is "new" there, and binding it to list movement here
// would only duplicate j: `/` filters rather than searches, so there is no
// hidden next match for it to reach.
func TestActionBindings(t *testing.T) {
	tests := []struct {
		key  string
		want mode
	}{
		{"n", modeCreate},
		{"a", modeAgent},
		{"d", modeConfirmTrash},
		{"D", modeConfirmDelete},
	}

	for _, test := range tests {
		t.Run(test.key, func(t *testing.T) {
			if got := press(listWith(t, 5), runes(test.key)).mode; got != test.want {
				t.Errorf("%q gave mode %d, want %d", test.key, got, test.want)
			}
		})
	}
}

// TestCopyAliases covers both keys bound to copying: c is the one the help bar
// advertises, y the vim alias kept alongside it.
func TestCopyAliases(t *testing.T) {
	for _, keystroke := range []string{"c", "y"} {
		model := listWith(t, 5)
		model.allKeys[0].PublicKey = "ssh-ed25519 AAAA test@example"
		model = model.rebuildList()

		if _, cmd := model.Update(runes(keystroke)); cmd == nil {
			t.Errorf("%q did not dispatch a copy", keystroke)
		}
	}
}

// TestPendingPrefixClearedByFiltering covers an armed g surviving into the
// filter, where it would otherwise swallow the next keystroke typed.
func TestPendingPrefixClearedByFiltering(t *testing.T) {
	model := listWith(t, 10)
	model.list.Select(5)

	armed := press(model, runes("g"))
	if !armed.pendingG {
		t.Fatal("g did not arm the prefix")
	}

	filtering := press(armed, runes("/"))
	if filtering.pendingG {
		t.Error("the g prefix survived into the filter")
	}
}
