//! One requested or approved capability. Approval may only reduce this scope.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantItem {
    pub capability: GrantCapability,
    pub sandbox_level: GrantLevel,
    #[serde(default)]
    pub selector: GrantSelector,
}
impl GrantItem {
    pub fn validate(&self) -> BridgeResult<()> {
        self.selector.validate()?;
        if !self.capability.supports(self.sandbox_level) {
            return Err(invalid("sandbox level does not support this capability"));
        }
        Ok(())
    }
    pub fn within(&self, requested: &Self) -> BridgeResult<()> {
        self.validate()?;
        requested.validate()?;
        // A decision narrows one requested capability; it cannot substitute another.
        if self.capability != requested.capability
            || !self.sandbox_level.within(requested.sandbox_level)
        {
            return Err(denied("capability changed or sandbox level widened"));
        }
        self.selector.within(&requested.selector)
    }
    pub fn needs_unsandboxed_policy(&self) -> bool {
        self.sandbox_level == GrantLevel::Unsandboxed
            || self.capability == GrantCapability::Input
                && self
                    .selector
                    .apps
                    .as_ref()
                    .is_none_or(|apps| matches!(apps, AllowedNames::Any(_)))
    }
}
