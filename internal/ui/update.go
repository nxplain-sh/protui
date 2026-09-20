package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/nxplain-sh/protui/internal/passcli"
)

// Update is the root update loop. It performs no I/O of its own: every
// pass-cli call is returned as a tea.Cmd.
//
// Spinner ticks are handled here rather than in dispatch so that every other
// branch can start work without each one remembering to restart the animation.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if tick, ok := msg.(spinner.TickMsg); ok {
		// Let the loop lapse once the screen is static; it restarts below the
		// moment anything becomes outstanding again.
		if !m.busy() {
			m.spinnerRunning = false

			return m, nil
		}

		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(tick)

		return m, cmd
	}

	updated, cmd := m.dispatch(msg)

	// Resetting here rather than in each navigation branch means every path
	// that can move the cursor is covered, including the list's own handling.
	updated = updated.syncDetailScroll()

	next, spinCmd := updated.startSpinner()

	return next, tea.Batch(cmd, spinCmd)
}

// dispatch routes one message to its handler.
func (m Model) dispatch(msg tea.Msg) (Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		return m.resize(msg), nil

	case tea.KeyMsg:
		return m.handleKey(msg)

	case vaultsLoadedMsg:
		return m.handleVaultsLoaded(msg)

	case keysLoadedMsg:
		return m.handleKeysLoaded(msg)

	case publicKeyLoadedMsg:
		return m.handlePublicKeyLoaded(msg)

	case agentStatusMsg:
		m.agentBusy = false
		if msg.err != nil {
			return m.notify(msg.err.Error(), styleError)
		}
		m.agent = msg.status

		return m, nil

	case agentActionMsg:
		if msg.err != nil {
			m.agentBusy = false
			return m.notify(msg.err.Error(), styleError)
		}
		// Re-read the status rather than assuming the action took effect.
		return m, loadAgentStatus(m.client)

	case itemCreatedMsg:
		return m.handleItemCreated(msg)

	case itemRemovedMsg:
		return m.handleItemRemoved(msg)

	case copiedMsg:
		if msg.err != nil {
			return m.notify("Clipboard unavailable: "+msg.err.Error(), styleError)
		}

		return m.notify(fmt.Sprintf("Copied public key for %q.", msg.title), styleOK)

	case statusExpiredMsg:
		// Ignore a stale timer belonging to a superseded message.
		if msg.token == m.statusToken {
			m.status = ""
		}

		return m, nil
	}

	return m, nil
}

func (m Model) resize(msg tea.WindowSizeMsg) Model {
	m.width = msg.Width
	m.height = msg.Height
	m.help.Width = msg.Width

	listWidth := msg.Width
	if msg.Width >= detailPaneMinWidth {
		listWidth = msg.Width / 2
	}

	// Rows reserved for the header, the help bar and the status line.
	m.list.SetSize(listWidth, max(1, msg.Height-6))

	return m
}

// syncDetailScroll returns the detail pane to the top when the cursor lands on
// a different key, so a scrolled position never carries over to another item.
func (m Model) syncDetailScroll() Model {
	selected, ok := m.selected()

	id := ""
	if ok {
		id = selected.ShareID + "/" + selected.ID
	}

	if id != m.detailFor {
		m.detailFor = id
		m.detailOffset = 0
	}

	return m
}

// scrollDetail moves the detail pane by lines, clamped to its content.
func (m Model) scrollDetail(lines int) Model {
	if m.width < detailPaneMinWidth {
		return m
	}

	m.detailOffset = min(max(0, m.detailOffset+lines), m.maxDetailOffset())

	return m
}

// handleKey dispatches a key press to whichever mode owns it.
func (m Model) handleKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch m.mode {
	case modeCreate:
		return m.handleCreateKey(msg)
	case modeConfirmTrash:
		return m.handleConfirmTrashKey(msg)
	case modeConfirmDelete:
		return m.handleConfirmDeleteKey(msg)
	case modeAgent:
		return m.handleAgentKey(msg)
	}

	return m.handleListKey(msg)
}

