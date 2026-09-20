// Package passcli is the only package in protui that executes the pass-cli
// binary. Everything else depends on these typed wrappers rather than on
// os/exec, so the upstream command surface stays behind one seam.
//
// Security invariants, enforced here and nowhere else:
//
//   - No secret is ever an argv element, because argv is world-readable via ps.
//     The one secret protui can send — an SSH key passphrase on create — goes
//     through the child environment.
//   - `item view --output json` is never used: it returns the private key
//     unredacted. Public keys are read with --field public_key, which prints a
//     single field as bare text.
//   - --show-secrets is never passed.
//   - No private key material is read, stored, or logged by any function here.
//
// See docs/schema.md for the upstream output contract these wrappers assume.
package passcli

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/nxplain-sh/protui/internal/keys"
)

// Binary is the upstream executable protui drives.
const Binary = "pass-cli"

// Sentinel errors, for callers that need to distinguish setup failures from
// ordinary command failures.
var (
	// ErrNotInstalled means the pass-cli binary was not found on PATH.
	ErrNotInstalled = errors.New("pass-cli is not installed or not on PATH")

	// ErrNoSession means pass-cli is installed but has no usable session.
	ErrNoSession = errors.New("no valid pass-cli session")

	// ErrUnexpectedSchema means pass-cli produced output that does not match
	// the documented shape. Almost always an upstream change; see docs/schema.md.
	ErrUnexpectedSchema = errors.New("unexpected pass-cli output")
)

// CommandError reports a failed pass-cli invocation. It always names the
// subcommand so the UI can say which call failed rather than showing an empty
// list or a bare exit status.
type CommandError struct {
	// Command is the subcommand, e.g. "item list".
	Command string
	// ExitCode is the process exit status, or -1 if it never ran.
	ExitCode int
	// Stderr is upstream's message, trimmed. Unstructured prose: display it,
	// never match on it.
	Stderr string
	// Err is the underlying exec or decode error.
	Err error
}

func (e *CommandError) Error() string {
	if e.Stderr != "" {
		return fmt.Sprintf("%s %s: %s", Binary, e.Command, e.Stderr)
	}
	return fmt.Sprintf("%s %s: %v", Binary, e.Command, e.Err)
}

func (e *CommandError) Unwrap() error { return e.Err }

// runner executes a pass-cli invocation. It exists so tests can drive the
// wrappers without a real binary.
type runner func(ctx context.Context, extraEnv []string, args ...string) (stdout, stderr []byte, err error)

// Client runs pass-cli subcommands.
//
// The zero value is not usable; construct one with New.
type Client struct {
	binary string
	run    runner
}

// New returns a Client bound to the pass-cli binary on PATH.
//
// It does not verify that a session exists — call Preflight for that.
func New() (*Client, error) {
	path, err := exec.LookPath(Binary)
	if err != nil {
		return nil, fmt.Errorf("%w: %w", ErrNotInstalled, err)
	}

	client := &Client{binary: path}
	client.run = client.exec

	return client, nil
}

// exec runs the binary and returns stdout and stderr separately.
//
// stdout is not pure JSON in every case even with --output json: upstream also
// writes advisory lines to stderr, so the two streams are kept apart and only
// stdout is ever decoded.
func (c *Client) exec(ctx context.Context, extraEnv []string, args ...string) ([]byte, []byte, error) {
	// gosec flags every exec with non-constant arguments. Running pass-cli
	// with arguments is what this package exists for, and it is safe here for
	// reasons that hold by construction: the binary is resolved by LookPath
	// rather than supplied, no shell is involved so there is nothing to quote
	// or inject, and every argument is a separate argv element built by
	// protui. User-supplied values reach it only as values, never as flags.
	//nolint:gosec // G204: see above.
	cmd := exec.CommandContext(ctx, c.binary, args...)

	// Suppress the auto-update probe on every invocation: it adds latency and
	// can emit unexpected lines.
	cmd.Env = append(os.Environ(), "PROTON_PASS_NO_UPDATE_CHECK=1")
	cmd.Env = append(cmd.Env, extraEnv...)

	// The child must never inherit the terminal. If it tried to prompt (which
	// protui's argument choices avoid) it would otherwise fight Bubble Tea for
	// the TTY; with no stdin it fails fast instead of hanging.
	cmd.Stdin = nil

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()

	return stdout.Bytes(), stderr.Bytes(), err
}

// call runs a subcommand and wraps any failure with the subcommand name.
//
// name is used only for error messages and must not contain user data.
func (c *Client) call(ctx context.Context, name string, extraEnv []string, args ...string) ([]byte, error) {
	stdout, stderr, err := c.run(ctx, extraEnv, args...)
	if err == nil {
		return stdout, nil
	}

	commandErr := &CommandError{
		Command:  name,
		ExitCode: -1,
		Stderr:   cleanStderr(stderr),
		Err:      err,
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		commandErr.ExitCode = exitErr.ExitCode()
	}

	return nil, commandErr
}

// cleanStderr trims upstream's stderr for display and drops the advisory line
// emitted when a passphrase is read from the environment, which is informational
// rather than a failure. Never used for control flow.
func cleanStderr(stderr []byte) string {
	lines := strings.Split(strings.TrimSpace(string(stderr)), "\n")
	kept := make([]string, 0, len(lines))

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "Reading password from") {
			continue
		}
		// Upstream echoes item titles into some errors, so this is display
		// input from outside protui like any other.
		kept = append(kept, keys.Sanitize(strings.TrimPrefix(line, "Error: ")))
	}

	return strings.Join(kept, "; ")
}

// Preflight verifies that pass-cli has a usable session.
//
// `info` exits 0 with a live session and 1 otherwise; its human-readable output
// is deliberately not parsed, since only the exit status is contractual.
func (c *Client) Preflight(ctx context.Context) error {
	if _, err := c.call(ctx, "info", nil, "info"); err != nil {
		return fmt.Errorf("%w: run `%s login` to authenticate", ErrNoSession, Binary)
	}

	return nil
}
