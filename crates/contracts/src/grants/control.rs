//! Action-bound permission for an existing process; never a start or clock refresh.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessControl {
    Input,
    Resize,
    Cancel,
}
impl ProcessControl {
    pub fn policy(self) -> &'static str {
        match self {
            Self::Input | Self::Resize => "proc.write",
            Self::Cancel => "proc.kill",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantControl {
    pub binding: GrantBinding,
    pub action: ProcessControl,
    pub issued_at: i64,
    pub expires_at: i64,
}
impl GrantControl {
    pub fn remaining_ms(&self, now: i64) -> BridgeResult<u64> {
        GrantAuthority {
            binding: self.binding.clone(),
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
        .remaining_ms(now)
    }
}
