package ui

import (
	"errors"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

// View renders the whole screen.
//
// Only public key material is ever drawn: protui has no code path that reads a
// private key, so none can reach the terminal.
func (m Model) View() string {
	if m.width == 0 {
		return "Loading…"
	}

	switch m.mode {
	case modeCreate:
		return m.centered(m.form.View())
	case modeConfirmTrash:
		return m.centered(m.trashPrompt())
	case modeConfirmDelete:
		return m.centered(m.deletePrompt())
	case modeAgent:
		return m.centered(m.agentPanel())
	}

	return m.listView()
}

func (m Model) listView() string {
	if m.err != nil {
		return lipgloss.JoinVertical(lipgloss.Left, m.header(), m.errorBody(), m.footer())
	}

	// With nothing in the list there is no selected key for a detail pane to
	// describe, so the loading and empty states take the full width rather than
	// sitting in one half beside an empty pane.
	if len(m.list.Items()) == 0 {
		return lipgloss.JoinVertical(lipgloss.Left, m.header(), m.emptyBody(), m.footer())
	}

	body := m.list.View()

	// The detail pane sits beside the list when there is room, and is dropped
	// entirely when there is not: below that width, splitting leaves neither
	// side readable.
	if m.width >= detailPaneMinWidth {
		slot := m.detailPaneWidth()
		detail := styleDetail.Width(slot).Render(m.detailWindow())
		body = lipgloss.JoinHorizontal(lipgloss.Top, body, detail)
	}

	return lipgloss.JoinVertical(lipgloss.Left, m.header(), body, m.footer())
}

// header is a wordmark on the left and a count on the right, over a rule.
// There is no filled banner: a block of colour across the top reads as chrome,
// and the list is what deserves the attention.
func (m Model) header() string {
	left := styleAccent.Render(glyphMark) + " " + styleWordmark.Render("protui")

	right := styleFaint.Render(m.headerCount())

	gap := max(1, m.width-lipgloss.Width(left)-lipgloss.Width(right)-4)

	line := "  " + left + strings.Repeat(" ", gap) + right

	return line + "\n" + "  " + rule(max(1, m.width-4)) + "\n"
}

// headerCount reports where the cursor is as well as how many keys there are.
// Only a fraction of a long list fits on screen and nothing else indicates
// position, so a bare total would leave a user unable to tell whether more
// exists below.
func (m Model) headerCount() string {
	if m.busy() && len(m.list.Items()) == 0 {
		return m.spinner.View() + " loading"
	}

	total := len(m.list.Items())
	if total == 0 {
		return "no keys"
	}

	visible := len(m.list.VisibleItems())
	position := min(m.list.Index()+1, visible)

	// While a filter is applied, both numbers matter: where you are in the
	// matches, and how much of the collection they represent.
	if m.list.FilterValue() != "" {
		return fmt.Sprintf("%d/%d of %d keys", position, visible, total)
	}

	if total == 1 {
		return "1 key"
	}

	return fmt.Sprintf("%d/%d keys", position, total)
}

// footer carries the transient status line above the help bar, separated from
// the body by a rule.
func (m Model) footer() string {
	var lines []string

	lines = append(lines, "  "+rule(max(1, m.width-4)))

	if m.status != "" {
		lines = append(lines, "  "+m.statusStyle.Render(truncate(m.status, max(10, m.width-4))))
	}

	// The list owns the filter prompt while it is being typed.
	if m.list.FilterState() == list.Filtering {
		lines = append(lines, "  "+m.list.FilterInput.View())
	} else {
		lines = append(lines, "  "+m.help.View(m.keymap))
	}

	return "\n" + strings.Join(lines, "\n")
}

// bodyHeight is the room between the header and the footer.
func (m Model) bodyHeight() int {
	return max(1, m.height-6)
}

// centreBody centres a short message in the space the list would occupy, so a
// state with no list does not sit hard against the top-left corner.
func (m Model) centreBody(content string) string {
	return lipgloss.Place(
		m.width, m.bodyHeight(),
		lipgloss.Center, lipgloss.Center,
		lipgloss.NewStyle().Align(lipgloss.Center).Render(content),
	)
}

// emptyBody covers both states where the list has nothing in it: still loading,
// and genuinely empty.
func (m Model) emptyBody() string {
	if m.loading {
		return m.centreBody(styleFaint.Render(m.spinner.View() + " Loading keys…"))
	}

	return m.centreBody(
		styleMuted.Render("No SSH keys found.") + "\n\n" +
			styleFaint.Render("Press ") + styleAccent.Render("n") + styleFaint.Render(" to create one."),
	)
}

func (m Model) errorBody() string {
	return m.centreBody(
		styleError.Render(glyphError+" "+m.err.Error()) + "\n\n" +
			styleFaint.Render("Press ") + styleAccent.Render("r") + styleFaint.Render(" to retry, ") +
			styleAccent.Render("q") + styleFaint.Render(" to quit."),
	)
}

// detailPaneWidth is the horizontal slot the detail pane occupies, border and
// padding included.
func (m Model) detailPaneWidth() int {
	return m.width - m.list.Width() - 2
}

// detailInnerWidth is the room the pane's text actually has.
func (m Model) detailInnerWidth() int {
	return m.detailPaneWidth() - detailChrome
}

// maxDetailOffset is how far the pane can scroll before its last line is in
// view. Zero when everything already fits.
func (m Model) maxDetailOffset() int {
	overflow := len(m.detailLines()) - m.bodyHeight()

	return max(0, overflow)
}

// detailWindow is the slice of the detail pane currently on screen, with
// markers when there is more above or below.
//
// The pane is clamped rather than allowed to overflow: a taller-than-terminal
// block would be cut by the terminal itself, silently and from the bottom.
func (m Model) detailWindow() string {
	lines := m.detailLines()
	height := m.bodyHeight()

	if len(lines) <= height {
		return strings.Join(lines, "\n")
	}

	offset := min(m.detailOffset, m.maxDetailOffset())
	window := lines[offset:min(offset+height, len(lines))]

	// The markers replace a line rather than being added to one, so the pane
	// never grows past the height it was given.
	if offset > 0 {
		window[0] = styleFaint.Render("↑ " + fmt.Sprintf("%d more", offset))
	}
	if offset+height < len(lines) {
		window[len(window)-1] = styleFaint.Render(
			fmt.Sprintf("↓ %d more · ^e/^y scrolls", len(lines)-offset-height),
		)
	}

	return strings.Join(window, "\n")
}

// detailLines renders the pane for the highlighted key, one string per line.
//
// It is a pure function of the model so the update loop can measure it when
// clamping a scroll, without View having to record anything.
func (m Model) detailLines() []string {
	return strings.Split(m.detail(m.detailInnerWidth()), "\n")
}

// detail renders the pane for the highlighted key.
func (m Model) detail(width int) string {
	selected, ok := m.selected()
	if !ok {
		return styleFaint.Render("No key selected.")
	}

	var b strings.Builder

	b.WriteString(styleSelected.Render(truncate(selected.Title, width)))
	b.WriteString("\n")

	// The algorithm is a chip rather than another label/value row: it is the
	// one attribute worth recognising at a glance.
	chip := styleBadge.Render(selected.Label())
	if selected.PublicKey == "" {
		chip = styleFaint.Render(m.spinner.View() + " reading key")
	}

	b.WriteString(chip)

	if selected.State == keys.StateTrashed {
		b.WriteString(" " + styleWarn.Render("trashed"))
	}

	b.WriteString("\n\n")

	b.WriteString(detailRow("Vault", selected.VaultName, width))

	// The fingerprint wraps rather than truncating. Comparing one against
	// GitHub or `ssh-keygen -lf` is a main reason to open this pane, and a
	// fingerprint with its middle elided cannot be compared at all.
	b.WriteString(detailWrapped("Fingerprint", m.orPending(selected.Fingerprint), width))
	b.WriteString(detailWrapped("Comment", m.commentOrDash(selected), width))

	if !selected.CreatedAt.IsZero() {
		// Upstream timestamps carry no zone, so they are shown as the
		// wall-clock values they are rather than converted.
		b.WriteString(detailRow("Created", selected.CreatedAt.Format("2006-01-02 15:04"), width))
	}
	if !selected.ModifiedAt.IsZero() {
		b.WriteString(detailRow("Modified", selected.ModifiedAt.Format("2006-01-02 15:04"), width))
	}

	// Item IDs are long base64; truncating one makes it useless for passing
	// back to pass-cli, which is the only reason to show it.
	b.WriteString(detailWrapped("Item ID", selected.ID, width))

	b.WriteString("\n")
	b.WriteString(styleSection.Render("PUBLIC KEY"))
	b.WriteString("\n")

	if selected.PublicKey == "" {
		b.WriteString(styleFaint.Render(m.spinner.View() + " loading…"))
	} else {
		b.WriteString(styleMuted.Render(wrap(selected.PublicKey, max(20, width-2))))
	}

	return b.String()
}

// orPending shows a spinner in place of a value that is still being fetched,
// so a blank field never reads as "this key has none".
func (m Model) orPending(value string) string {
	if value == "" {
		return styleFaint.Render(m.spinner.View() + " …")
	}

	return value
}

func (m Model) commentOrDash(key keys.Key) string {
	if key.PublicKey == "" {
		return styleFaint.Render(m.spinner.View() + " …")
	}
	if key.Comment == "" {
		return styleFaint.Render("—")
	}

	return key.Comment
}

// detailLabelWidth is the left column shared by every label/value row.
const detailLabelWidth = 13

// detailRow is one label/value line, truncated to fit. Used for values short
// enough that eliding them loses nothing: a vault name, a timestamp.
func detailRow(label, value string, width int) string {
	return detailLabel(label) + truncate(value, detailValueWidth(width)) + "\n"
}

// detailWrapped is a label/value row whose value continues on further lines,
// indented under the value column. Used where the whole value matters.
func detailWrapped(label, value string, width int) string {
	lines := strings.Split(wrap(value, detailValueWidth(width)), "\n")

	var b strings.Builder

	b.WriteString(detailLabel(label) + lines[0] + "\n")

	for _, line := range lines[1:] {
		b.WriteString(strings.Repeat(" ", detailLabelWidth) + line + "\n")
	}

	return b.String()
}

func detailLabel(label string) string {
	rendered := styleLabel.Render(label)

	return rendered + strings.Repeat(" ", max(1, detailLabelWidth-lipgloss.Width(rendered)))
}

func detailValueWidth(width int) int {
	return max(10, width-detailLabelWidth-1)
}

// dialog frames a modal as title, rule, body, key hints, so every prompt has
// the same shape. title arrives already styled, since its colour is what
// distinguishes a question from a warning.
func dialog(frame lipgloss.Style, title, body, hints string) string {
	// The rule spans the widest line so the header separator reaches the edge
	// of the content rather than stopping under the title.
	width := lipgloss.Width(title)
	for _, line := range strings.Split(body+"\n"+hints, "\n") {
		width = max(width, lipgloss.Width(line))
	}

	var b strings.Builder

	b.WriteString(title)
	b.WriteString("\n")
	b.WriteString(rule(width))
	b.WriteString("\n\n")
	b.WriteString(body)
	b.WriteString("\n\n")
	b.WriteString(styleFaint.Render(hints))

	return frame.Render(b.String())
}

func (m Model) trashPrompt() string {
	body := lipgloss.JoinVertical(
		lipgloss.Left,
		styleSelected.Render(m.target.Title),
		styleFaint.Render("in "+m.target.VaultName),
		"",
		styleMuted.Render("Restore it later with:"),
		styleAccent.Render("pass-cli item untrash"),
	)

	return dialog(
		styleDialog,
		styleText.Bold(true).Render("Move to trash?"),
		body,
		"y  trash    n  cancel",
	)
}

func (m Model) deletePrompt() string {
	body := lipgloss.JoinVertical(
		lipgloss.Left,
		styleSelected.Render(m.target.Title),
		styleFaint.Render("in "+m.target.VaultName),
		"",
		styleError.Render("This cannot be undone. The key is destroyed,"),
		styleError.Render("not moved to the trash."),
		"",
		styleMuted.Render("Type the title to confirm:"),
		m.confirmInput.View(),
	)

	return dialog(
		styleDanger,
		styleError.Bold(true).Render(glyphWarn+"  Permanently delete?"),
		body,
		"enter  delete    esc  cancel    d  trashes instead",
	)
}

func (m Model) agentPanel() string {
	var rows []string

	if m.agentBusy {
		rows = append(rows, styleFaint.Render(m.spinner.View()+" Checking…"))
	} else {
		rows = append(rows, agentStateLine(m.agent))

		if m.agent.PID != "" {
			rows = append(rows, strings.TrimRight(detailRow("PID", m.agent.PID, 60), "\n"))
		}
		if m.agent.Socket != "" {
			rows = append(rows, strings.TrimRight(detailRow("Socket", m.agent.Socket, 60), "\n"))
		}
	}

	if m.agent.State == passcli.AgentRunning && m.agent.Socket != "" {
		rows = append(rows,
			"",
			styleSection.Render("POINT SSH AT IT"),
			styleMuted.Render("export SSH_AUTH_SOCK="+m.agent.Socket),
		)
	}

	return dialog(
		styleDialog,
		styleText.Bold(true).Render("SSH agent daemon"),
		lipgloss.JoinVertical(lipgloss.Left, rows...),
		"s  start    x  stop    r  refresh    esc  back",
	)
}

// agentStateLine renders the daemon state as a coloured dot plus its label, so
// the state is legible before the text is read.
func agentStateLine(status passcli.AgentStatus) string {
	label := styleLabel.Render("Status") + strings.Repeat(" ", 7)

	detail := status.Detail

	switch status.State {
	case passcli.AgentRunning:
		return label + styleOK.Render(glyphRunning+" running")
	case passcli.AgentDegraded:
		return label + styleWarn.Render(glyphDegraded+" "+detail)
	case passcli.AgentStopped:
		if detail == "" {
			detail = "stopped"
		}

		return label + styleMuted.Render(glyphStopped+" "+detail)
	default:
		if detail == "" {
			detail = "unknown"
		}

		return label + styleWarn.Render(glyphStopped+" "+detail)
	}
}

// centered places a dialog in the middle of the screen.
func (m Model) centered(content string) string {
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, content)
}

