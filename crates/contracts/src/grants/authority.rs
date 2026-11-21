//! Broker authority is supplied by an authenticated transport, never a tool argument.
use super::*;

pub const GRANT_AUTHORITY_MAX_TTL_MS: i64 = 30_000;
pub const REMOTE_ACCESS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantOrigin {
    /// SHA-256 of the authenticated hub identity; not a URL or credential.
    pub hub: String,
    pub machine: String,
    pub generation: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantActor {
    pub account: String,
    pub principal: String,
    pub client: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantBinding {
    pub origin: GrantOrigin,
    pub account: String,
    pub principal: String,
    pub admission_id: String,
    pub grant_id: String,
    pub grant_version: i64,
    pub operation_key: String,
    pub input_sha256: String,
    pub effective: GrantItem,
    /// Hash of immutable original task/guide identity. Lease expiry is in authority expiry.
    pub lease_sha256: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantAuthority {
    pub binding: GrantBinding,
    pub issued_at: i64,
    pub expires_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantInterruption {
    pub origin: GrantOrigin,
    pub grant_id: String,
    /// Reject older versions. A delayed pause notice must not kill resumed version N.
    pub minimum_version: i64,
}
pub(super) fn token(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
pub(super) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn principal(account: &str, value: &str) -> bool {
    value == account
        || value
            .strip_prefix(&format!("@agent:{account}:"))
            .is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 63
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
}
impl GrantOrigin {
    pub fn validate(&self) -> BridgeResult<()> {
        if !digest(&self.hub) || !token(&self.machine, 128) || !token(&self.generation, 128) {
            return Err(invalid("invalid authenticated machine origin"));
        }
        Ok(())
    }
}
impl GrantActor {
    pub fn validate(&self) -> BridgeResult<()> {
        if !token(&self.account, 256)
            || !token(&self.principal, 512)
            || !token(&self.client, 256)
            || !principal(&self.account, &self.principal)
        {
            return Err(invalid("invalid authenticated machine principal"));
        }
        Ok(())
    }
}
impl GrantBinding {
    pub fn validate(&self) -> BridgeResult<()> {
        self.origin.validate()?;
        self.effective.validate()?;
        if !token(&self.account, 256)
            || !principal(&self.account, &self.principal)
            || !token(&self.principal, 512)
            || !token(&self.admission_id, 128)
            || !token(&self.grant_id, 128)
            || self.grant_version < 1
            || !token(&self.operation_key, 128)
            || !digest(&self.input_sha256)
            || self.lease_sha256.as_deref().is_some_and(|v| !digest(v))
        {
            return Err(invalid("invalid broker admission binding"));
        }
        Ok(())
    }
}
impl GrantAuthority {
    /// Convert this remaining lifetime to a monotonic deadline at the broker boundary.
    /// A future issue time is refused, including when the broker's clock is behind.
    pub fn remaining_ms(&self, now: i64) -> BridgeResult<u64> {
        self.binding.validate()?;
        let lifetime = self
            .expires_at
            .checked_sub(self.issued_at)
            .filter(|v| (1..=GRANT_AUTHORITY_MAX_TTL_MS).contains(v))
            .ok_or_else(|| invalid("authority lifetime exceeds its bound"))?;
        if self.issued_at < 0 || now < self.issued_at || now >= self.expires_at {
            return Err(BridgeError::new(
                "authority_expired",
                "broker authority is expired or not yet valid",
            ));
        }
        Ok((self.expires_at - now).min(lifetime) as u64)
    }
}
impl GrantInterruption {
    pub fn validate(&self) -> BridgeResult<()> {
        self.origin.validate()?;
        if !token(&self.grant_id, 128) || self.minimum_version < 1 {
            return Err(invalid("invalid grant interruption"));
        }
        Ok(())
    }
}
