//! Closed table/field vocabulary; no payload can supply a SQL identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Capabilities,
    Risks,
    CapabilityRisks,
    ShellProfiles,
    GrantPresets,
    Runtimes,
    Routers,
    ChatApps,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldType {
    Text,
    LongText,
    Json,
    List,
    Bool,
    Nullable,
}
pub(crate) struct Field {
    pub name: &'static str,
    pub kind: FieldType,
}
macro_rules! fields {($($name:literal:$kind:ident),* $(,)?)=>{&[$(Field {name:$name,kind:FieldType::$kind}),*]};}
impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Capabilities,
        Self::Risks,
        Self::CapabilityRisks,
        Self::ShellProfiles,
        Self::GrantPresets,
        Self::Runtimes,
        Self::Routers,
        Self::ChatApps,
    ];
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Risks => "risks",
            Self::CapabilityRisks => "capability-risks",
            Self::ShellProfiles => "shell-profiles",
            Self::GrantPresets => "grant-presets",
            Self::Runtimes => "docker-runtime-presets",
            Self::Routers => "router-endpoints",
            Self::ChatApps => "chat-apps",
        }
    }
    pub fn table(self) -> &'static str {
        match self {
            Self::Capabilities => "catalog_capabilities",
            Self::Risks => "catalog_risks",
            Self::CapabilityRisks => "catalog_capability_risks",
            Self::ShellProfiles => "catalog_shell_profiles",
            Self::GrantPresets => "catalog_grant_presets",
            Self::Runtimes => "catalog_docker_runtime_presets",
            Self::Routers => "catalog_router_endpoints",
            Self::ChatApps => "catalog_chat_apps",
        }
    }
    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Capabilities => {
                fields!["label":Text,"description":LongText,"selector_schema":Json,"sandbox_levels":List,"default_duration":Text]
            }
            Self::Risks => {
                fields!["title":Text,"plain_text":LongText,"severity":Text,"mitigation":LongText,"source_url":Nullable]
            }
            Self::CapabilityRisks => {
                fields!["capability_id":Text,"risk_id":Text,"when_selector":Json]
            }
            Self::ShellProfiles => {
                fields!["os":Text,"label":Text,"argv_prefix":Json,"env_allow":List,"cwd_default":Text,"pty":Bool]
            }
            Self::GrantPresets => {
                fields!["label":Text,"description":LongText,"items":Json,"duration":Text,"full_access":Bool,"requires_policy":Nullable]
            }
            Self::Runtimes => {
                fields!["os":Text,"label":Text,"what":LongText,"facts":Json,"operator_commands":Json,"agent_argv":Json,"verify_argv":Json,"hosts":List,"risk_ids":List,"licence_confirm":Bool]
            }
            Self::Routers => fields!["label":Text,"url":Text,"kind":Text],
            Self::ChatApps => fields!["label":Text,"provider_ids":List,"connect_template":LongText],
        }
    }
    /// Source policy families, under the current human administration namespace.
    pub fn policy_prefix(self) -> &'static str {
        match self {
            Self::Capabilities => "admin.catalog.capabilities",
            Self::Risks | Self::CapabilityRisks => "admin.catalog.risks",
            Self::ShellProfiles => "admin.catalog.shell_profiles",
            Self::GrantPresets => "admin.preset",
            Self::Runtimes => "admin.catalog.runtimes",
            Self::Routers => "admin.catalog.router_endpoints",
            Self::ChatApps => "admin.catalog.chat_apps",
        }
    }
}
