//! The SSH agent daemon.

use crate::passcli::client::{Client, CommandError};
use crate::passcli::parse;

/// The daemon's lifecycle state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentState {
    Running,
    Stopped,
    /// The process is alive but its socket is missing.
    Degraded,
    /// The status output could not be classified.
    #[default]
    Unknown,
}

/// The parsed result of `ssh-agent daemon status`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentStatus {
    pub state: AgentState,
    /// The full `Status:` value, including the parenthetical explanation
    /// upstream adds for the degraded and stale-socket cases.
    pub detail: String,
    /// Empty when no PID file exists, as is `socket`.
    pub pid: String,
    pub socket: String,
}

impl Client {
    /// Reports the SSH agent daemon's state.
    ///
    /// The command has no JSON output and must be line-parsed, and it exits 0
    /// whether or not the daemon is running — so the state comes from stdout
    /// and never from the exit code. See `docs/schema.md` §7.
    pub async fn agent_status(&self) -> Result<AgentStatus, CommandError> {
        let stdout = self
            .call(
                "ssh-agent daemon status",
                &["ssh-agent", "daemon", "status"],
                &[],
            )
            .await?;

        Ok(parse::agent_status(&stdout))
    }

    /// Starts the SSH agent daemon in the background.
    pub async fn start_agent(&self) -> Result<(), CommandError> {
        self.call(
            "ssh-agent daemon start",
            &["ssh-agent", "daemon", "start"],
            &[],
        )
        .await?;

        Ok(())
    }

    /// Stops the SSH agent daemon.
    pub async fn stop_agent(&self) -> Result<(), CommandError> {
        self.call(
            "ssh-agent daemon stop",
            &["ssh-agent", "daemon", "stop"],
            &[],
        )
        .await?;

        Ok(())
    }
}
