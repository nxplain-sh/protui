// Command protui is an unofficial terminal UI for managing Proton Pass SSH
// keys. It drives the official pass-cli binary and is not affiliated with or
// endorsed by Proton AG.
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/nxplain-sh/protui/internal/passcli"
	"github.com/nxplain-sh/protui/internal/ui"
)

// version is overridden at build time via -ldflags.
var version = "dev"

// preflightTimeout bounds the startup session check, so a stalled network call
// cannot hang the launch indefinitely.
const preflightTimeout = 20 * time.Second

func main() {
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "-v", "--version", "version":
			fmt.Printf("protui %s\n", version)
			return
		case "-h", "--help", "help":
			usage()
			return
		default:
			fmt.Fprintf(os.Stderr, "protui: unknown argument %q\n\n", os.Args[1])
			usage()
			os.Exit(2)
		}
	}

	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, ui.FatalMessage(err))
		os.Exit(1)
	}
}

func run() error {
	// pass-cli must be present and authenticated before the TUI takes over the
	// terminal: a missing session is the most likely first-run failure, and it
	// is far clearer reported as plain text than inside a full-screen UI.
	client, err := passcli.New()
	if err != nil {
		return err
	}

	ctx, cancel := context.WithTimeout(context.Background(), preflightTimeout)
	defer cancel()

	if err := client.Preflight(ctx); err != nil {
		return err
	}

	program := tea.NewProgram(ui.New(client), tea.WithAltScreen())
	if _, err := program.Run(); err != nil {
		return fmt.Errorf("terminal UI failed: %w", err)
	}

	return nil
}

func usage() {
	fmt.Println(`protui — a terminal UI for Proton Pass SSH keys.

Usage:
  protui            launch the UI
  protui --version  print the version
  protui --help     print this message

Requires the official pass-cli binary, authenticated with 'pass-cli login'.
Not affiliated with or endorsed by Proton AG.`)
}
