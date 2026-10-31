# Keep I/O out of the update loop

- **Status:** accepted
- **Date:** 2026-08-31

## Context

Bubble Tea runs a single update loop. `Update` is called for each message and
must return promptly: while it runs, no other message is processed and the UI
does not repaint.

Every operation protui performs is a subprocess spawn against a network-backed
service. `vault list` on a live account takes hundreds of milliseconds; a
listing fans out across vaults; deriving metadata costs one call per key.

Calling any of that directly from `Update` would freeze the interface for the
duration — no repaint, no keypress handling, no way to quit. With a bounded
30-second timeout on each call, a stalled network would lock the terminal for
half a minute.

Bubble Tea's answer is `tea.Cmd`: a function returning a message, which the
runtime executes on its own goroutine and delivers back through `Update`.

## Decision

We will perform no I/O in `Update`. Every `pass-cli` call is dispatched as a
`tea.Cmd` and reported back as a typed message.

`Update` and its handlers only transform model state and return commands. The
commands live in one file, each one a thin wrapper that runs a `passcli`
function under a timeout and packages the result into a message.

Every message type carries its own error field rather than sharing an error
channel, so a failure is handled where its result would have been handled.

## Consequences

The UI stays responsive while work is in flight. Keys can be pressed and the
program can be quit during a slow load.

Errors arrive as data. A vault failure is a field on `keysLoadedMsg`, which is
what makes independent per-vault handling possible — see
[fan-out-item-listing-per-vault](0007-fan-out-item-listing-per-vault.md).

State transitions are explicit. Anything conceptually "in progress" needs a
model field, because the work is happening elsewhere: outstanding vault count,
in-flight fetch count, a pending queue, an agent-busy flag.

Concurrency must be bounded by the model rather than by the call stack. Commands
run on their own goroutines, so issuing one per key at once would spawn a
process per key; the pending queue and in-flight counter exist to drain that
work through a fixed window.

The model becomes testable without a terminal. Feeding synthetic messages into
`Update` and rendering `View` exercises every screen with no subprocess and no
TTY, which is how the render tests work.

Model methods take value receivers and return the updated model, matching how
Bubble Tea threads state through `Update` — and avoiding the mixed
pointer-and-value receivers the Go style guide warns against.

## Amended 2026-10-30

protui is now Rust on ratatui ([rewrite in Rust with ratatui](0014-rewrite-in-rust-with-ratatui.md)). The decision holds; its mechanism
changed. `Model::update` returns `Command` values — plain data rather than
closures — and the runtime runs each on its own tokio task and posts the result
back as a `Message` over a channel. Because commands are data, tests now assert
which effect was asked for, not merely that one was.

The concurrency window for public key fetches is unchanged. The remark about
receivers no longer applies: `update` takes `&mut self` and returns the
commands it wants run.
