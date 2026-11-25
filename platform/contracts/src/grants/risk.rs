//! Source risk predicates, using the same empty/absent semantics as authorization.
use super::*;
use selector::present;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRiskCondition {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub hosts_wildcard: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub argv0: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub sandbox_level: Option<GrantLevel>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<GrantDuration>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_server: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub apps_any: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub path_prefix: Option<Vec<String>>,
}
pub struct GrantRiskContext<'a> {
    pub selector: &'a GrantSelector,
    pub level: GrantLevel,
    pub duration: GrantDuration,
    pub display_server: &'a str,
    pub machine_kind: &'a str,
}
impl GrantRiskCondition {
    pub fn validate(&self) -> BridgeResult<()> {
        if let Some(argv) = &self.argv0 {
            selector::strings(argv)?;
        }
        if let Some(paths) = &self.path_prefix {
            selector::strings(paths)?;
            if paths.iter().any(|p| selector::lexical(p).is_none()) {
                return Err(invalid(
                    "risk path prefixes need an absolute or home anchor",
                ));
            }
        }
        if let Some(display) = &self.display_server {
            selector::strings(std::slice::from_ref(display))?;
        }
        if let Some(duration) = self.duration {
            duration.validate()?;
        }
        Ok(())
    }
    pub fn matches(&self, context: &GrantRiskContext<'_>) -> BridgeResult<bool> {
        self.validate()?;
        context.selector.validate()?;
        context.duration.validate()?;
        let s = context.selector;
        let wildcard = s
            .hosts
            .as_ref()
            .is_some_and(|hosts| hosts.iter().any(|h| h == "*" || h.starts_with("*.")));
        let broad_apps = s
            .apps
            .as_ref()
            .is_none_or(|a| matches!(a, AllowedNames::Any(_)));
        Ok(self
            .hosts_wildcard
            .is_none_or(|expected| expected == wildcard)
            && self.apps_any.is_none_or(|expected| expected == broad_apps)
            && self
                .sandbox_level
                .is_none_or(|level| level == context.level)
            && self
                .duration
                .is_none_or(|duration| duration == context.duration)
            && self
                .display_server
                .as_ref()
                // Unknown display facts cannot silently remove a display-specific warning.
                .is_none_or(|display| {
                    context.display_server.is_empty() || display == context.display_server
                })
            && self.argv0.as_ref().is_none_or(|names| {
                !names.is_empty()
                    && s.argv_prefix.as_ref().is_none_or(|prefixes| {
                        prefixes
                            .iter()
                            .any(|p| p.first().is_some_and(|p| names.contains(p)))
                    })
            })
            && self.path_prefix.as_ref().is_none_or(|prefixes| {
                !prefixes.is_empty()
                    && match &s.paths {
                        None => true,
                        Some(paths) => paths.iter().any(|path| {
                            prefixes.iter().any(|prefix| {
                                selector::path_within(path, prefix)
                                    || selector::path_within(prefix, path)
                            })
                        }),
                    }
            }))
    }
}
