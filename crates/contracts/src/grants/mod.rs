//! Shared grant scope rules. These pure checks do not establish OS confinement.
//! The hub and broker must additionally verify live identity, lease and platform authority.
use crate::{BridgeError, BridgeResult};
use serde::{Deserialize, Serialize};
mod action;
mod control;
mod files;
pub use control::{GrantControl, ProcessControl};
pub use files::*;
mod authority;
mod item;
mod remote;
pub use action::GrantAction;
pub use authority::{
    GRANT_AUTHORITY_MAX_TTL_MS, GrantActor, GrantAuthority, GrantBinding, GrantInterruption,
    GrantOrigin, REMOTE_ACCESS_VERSION,
};
pub use remote::{
    GrantObservation, RemoteReply, RemoteRequest, RemoteWireRequest, RemoteWireResponse,
};
#[cfg(test)]
mod authority_tests;
mod matching;
mod narrow;
mod risk;
mod selector;
#[cfg(test)]
mod tests;
pub use item::GrantItem;
pub use matching::GrantCall;
pub use risk::{GrantRiskCondition, GrantRiskContext};
pub use selector::{AllowedNames, AnyName, GrantSelector};

fn invalid(message: &str) -> BridgeError {
    BridgeError::new("invalid_grant", message)
}
fn denied(message: &str) -> BridgeError {
    BridgeError::new("outside_grant", message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum GrantLevel {
    #[serde(rename = "read-only")]
    ReadOnly,
    #[serde(rename = "workspace-write")]
    WorkspaceWrite,
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "unsandboxed")]
    Unsandboxed,
}
impl GrantLevel {
    pub fn within(self, requested: Self) -> bool {
        self == requested
            || requested == Self::Unsandboxed
            || self == Self::ReadOnly && requested == Self::WorkspaceWrite
    }
}

/// Bounded, validated duration; task lifetime and wall time are not interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub enum GrantDuration {
    Once,
    Task,
    Hours(u32),
    UntilRevoked,
}
impl TryFrom<String> for GrantDuration {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "once" => Ok(Self::Once),
            "task" => Ok(Self::Task),
            "until_revoked" => Ok(Self::UntilRevoked),
            value => value
                .strip_suffix('h')
                .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|n| *n > 0)
                .map(Self::Hours)
                .ok_or_else(|| {
                    "duration must be once, task, a positive number of hours, or until_revoked"
                        .into()
                }),
        }
    }
}
impl From<GrantDuration> for String {
    fn from(value: GrantDuration) -> Self {
        match value {
            GrantDuration::Once => "once".into(),
            GrantDuration::Task => "task".into(),
            GrantDuration::UntilRevoked => "until_revoked".into(),
            GrantDuration::Hours(n) => format!("{n}h"),
        }
    }
}
impl GrantDuration {
    pub fn validate(self) -> BridgeResult<()> {
        if self == Self::Hours(0) {
            Err(invalid("hour duration must be positive"))
        } else {
            Ok(())
        }
    }
    pub fn within(self, requested: Self) -> bool {
        self.validate().is_ok()
            && requested.validate().is_ok()
            && (self == requested
                || self == Self::Once
                || requested == Self::UntilRevoked
                || matches!((self,requested),(Self::Hours(a),Self::Hours(b)) if a<=b))
    }
    pub fn milliseconds(self) -> Option<u64> {
        if let Self::Hours(n) = self {
            Some(u64::from(n) * 3_600_000)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum GrantCapability {
    #[serde(rename = "exec")]
    Exec,
    #[serde(rename = "proc.interactive")]
    Process,
    #[serde(rename = "fs.read")]
    FileRead,
    #[serde(rename = "fs.write")]
    FileWrite,
    #[serde(rename = "net.egress")]
    Network,
    #[serde(rename = "gui.observe")]
    Observe,
    #[serde(rename = "gui.input")]
    Input,
    #[serde(rename = "gui.window")]
    Window,
    #[serde(rename = "gui.app")]
    App,
}
impl GrantCapability {
    pub fn supports(self, level: GrantLevel) -> bool {
        match self {
            Self::Observe | Self::Input | Self::Window | Self::App => {
                matches!(level, GrantLevel::Desktop | GrantLevel::Unsandboxed)
            }
            Self::FileWrite => {
                matches!(level, GrantLevel::WorkspaceWrite | GrantLevel::Unsandboxed)
            }
            _ => level != GrantLevel::Desktop,
        }
    }
    pub fn covers(self, needed: Self) -> bool {
        self == needed || self == Self::FileWrite && needed == Self::FileRead
    }
}