// wrap hard-wraps a public key into a block of the given width.
//
// It breaks mid-token on purpose. A key is one long base64 run with no useful
// break points, so wrapping on whitespace would strand the short "ssh-rsa"
// prefix on a line of its own and leave the body ragged.
//
// It splits on runes rather than bytes: the body is ASCII, but the trailing
// comment is arbitrary user text, and slicing that by byte offset would cut a
// multi-byte rune in half.
func wrap(s string, width int) string {
	if width <= 0 {
		return s
	}

	runes := []rune(strings.Join(strings.Fields(s), " "))

	var b strings.Builder

	for len(runes) > width {
		b.WriteString(string(runes[:width]))
		b.WriteString("\n")
		runes = runes[width:]
	}

	b.WriteString(string(runes))

	return b.String()
}

// FatalMessage renders a startup failure. It is used before the Bubble Tea
// program starts, so it returns plain text rather than a model.
func FatalMessage(err error) string {
	var b strings.Builder

	b.WriteString(styleError.Render(glyphError + " protui cannot start."))
	b.WriteString("\n\n")

	switch {
	case errors.Is(err, passcli.ErrNotInstalled):
		b.WriteString(passcli.Binary + " is not installed, or is not on your PATH.\n\n")
		b.WriteString(styleMuted.Render("protui is a front-end for it, so install it first:") + "\n")
		b.WriteString(styleAccent.Render("  https://protonpass.github.io/pass-cli/") + "\n")

	case errors.Is(err, passcli.ErrNoSession):
		b.WriteString(passcli.Binary + " is installed but has no valid session.\n\n")
		b.WriteString(styleMuted.Render("Authenticate first:") + "\n")
		b.WriteString(styleAccent.Render("  "+passcli.Binary+" login") + "\n")

	default:
		b.WriteString(err.Error())
		b.WriteString("\n")
	}

	return b.String()
}
