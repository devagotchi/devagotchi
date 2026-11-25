//! Exact policy actions selected by operation adapters, never a caller's permission name.
use super::GrantCapability;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantAction {
    Exec,
    FileRead,
    FileWrite,
    ProcessStart,
    ProcessRead,
    ProcessWrite,
    ProcessHeartbeat,
    ProcessKill,
    Network,
    Observe,
    Input,
    Window,
    App,
}
impl GrantAction {
    pub fn policy(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::FileRead => "fs.read",
            Self::FileWrite => "fs.write",
            Self::ProcessStart => "proc.start",
            Self::ProcessRead => "proc.read",
            Self::ProcessWrite => "proc.write",
            Self::ProcessHeartbeat => "proc.heartbeat",
            Self::ProcessKill => "proc.kill",
            Self::Network => "net.egress",
            Self::Observe => "gui.observe",
            Self::Input => "gui.input",
            Self::Window => "gui.window",
            Self::App => "gui.app",
        }
    }
    pub fn capability(self) -> GrantCapability {
        match self {
            Self::Exec => GrantCapability::Exec,
            Self::FileRead => GrantCapability::FileRead,
            Self::FileWrite => GrantCapability::FileWrite,
            Self::ProcessStart
            | Self::ProcessRead
            | Self::ProcessWrite
            | Self::ProcessHeartbeat
            | Self::ProcessKill => GrantCapability::Process,
            Self::Network => GrantCapability::Network,
            Self::Observe => GrantCapability::Observe,
            Self::Input => GrantCapability::Input,
            Self::Window => GrantCapability::Window,
            Self::App => GrantCapability::App,
        }
    }
}