func (m Model) handleListKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	// While a filter is being typed, every key belongs to the list.
	if m.list.FilterState() == list.Filtering {
		m.pendingG = false

		var cmd tea.Cmd
		m.list, cmd = m.list.Update(msg)

		return m, cmd
	}

	// vim's gg. The first g only arms the prefix; the second completes it, and
	// anything else cancels it and is handled normally.
	if m.pendingG {
		m.pendingG = false

		if key.Matches(msg, m.keymap.Top) {
			m.list.Select(0)

			return m, nil
		}
	} else if key.Matches(msg, m.keymap.Top) {
		m.pendingG = true

		return m, nil
	}

	switch {
	case key.Matches(msg, m.keymap.Quit):
		return m, tea.Quit

	case key.Matches(msg, m.keymap.Bottom):
		m.list.Select(max(0, len(m.list.VisibleItems())-1))

		return m, nil

	case key.Matches(msg, m.keymap.PageDown):
		return m.moveCursor(m.visibleRows()), nil

	case key.Matches(msg, m.keymap.PageUp):
		return m.moveCursor(-m.visibleRows()), nil

	case key.Matches(msg, m.keymap.HalfDown):
		return m.moveCursor(max(1, m.visibleRows()/2)), nil

	case key.Matches(msg, m.keymap.HalfUp):
		return m.moveCursor(-max(1, m.visibleRows()/2)), nil

	case key.Matches(msg, m.keymap.ScrollDown):
		return m.scrollDetail(1), nil

	case key.Matches(msg, m.keymap.ScrollUp):
		return m.scrollDetail(-1), nil

	case key.Matches(msg, m.keymap.Help):
		m.help.ShowAll = !m.help.ShowAll

		return m, nil

	case key.Matches(msg, m.keymap.Refresh):
		return m.reload()

	case key.Matches(msg, m.keymap.Agent):
		m.mode = modeAgent
		m.agentBusy = true

		return m, loadAgentStatus(m.client)

	case key.Matches(msg, m.keymap.New):
		if len(m.vaults) == 0 {
			return m.notify("No vaults available yet.", styleWarn)
		}
		m.mode = modeCreate
		m.form = newCreateForm(m.vaults, m.selectedVault())

		return m, textinput.Blink

	case key.Matches(msg, m.keymap.Copy):
		selected, ok := m.selected()
		if !ok {
			return m, nil
		}
		if selected.PublicKey == "" {
			return m.notify("No public key loaded for this item yet.", styleWarn)
		}

		return m, copyPublicKey(selected)

	case key.Matches(msg, m.keymap.Trash):
		selected, ok := m.selected()
		if !ok {
			return m, nil
		}
		m.target = selected
		m.mode = modeConfirmTrash

		return m, nil

	case key.Matches(msg, m.keymap.Delete):
		selected, ok := m.selected()
		if !ok {
			return m, nil
		}
		m.target = selected
		m.mode = modeConfirmDelete
		m.confirmInput.Reset()
		m.confirmInput.Focus()

		return m, textinput.Blink
	}

	var cmd tea.Cmd
	m.list, cmd = m.list.Update(msg)

	return m, cmd
}

func (m Model) handleCreateKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	if msg.Type == tea.KeyEsc {
		m.mode = modeList

		return m, nil
	}

	updated, cmd, submitted := m.form.Update(msg)
	m.form = updated

	if !submitted {
		return m, cmd
	}

	request := m.form.request()

	// Drop the whole form, not just the passphrase field: from here the client
	// owns the secret and protui keeps no copy.
	m.form = createForm{}
	m.mode = modeList

	m, notifyCmd := m.notify(fmt.Sprintf("Creating %q…", request.Title), styleMuted)

	return m, tea.Batch(generateKey(m.client, request), notifyCmd)
}

func (m Model) handleConfirmTrashKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch strings.ToLower(msg.String()) {
	case "y":
		m.mode = modeList

		return m, trashKey(m.client, m.target)

	case "n", "esc", "q":
		m.mode = modeList

		return m, nil
	}

	return m, nil
}

// handleConfirmDeleteKey guards the permanent delete. Upstream offers no undo
// for it, so the exact title must be typed rather than a single keypress.
func (m Model) handleConfirmDeleteKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.Type {
	case tea.KeyEsc:
		m.mode = modeList
		m.confirmInput.Reset()

		return m, nil

	case tea.KeyEnter:
		if m.confirmInput.Value() != m.target.Title {
			return m.notify("Title does not match; nothing was deleted.", styleWarn)
		}
		m.mode = modeList
		m.confirmInput.Reset()

		return m, deleteKey(m.client, m.target)
	}

	var cmd tea.Cmd
	m.confirmInput, cmd = m.confirmInput.Update(msg)

	return m, cmd
}

