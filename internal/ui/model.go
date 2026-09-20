// Package ui holds protui's Bubble Tea models.
//
// The update loop performs no I/O: every pass-cli call is dispatched as a
// tea.Cmd (see commands.go) and reported back as a message. Only public key
// material ever reaches this package.
package ui

import (
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/charmbracelet/bubbles/help"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

// maxPublicKeyFetches bounds how many `item view` calls run at once.
//
// Deriving the algorithm and fingerprint needs one call per item, since the
// list output carries no key material. Fetching them all at once would spawn a
// process per key, so they are drained through a small window instead.
const maxPublicKeyFetches = 4

// detailPaneMinWidth is the terminal width below which the detail pane is
// dropped and the list gets the full width.
const detailPaneMinWidth = 92

// detailChrome is the detail pane's left border plus its left padding. lipgloss
// wraps content inside the width it is given, so this has to come off before
// the pane is told how much room its text has.
const detailChrome = 3

type mode int

const (
	modeList mode = iota
	modeCreate
	modeConfirmTrash
	modeConfirmDelete
	modeAgent
)

// keyItem adapts a domain key for the bubbles list.
type keyItem struct{ key keys.Key }

// FilterValue is what the fuzzy filter matches against. Comment and
// fingerprint are included so a key can be found by any of the things a user
// might remember about it.
func (i keyItem) FilterValue() string {
	return strings.Join([]string{
		i.key.Title,
		i.key.VaultName,
		i.key.Comment,
		i.key.Label(),
		i.key.ShortFingerprint(),
	}, " ")
}

// summary is the dimmed second line of a row: where the key lives and what it
// is. Empty parts are dropped so the separator never dangles.
//
// Until the public key arrives the algorithm and fingerprint are unknown, and
// saying so beats printing a confident "unknown" for a key that is merely still
// loading.
func (i keyItem) summary() string {
	parts := []string{i.key.VaultName}

	if i.key.PublicKey == "" {
		parts = append(parts, "loading…")
	} else {
		parts = append(parts, i.key.Label(), i.key.ShortFingerprint())
	}

	kept := parts[:0]
	for _, part := range parts {
		if part != "" {
			kept = append(kept, part)
		}
	}

	return strings.Join(kept, " "+glyphSeparator+" ")
}

// keyDelegate renders each key as two lines: the title, then its metadata
// dimmed beneath it.
//
// Two lines rather than aligned columns, because a key list is scanned by name
// and columns force every field to the width of its longest value — one long
// vault name would push fingerprints off the edge of every other row.
type keyDelegate struct{}

func (keyDelegate) Height() int  { return 2 }
func (keyDelegate) Spacing() int { return 1 }

func (keyDelegate) Update(tea.Msg, *list.Model) tea.Cmd { return nil }

func (keyDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	entry, ok := item.(keyItem)
	if !ok {
		return
	}

	// The gutter carries the selection bar; both lines indent past it so the
	// block of text stays aligned whether or not the row is selected.
	width := max(12, m.Width()-4)
	gutter := "  "

	title := styleText.Render(truncate(entry.key.Title, width))
	summary := styleFaint.Render(truncate(entry.summary(), width))

	if index == m.Index() {
		gutter = styleAccent.Render(glyphSelected) + " "
		title = styleSelected.Render(truncate(entry.key.Title, width))
		// The selected row's metadata is lifted one step out of the background
		// so it reads as part of the selection rather than as greyed out.
		summary = styleMuted.Render(truncate(entry.summary(), width))
	}

	// The delegate interface gives no way to report a write failure, and the
	// writer is the list's own render buffer, so the error is discarded
	// explicitly rather than silently.
	_, _ = fmt.Fprint(w, gutter+title+"\n  "+summary)
}

// Model is protui's root Bubble Tea model.
type Model struct {
	client *passcli.Client
	keymap keyMap
	help   help.Model
	list   list.Model

	mode mode
	form createForm

	// confirmInput takes the typed title required to permanently delete.
	confirmInput textinput.Model

	vaults  []keys.Vault
	allKeys []keys.Key

	// pending holds keys still awaiting a public key fetch; inflight counts
	// those in progress.
	pending  []keys.Key
	inflight int

	// vaultsLoading counts vaults whose item list has not come back yet.
	vaultsLoading int

	agent     passcli.AgentStatus
	agentBusy bool

	// target is the key a confirmation prompt refers to.
	target keys.Key

	// pendingG is set between the two presses of vim's gg. A bare g is a
	// prefix in vim and never acts alone, so the first press only arms this.
	pendingG bool

	// detailOffset scrolls the detail pane, which on a short terminal holds
	// more than it can show. detailFor is the item that offset belongs to, so
	// moving to another key starts it at the top again.
	detailOffset int
	detailFor    string

	// spinner animates while work is outstanding. spinnerRunning tracks whether
	// its tick loop is live, so it is not restarted on every message and stops
	// once the screen is static.
	spinner        spinner.Model
	spinnerRunning bool

	width  int
	height int

	loading bool
	err     error

	status      string
	statusStyle lipgloss.Style
	statusToken int
}

// New builds the root model.
func New(client *passcli.Client) Model {
	delegate := keyDelegate{}

	keyList := list.New(nil, delegate, 0, 0)
	keyList.SetShowTitle(false)
	keyList.SetShowStatusBar(false)
	keyList.SetShowHelp(false)
	keyList.SetFilteringEnabled(true)
	keyList.FilterInput.Prompt = "/"
	keyList.SetStatusBarItemName("key", "keys")

	// protui owns navigation, so the list keeps only filtering. bubbles binds
	// paging to bare f/b/u/h/l by default, which is neither vim (which pages
	// with Ctrl held) nor something protui documents, so those are unbound
	// rather than left live and undiscoverable.
	keyList.KeyMap.CursorUp.SetKeys("k", "up")
	keyList.KeyMap.CursorDown.SetKeys("j", "down")
	keyList.KeyMap.NextPage.Unbind()
	keyList.KeyMap.PrevPage.Unbind()
	keyList.KeyMap.GoToStart.Unbind()
	keyList.KeyMap.GoToEnd.Unbind()
	keyList.KeyMap.Filter.SetKeys("/")
	keyList.KeyMap.Quit.SetKeys("q")
	keyList.KeyMap.ShowFullHelp.Unbind()
	keyList.KeyMap.CloseFullHelp.Unbind()

	confirm := textinput.New()
	confirm.Placeholder = "type the title to confirm"
	confirm.CharLimit = 128
	confirm.Prompt = "› "
	confirm.PromptStyle = styleError
	confirm.TextStyle = styleText
	confirm.PlaceholderStyle = styleFaint

	// MiniDot is a single braille cell: it animates in place without the width
	// of the frame changing, so nothing beside it shifts.
	load := spinner.New()
	load.Spinner = spinner.MiniDot
	load.Style = styleAccent

	helpModel := help.New()
	helpModel.Styles.ShortKey, helpModel.Styles.ShortDesc, helpModel.Styles.ShortSeparator = helpStyles()
	helpModel.Styles.FullKey, helpModel.Styles.FullDesc, helpModel.Styles.FullSeparator = helpStyles()
	helpModel.Styles.Ellipsis = styleFaint

	return Model{
		client:         client,
		keymap:         defaultKeyMap(),
		help:           helpModel,
		list:           keyList,
		confirmInput:   confirm,
		spinner:        load,
		spinnerRunning: true,
		loading:        true,
	}
}

// moveCursor steps the list by n rows, negative for up. It is how the paging
// bindings are implemented, since bubbles exposes cursor movement but no
// half-page jump of its own.
func (m Model) moveCursor(rows int) Model {
	for i := 0; i < rows; i++ {
		m.list.CursorDown()
	}
	for i := 0; i > rows; i-- {
		m.list.CursorUp()
	}

	return m
}

// visibleRows is how many items fit on screen, used to size a page jump. The
// delegate draws each item over Height plus Spacing lines.
func (m Model) visibleRows() int {
	delegate := keyDelegate{}

	return max(1, m.list.Height()/(delegate.Height()+delegate.Spacing()))
}

// busy reports whether anything is outstanding, which is what the spinner
// animates and several views key their placeholder text off.
func (m Model) busy() bool {
	return m.loading || m.agentBusy || m.inflight > 0
}

// startSpinner kicks off the tick loop if work is in flight and it is not
// already running. Returning a nil command when it is already live keeps a
// single loop rather than compounding one per message.
func (m Model) startSpinner() (Model, tea.Cmd) {
	if !m.busy() || m.spinnerRunning {
		return m, nil
	}

	m.spinnerRunning = true

	return m, m.spinner.Tick
}

// Init starts the initial load. Vaults come first because pass-cli cannot list
// items without a vault selector.
func (m Model) Init() tea.Cmd {
	return tea.Batch(loadVaults(m.client), loadAgentStatus(m.client), m.spinner.Tick)
}

// Model methods all take value receivers and return the updated Model, matching
// how Bubble Tea threads state through Update. Mixing pointer and value
// receivers on one type is what the Go style guide warns against.

// rebuildList sorts allKeys and refreshes the list from it.
func (m Model) rebuildList() Model {
	sort.SliceStable(m.allKeys, func(i, j int) bool {
		if m.allKeys[i].VaultName != m.allKeys[j].VaultName {
			return m.allKeys[i].VaultName < m.allKeys[j].VaultName
		}

		return strings.ToLower(m.allKeys[i].Title) < strings.ToLower(m.allKeys[j].Title)
	})

	items := make([]list.Item, 0, len(m.allKeys))
	for _, key := range m.allKeys {
		items = append(items, keyItem{key: key})
	}

	m.list.SetItems(items)

	return m
}

// replaceKey swaps one key into both the backing slice and the list, leaving
// any active filter and the cursor position intact.
func (m Model) replaceKey(updated keys.Key) Model {
	for i := range m.allKeys {
		if m.allKeys[i].ID == updated.ID && m.allKeys[i].ShareID == updated.ShareID {
			m.allKeys[i] = updated

			break
		}
	}

	for i, item := range m.list.Items() {
		entry, ok := item.(keyItem)
		if !ok {
			continue
		}

		if entry.key.ID == updated.ID && entry.key.ShareID == updated.ShareID {
			m.list.SetItem(i, keyItem{key: updated})

			break
		}
	}

	return m
}

// selected returns the highlighted key.
func (m Model) selected() (keys.Key, bool) {
	item, ok := m.list.SelectedItem().(keyItem)
	if !ok {
		return keys.Key{}, false
	}

	return item.key, true
}

// selectedVault reports the vault of the highlighted key, so the create form
// can default to it.
func (m Model) selectedVault() keys.Vault {
	if key, ok := m.selected(); ok {
		for _, vault := range m.vaults {
			if vault.ShareID == key.ShareID {
				return vault
			}
		}
	}

	if len(m.vaults) > 0 {
		return m.vaults[0]
	}

	return keys.Vault{}
}

// nextPublicKeyFetches drains the pending queue up to the concurrency window.
func (m Model) nextPublicKeyFetches() (Model, tea.Cmd) {
	var cmds []tea.Cmd

	for m.inflight < maxPublicKeyFetches && len(m.pending) > 0 {
		next := m.pending[0]
		m.pending = m.pending[1:]
		m.inflight++
		cmds = append(cmds, loadPublicKey(m.client, next))
	}

	if len(cmds) == 0 {
		return m, nil
	}

	return m, tea.Batch(cmds...)
}

// notify sets a transient status line and schedules its expiry.
func (m Model) notify(text string, style lipgloss.Style) (Model, tea.Cmd) {
	m.statusToken++
	m.status = text
	m.statusStyle = style

	return m, expireStatus(m.statusToken)
}

// truncate shortens s to fit width display columns, marking the cut with an
// ellipsis.
func truncate(s string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= width {
		return s
	}
	if width == 1 {
		return "…"
	}

	runes := []rune(s)
	if len(runes) > width-1 {
		runes = runes[:width-1]
	}

	return string(runes) + "…"
}
