package ui

import (
	"fmt"
	"os"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"

	"github.com/nxplain-sh/protui/internal/keys"
	"github.com/nxplain-sh/protui/internal/passcli"
)

const previewRSA = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQDDMU3w/a9F5JytBm0CPnKCZ9XlyVxG7d3afM0dJg1+4yZp2dWifkADPAxWAxaLnY7gcJzyCDIC3j5klN1deFpspph4AX09detMoaunhNa/xpxFmWRHin36F+6EQWAKtshvBCoenF8PCcFYrnGsatPPxI2Dbyl0QNacQgfiG4C0YLS1/Ajx/JyY8MwoVqM6hIBmE/hVdAzH+EwVe7R7jLcLNAW63j71At/OYyWOCMeOkPN3JPzRHxadMl666XI/ML0tzmv2akTi3bohkQUtXtFS7rFmVhZF78kYACs4gWoyxNCPpHFh3omYLvGL5xk0qhCgs0O99d6vmNZ2BDb1MNgf deploy@ci"

// TestPreview prints every screen with colour forced on, for eyeballing the
// layout. Skipped unless PROTUI_PREVIEW is set, since it asserts nothing.
//
//	PROTUI_PREVIEW=1 go test ./internal/ui -run TestPreview -v
func TestPreview(t *testing.T) {
	if os.Getenv("PROTUI_PREVIEW") == "" {
		t.Skip("set PROTUI_PREVIEW=1 to render the screens")
	}

	lipgloss.SetColorProfile(termenv.TrueColor)
	lipgloss.SetHasDarkBackground(os.Getenv("PROTUI_PREVIEW_LIGHT") == "")

	stamp := time.Date(2026, 1, 23, 19, 48, 23, 0, time.UTC)
	vaults := []keys.Vault{
		{Name: "Personal", ShareID: "s1"},
		{Name: "Work", ShareID: "s2"},
	}

	newKey := func(id, share, vault, title string) keys.Key {
		return keys.Key{
			ID: id, ShareID: share, VaultName: vault, Title: title,
			State: keys.StateActive, Algorithm: keys.AlgorithmUnknown,
			CreatedAt: stamp, ModifiedAt: stamp,
		}
	}

	base := New(nil)
	next, _ := base.Update(tea.WindowSizeMsg{Width: 118, Height: 34})
	next, _ = next.(Model).Update(vaultsLoadedMsg{vaults: vaults})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: vaults[0], keys: []keys.Key{
		newKey("i1", "s1", "Personal", "laptop"),
		newKey("i2", "s1", "Personal", "deploy key"),
	}})
	next, _ = next.(Model).Update(keysLoadedMsg{vault: vaults[1], keys: []keys.Key{
		newKey("i3", "s2", "Work", "bastion jump host"),
	}})
	next, _ = next.(Model).Update(publicKeyLoadedMsg{shareID: "s1", itemID: "i1", publicKey: testPublicKey})
	next, _ = next.(Model).Update(publicKeyLoadedMsg{shareID: "s1", itemID: "i2", publicKey: previewRSA})

	model := next.(Model)
	model.inflight = 0
	model.loading = false

	show := func(name string, view string) {
		fmt.Printf("\n\x1b[7m %s \x1b[0m\n\n%s\n", name, view)
	}

	show("LIST + DETAIL", model.View())

	form := model
	form.mode = modeCreate
	form.form = newCreateForm(vaults, vaults[0])
	form.form.focus = fieldVault
	form.form = form.form.applyFocus()
	show("CREATE", form.View())

	trash := model
	trash.mode = modeConfirmTrash
	trash.target = model.allKeys[0]
	show("TRASH", trash.View())

	del := model
	del.mode = modeConfirmDelete
	del.target = model.allKeys[0]
	del.confirmInput.Focus()
	show("DELETE", del.View())

	agent := model
	agent.mode = modeAgent
	agent.agent = passcli.AgentStatus{
		State: passcli.AgentRunning, Detail: "running",
		PID: "54321", Socket: "/Users/example/.proton-pass/ssh-agent.sock",
	}
	show("AGENT", agent.View())

	empty := New(nil)
	sized, _ := empty.Update(tea.WindowSizeMsg{Width: 118, Height: 20})
	loaded, _ := sized.(Model).Update(vaultsLoadedMsg{vaults: nil})
	show("EMPTY", loaded.(Model).View())

	narrow := model
	narrow = narrow.resize(tea.WindowSizeMsg{Width: 60, Height: 20})
	show("NARROW (60 cols, no detail pane)", narrow.View())
}
