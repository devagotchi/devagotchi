//! Approval can reduce one requested item, duration and number of admissions.
use super::*;
use devagotchi_contracts::{GrantDuration, GrantItem};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantTerms {
    pub item: GrantItem,
    pub duration: GrantDuration,
    pub uses_max: Option<u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Narrow {
    pub item: Option<GrantItem>,
    pub duration: Option<GrantDuration>,
    pub uses_max: Option<u32>,
    #[serde(default)]
    pub note: String,
}
impl GrantTerms {
    pub(crate) fn validate(&self) -> BridgeResult<()> {
        self.item.validate()?;
        self.duration.validate()?;
        if self.uses_max == Some(0)
            || self.duration == GrantDuration::Once && self.uses_max != Some(1)
        {
            return Err(invalid(
                "use limits must be positive; once permits one admission",
            ));
        }
        Ok(())
    }
    pub(crate) fn narrowed(&self, decision: &Narrow) -> BridgeResult<Self> {
        self.validate()?;
        if decision.uses_max == Some(0) {
            return Err(invalid("use limits must be positive"));
        }
        if decision.note.len() > 8192 || decision.note.contains('\0') {
            return Err(invalid("decision note is too long or contains a NUL"));
        }
        let mut chosen = Self {
            item: decision.item.clone().unwrap_or_else(|| self.item.clone()),
            duration: decision.duration.unwrap_or(self.duration),
            uses_max: decision.uses_max.or(self.uses_max),
        };
        chosen
            .item
            .within(&self.item)
            .map_err(|e| BridgeError::new("scope_wider_than_request", e.message))?;
        if !chosen.duration.within(self.duration)
            || self
                .uses_max
                .is_some_and(|limit| chosen.uses_max.is_none_or(|n| n > limit))
        {
            return Err(BridgeError::new(
                "scope_wider_than_request",
                "duration or use limit widened",
            ));
        }
        if chosen.duration == GrantDuration::Once {
            chosen.uses_max = Some(1);
        }
        chosen.validate()?;
        Ok(chosen)
    }
    pub(crate) fn expires_at(&self, now: i64) -> BridgeResult<Option<i64>> {
        self.validate()?;
        self.duration
            .milliseconds()
            .map(|ms| {
                now.checked_add(ms as i64)
                    .ok_or_else(|| invalid("grant duration exceeds the database clock"))
            })
            .transpose()
    }
}
