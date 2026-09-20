package ui

import (
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

// Form field indices, in tab order.
const (
	fieldTitle = iota
	fieldVault
	fieldKeyType
	fieldComment
	fieldPassphrase
	fieldCount
)

// createForm collects the inputs for `item create ssh-key generate`.
//
// The passphrase is optional. When set it reaches pass-cli through the child
// environment only — never argv, never disk. See docs/schema.md §6.1.
type createForm struct {
	focus int

	title      textinput.Model
	comment    textinput.Model
	passphrase textinput.Model

	vaults     []keys.Vault
	vaultIndex int

	keyTypeIndex int

	err string
}

// newFormInput builds a text input with the form's shared styling.
func newFormInput(placeholder string, limit int) textinput.Model {
	input := textinput.New()
	input.Placeholder = placeholder
	input.CharLimit = limit
	input.Prompt = "  "
	input.TextStyle = styleText
	input.PlaceholderStyle = styleFaint
	input.Cursor.Style = styleAccent

	return input
}

func newCreateForm(vaults []keys.Vault, selected keys.Vault) createForm {
	title := newFormInput("laptop", 128)
	title.Focus()

	comment := newFormInput("optional, e.g. user@host", 128)

	passphrase := newFormInput("optional", 256)
	// Masked so the passphrase is never rendered to the terminal.
	passphrase.EchoMode = textinput.EchoPassword
	passphrase.EchoCharacter = '•'

	form := createForm{
		title:      title,
		comment:    comment,
		passphrase: passphrase,
		vaults:     vaults,
	}

	// Default to the vault the cursor was on, so creating a key next to an
	// existing one takes no extra keystrokes.
	for i, vault := range vaults {
		if vault.ShareID == selected.ShareID {
			form.vaultIndex = i
			break
		}
	}

	return form
}

// Update handles a key press within the form and reports whether the form was
// submitted.
func (f createForm) Update(msg tea.KeyMsg) (createForm, tea.Cmd, bool) {
	switch msg.String() {
	case "tab", "ctrl+n":
		f.focus = (f.focus + 1) % fieldCount
		return f.applyFocus(), nil, false

	case "shift+tab", "ctrl+p":
		f.focus = (f.focus - 1 + fieldCount) % fieldCount
		return f.applyFocus(), nil, false

	case "down":
		if f.focus != fieldVault && f.focus != fieldKeyType {
			f.focus = (f.focus + 1) % fieldCount
			return f.applyFocus(), nil, false
		}

	case "up":
		if f.focus != fieldVault && f.focus != fieldKeyType {
			f.focus = (f.focus - 1 + fieldCount) % fieldCount
			return f.applyFocus(), nil, false
		}

	case "left", "h":
		// h only cycles on the select fields; elsewhere it is literal text.
		if f.focus == fieldVault && len(f.vaults) > 0 {
			f.vaultIndex = (f.vaultIndex - 1 + len(f.vaults)) % len(f.vaults)
			return f, nil, false
		}
		if f.focus == fieldKeyType {
			f.keyTypeIndex = (f.keyTypeIndex - 1 + len(passcli.KeyTypes)) % len(passcli.KeyTypes)
			return f, nil, false
		}

	case "right", "l":
		if f.focus == fieldVault && len(f.vaults) > 0 {
			f.vaultIndex = (f.vaultIndex + 1) % len(f.vaults)
			return f, nil, false
		}
		if f.focus == fieldKeyType {
			f.keyTypeIndex = (f.keyTypeIndex + 1) % len(passcli.KeyTypes)
			return f, nil, false
		}

	case "enter":
		if err := f.validate(); err != "" {
			f.err = err
			return f, nil, false
		}
		return f, nil, true
	}

	// Anything else is text, and only the focused text input should see it.
	var cmd tea.Cmd

	switch f.focus {
	case fieldTitle:
		f.title, cmd = f.title.Update(msg)
	case fieldComment:
		f.comment, cmd = f.comment.Update(msg)
	case fieldPassphrase:
		f.passphrase, cmd = f.passphrase.Update(msg)
	}

	f.err = ""

	return f, cmd, false
}

// applyFocus points the cursor at the focused text input and blurs the rest.
func (f createForm) applyFocus() createForm {
	f.title.Blur()
	f.comment.Blur()
	f.passphrase.Blur()

	switch f.focus {
	case fieldTitle:
		f.title.Focus()
	case fieldComment:
		f.comment.Focus()
	case fieldPassphrase:
		f.passphrase.Focus()
	}

	return f
}

func (f createForm) validate() string {
	if strings.TrimSpace(f.title.Value()) == "" {
		return "A title is required."
	}
	if len(f.vaults) == 0 {
		return "No vault available."
	}

	return ""
}

// request builds the pass-cli call from the form's current values.
//
// The returned Passphrase is a []byte so the client can wipe it once it has
// been placed in the child environment. The caller is expected to discard the
// form immediately afterwards so that protui keeps no copy of its own.
func (f createForm) request() passcli.GenerateRequest {
	request := passcli.GenerateRequest{
		Title:   strings.TrimSpace(f.title.Value()),
		ShareID: f.vaults[f.vaultIndex].ShareID,
		Comment: strings.TrimSpace(f.comment.Value()),
		KeyType: passcli.KeyTypes[f.keyTypeIndex],
	}

	if passphrase := f.passphrase.Value(); passphrase != "" {
		request.Passphrase = []byte(passphrase)
	}

	return request
}

// formFieldWidth is the width of the input column, so every field's underline
// lines up regardless of how much has been typed into it.
const formFieldWidth = 34

// View renders the form.
func (f createForm) View() string {
	var b strings.Builder

	b.WriteString(f.row(fieldTitle, "Title", f.title.View()))
	b.WriteString(f.row(fieldVault, "Vault", f.selector(fieldVault, f.vaultName())))
	b.WriteString(f.row(fieldKeyType, "Type", f.selector(fieldKeyType, string(passcli.KeyTypes[f.keyTypeIndex]))))
	b.WriteString(f.row(fieldComment, "Comment", f.comment.View()))
	b.WriteString(f.row(fieldPassphrase, "Passphrase", f.passphrase.View()))

	b.WriteString("\n")
	b.WriteString(styleFaint.Render("The comment is stored inside the public key itself."))
	b.WriteString("\n")
	b.WriteString(styleFaint.Render("A passphrase goes via the environment, never the command line."))

	if f.err != "" {
		b.WriteString("\n\n")
		b.WriteString(styleError.Render(glyphWarn + "  " + f.err))
	}

	return dialog(
		styleDialog,
		styleText.Bold(true).Render("New SSH key"),
		strings.TrimRight(b.String(), "\n"),
		"tab  next    ←/→  change    enter  create    esc  cancel",
	)
}

// row is one labelled field. The focused row is marked with an accent bar in
// the gutter and an accent underline, so focus is visible from the shape of the
// row rather than from the cursor alone.
func (f createForm) row(field int, label, input string) string {
	focused := f.focus == field

	gutter := "  "
	labelStyle := styleLabel
	underline := styleRule

	if focused {
		gutter = styleAccent.Render(glyphSelected) + " "
		labelStyle = styleAccent.Bold(true)
		underline = lipgloss.NewStyle().Foreground(colorAccentDim)
	}

	head := lipgloss.JoinHorizontal(
		lipgloss.Top,
		gutter,
		labelStyle.Width(12).Render(label),
		lipgloss.NewStyle().Width(formFieldWidth).Render(input),
	)

	return head + "\n" +
		strings.Repeat(" ", 14) +
		underline.Render(strings.Repeat(glyphRule, formFieldWidth)) + "\n"
}

// selector renders a cycling choice. The arrows appear only on the focused
// field, since they are an affordance for the keys that are live right now.
func (f createForm) selector(field int, value string) string {
	if f.focus == field {
		return styleAccent.Render("‹ ") + styleText.Render(value) + styleAccent.Render(" ›")
	}

	return "  " + styleMuted.Render(value)
}

func (f createForm) vaultName() string {
	if len(f.vaults) == 0 {
		return "(none)"
	}

	return f.vaults[f.vaultIndex].Name
}