func (m Model) handleAgentKey(msg tea.KeyMsg) (Model, tea.Cmd) {
	switch msg.String() {
	case "esc", "q", "a":
		m.mode = modeList

		return m, nil

	case "s":
		if m.agent.State == passcli.AgentRunning {
			return m.notify("The agent is already running.", styleWarn)
		}
		m.agentBusy = true

		return m, startAgent(m.client)

	case "x":
		if m.agent.State == passcli.AgentStopped {
			return m.notify("The agent is not running.", styleWarn)
		}
		m.agentBusy = true

		return m, stopAgent(m.client)

	case "r":
		m.agentBusy = true

		return m, loadAgentStatus(m.client)
	}

	return m, nil
}

func (m Model) handleVaultsLoaded(msg vaultsLoadedMsg) (Model, tea.Cmd) {
	if msg.err != nil {
		m.loading = false
		m.err = msg.err

		return m, nil
	}

	m.vaults = msg.vaults
	m.err = nil

	if len(m.vaults) == 0 {
		m.loading = false

		return m, nil
	}

	// pass-cli cannot list across vaults, so fan out and let each report
	// independently.
	m.vaultsLoading = len(m.vaults)

	cmds := make([]tea.Cmd, 0, len(m.vaults))
	for _, vault := range m.vaults {
		cmds = append(cmds, loadKeys(m.client, vault))
	}

	return m, tea.Batch(cmds...)
}

func (m Model) handleKeysLoaded(msg keysLoadedMsg) (Model, tea.Cmd) {
	m.vaultsLoading--
	if m.vaultsLoading <= 0 {
		m.loading = false
	}

	if msg.err != nil {
		// One unreachable vault must not blank the list, so this is surfaced
		// without discarding what the others returned.
		return m.notify(fmt.Sprintf("Vault %q: %v", msg.vault.Name, msg.err), styleError)
	}

	m.allKeys = append(m.allKeys, msg.keys...)
	m = m.rebuildList()

	// Algorithm and fingerprint need the public key, which the list output
	// does not carry; queue one fetch per item.
	m.pending = append(m.pending, msg.keys...)

	return m.nextPublicKeyFetches()
}

func (m Model) handlePublicKeyLoaded(msg publicKeyLoadedMsg) (Model, tea.Cmd) {
	m.inflight--

	if msg.err == nil {
		for _, existing := range m.allKeys {
			if existing.ID != msg.itemID || existing.ShareID != msg.shareID {
				continue
			}

			// WithPublicKey reports a malformed key but still returns a usable
			// row, so the error is deliberately not escalated to the user here.
			updated, _ := existing.WithPublicKey(msg.publicKey)
			m = m.replaceKey(updated)

			break
		}
	}

	return m.nextPublicKeyFetches()
}

func (m Model) handleItemCreated(msg itemCreatedMsg) (Model, tea.Cmd) {
	if msg.err != nil {
		return m.notify(msg.err.Error(), styleError)
	}

	m, notifyCmd := m.notify(fmt.Sprintf("Created %q.", msg.title), styleOK)
	m, reloadCmd := m.reload()

	return m, tea.Batch(reloadCmd, notifyCmd)
}

func (m Model) handleItemRemoved(msg itemRemovedMsg) (Model, tea.Cmd) {
	if msg.err != nil {
		return m.notify(msg.err.Error(), styleError)
	}

	verb := "Moved to trash"
	if msg.permanent {
		verb = "Permanently deleted"
	}

	m, notifyCmd := m.notify(fmt.Sprintf("%s %q.", verb, msg.title), styleOK)
	m, reloadCmd := m.reload()

	return m, tea.Batch(reloadCmd, notifyCmd)
}

// reload discards the current list and re-reads everything from pass-cli.
func (m Model) reload() (Model, tea.Cmd) {
	m.allKeys = nil
	m.pending = nil
	m.inflight = 0
	m.vaultsLoading = 0
	m.loading = true
	m.err = nil
	m.list.SetItems(nil)

	return m, loadVaults(m.client)
}
