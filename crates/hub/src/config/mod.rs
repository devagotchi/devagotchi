//! Hub configuration: who may reach which broker, and how callers are authenticated.
//!
//! Unknown fields and unsafe values fail closed, like the broker's own config.
//!
//! This file is [`HubConfig`] itself, its parse (the file, then the environment over it)
//! and the checks that span sections. Each section keeps its type, its checks and what it
//! answers in a file of its own: `auth.rs` (`auth`, `trusted_issuers`, `web`),
//! `access.rs` (`brokers`, `boards`, `grants`, `account_boards`), `storage.rs` (`storage`,
//! `clickhouse`), `signals.rs`, `site.rs`, `tenancy.rs`, `analytics.rs`, `providers.rs`,
//! `github.rs`, `machines.rs`, `shippin.rs` and `workflows.rs`. `file.rs` reads the file
//! and prepares the state directory; `urls.rs` holds the URL checks they share.

use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;

mod access;
pub(crate) mod analytics;
mod auth;
mod file;
pub(crate) mod github;
pub(crate) mod live;
pub(crate) mod machines;
pub(crate) mod providers;
pub(crate) mod shippin;
mod signals;
mod site;
mod storage;
pub(crate) mod tenancy;
mod urls;
pub(crate) mod workflows;
pub use access::{AccountBoardsConfig, BoardEntry, BrokerEntry, DEFAULT_FREE_LIMIT, Grant};
pub use analytics::AnalyticsConfig;
pub(crate) use auth::valid_subject;
pub use auth::{AuthConfig, TokenConfig, TrustedIssuer, WebConfig};
pub(crate) use file::{prepare_state_dir, read};
pub use github::GithubConfig;
pub use live::{Live, LiveSettings, Pinned};
pub use machines::MachinesConfig;
pub use providers::{EmailConfig, ProviderConfig, ProviderKind};
pub use shippin::ShippinConfig;
pub use signals::{SignalsConfig, SinkConfig, SinkFields};
pub use site::SiteConfig;
pub use storage::{CLICKHOUSE_PASSWORD_ENV, ClickHouseConfig, StorageConfig};
use tenancy::check_operator;
pub use tenancy::{OPERATORS_ENV, TENANCY_ENV, TenancyConfig, TenancyMode, is_operator};
pub(crate) use urls::url_host;
use urls::{check_rp_id, loopback_http};
pub use workflows::WorkflowsConfig;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HubConfig {
    /// Socket address the hub listens on. Put it behind the edge (Caddy); the hub
    /// speaks plain HTTP and trusts nothing from the network but the bearer token.
    pub listen: String,
    /// The hub's own HTTPS URL as clients see it. It is the protected resource
    /// identifier (RFC 9728) and the `resource` hosted clients send.
    pub public_url: String,
    pub auth: AuthConfig,
    /// More OpenID Connect issuers whose signed JWTs the hub accepts next to `auth`, such
    /// as the devagotchi.com site's sign-in, so a person signed in there reaches their own
    /// machines with that same token. A token goes to the issuer its `iss` names; any other
    /// token goes to `auth`. Grants and ownership still key on the token's `sub`.
    #[serde(default)]
    pub trusted_issuers: Vec<TrustedIssuer>,
    /// The account ID (`sub`) of the person who owns this hub. The owner reaches every
    /// broker without a grant, and nothing at runtime can take that away: only editing
    /// this file changes it. Required with Nomen sign-in, so a hub can never be set up
    /// without someone who can always get in.
    #[serde(default)]
    pub owner: Option<String>,
    /// Brokers this hub can route to: local ones by their IPC endpoint, and brokers on
    /// other machines reached through the link their `devagotchi link` holds open.
    pub brokers: Vec<BrokerEntry>,
    /// Which authenticated subject may use which brokers and boards. No grant, no route.
    pub grants: Vec<Grant>,
    /// Project boards this hub serves over `/mcp` (`board_*` tools): each is a project
    /// directory on this host that `devagotchi board init` prepared, so the same
    /// store answers the command line here and agents anywhere.
    #[serde(default)]
    pub boards: Vec<BoardEntry>,
    /// GitHub repositories admitted to configured project boards. Disabled when empty.
    #[serde(default)]
    pub github: GithubConfig,
    /// Boards this hub hosts for accounts (D23, packet 15): one per account and project,
    /// made by `board_bind`, kept under `dir`. Absent: only the `boards` above exist here.
    #[serde(default)]
    pub account_boards: Option<AccountBoardsConfig>,
    /// Browser sign-in for people: `/account` shows who you are signed in as and your
    /// machines. Nomen sign-in only. Absent: no pages, only `/mcp`, `/link` and the APIs.
    #[serde(default)]
    pub web: Option<WebConfig>,
    /// A directory of Devagotchi builds this hub hands to machines that join it
    /// (`/dist/<file>`, `/join/<code>`): `SHA256SUMS`, `VERSION` and the files they list,
    /// laid down at deploy from the GitHub build the hub itself runs.
    /// `DEVAGOTCHI_HUB_DOWNLOADS` overrides it. Absent: the builds the hub image carries
    /// (`/usr/share/devagotchi/downloads`, `downloads.rs`); with neither, no `/join`, and
    /// "Add a machine" shows the installer lines instead.
    #[serde(default)]
    pub downloads: Option<String>,
    /// The hub's Postgres (`store/mod.rs`): its URL and the schema its tables live in. A
    /// password never goes here: `DEVAGOTCHI_HUB_STORAGE_URL`, set from the secret store, names the store
    /// instead and overrides this (`DEVAGOTCHI_HUB_STORAGE_SCHEMA` the schema).
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    /// The ClickHouse the hub exports its board events to for statistics (ADR 0001,
    /// `warehouse.rs`). `DEVAGOTCHI_CLICKHOUSE_URL` sets or overrides the URL and
    /// `DEVAGOTCHI_CLICKHOUSE_DATABASE`/`_USER` the rest; the password is never here, only in
    /// `DEVAGOTCHI_CLICKHOUSE_PASSWORD`. Absent: nothing is exported and `/healthz` says so.
    #[serde(default)]
    pub clickhouse: Option<ClickHouseConfig>,
    /// Where the hub pushes its signals, the versioned stream of what happens on it
    /// (`notify/signals/`, `docs/signals.md`). Absent: none leave the hub.
    #[serde(default)]
    pub signals: SignalsConfig,
    /// One owner's hub (`single`, the default) or a hosted service where everyone who
    /// connects gets an account and a tenant of their own (`multi`), with the limits on an
    /// account nobody has secured yet (`config/tenancy.rs`). `DEVAGOTCHI_HUB_TENANCY`
    /// overrides the mode.
    #[serde(default)]
    pub tenancy: TenancyConfig,
    /// Who runs this hub: identities (`email:<verified address>` or
    /// `<provider>:<subject>`) whose sign-in is the hub's owner, the platform operator,
    /// with no claim. Set at deploy, from `DEVAGOTCHI_HUB_OPERATORS` (comma separated) or
    /// here; never in the code.
    #[serde(default)]
    pub operators: Vec<String>,
    /// The site the hub's pages, sign-in email and MCP display read (`site.rs`): `dir`, an
    /// absolute site directory, or `DEVAGOTCHI_SITE_DIR` over it. Absent: the built-in one.
    #[serde(default)]
    pub site: SiteConfig,
    /// Whether a hosted hub counts visits to its public pages and its accounts' milestones
    /// for its operator, and how long it keeps raw visits (`config/analytics.rs`).
    /// `DEVAGOTCHI_HUB_ANALYTICS` and `DEVAGOTCHI_HUB_ANALYTICS_RETENTION_DAYS` override it.
    #[serde(default)]
    pub analytics: AnalyticsConfig,
    /// How long a workflow's link works (`config/workflows.rs`).
    #[serde(default)]
    pub workflows: WorkflowsConfig,
    /// How an account whose person says yes to the site's offers is linked to its account
    /// on the network (`config/shippin.rs`, `src/offers.rs`).
    #[serde(default)]
    pub shippin: ShippinConfig,
    /// Whether machines update themselves to the build this hub hands out
    /// (`config/machines.rs`).
    #[serde(default)]
    pub machines: MachinesConfig,
    /// The owner every holder of this config sees: `owner` above, or in hub mode the
    /// one the accounts store names, learned once at start or when it is made.
    #[serde(skip)]
    owner_cell: Arc<OnceLock<String>>,
    /// The site `site` names, read and checked when the config is parsed, and again by
    /// the running hub whenever its directory changes (`site::watch`).
    #[serde(skip, default = "site::default_site")]
    loaded_site: Arc<crate::site::SiteCell>,
    /// What a hosted edition replaces. Not in the config file: `serve_with` sets it, and
    /// a file leaves the core defaults (`extensions.rs`).
    #[serde(skip, default)]
    pub extensions: crate::extensions::Extensions,
    /// The settings the owner changes on `/admin` (`config/live.rs`), shared by every holder
    /// of this config: the file's values at parse, the saved ones over them once the hub
    /// reads its store (`serve`), a save replacing them at once.
    #[serde(skip)]
    live: Live,
    /// The settings the file or the environment set, which the page shows but leaves.
    #[serde(skip)]
    pinned: Pinned,
}

impl HubConfig {
    pub fn parse(text: &str) -> BridgeResult<Self> {
        Self::parse_with(text, |_| None)
    }

    /// `parse`, with `DEVAGOTCHI_HUB_TENANCY`, `DEVAGOTCHI_HUB_OPERATORS`,
    /// `DEVAGOTCHI_HUB_ANALYTICS`, `DEVAGOTCHI_HUB_ANALYTICS_RETENTION_DAYS`,
    /// `DEVAGOTCHI_HUB_DOWNLOADS` and `DEVAGOTCHI_SITE_DIR` from `env` over the file's
    /// `tenancy.mode`, `operators`, `analytics`, `downloads` and `site.dir`: how a deploy sets
    /// them from `.env` or the secret store. The site directory is read and checked here, so a
    /// bad one stops the start.
    pub fn parse_with(text: &str, env: impl Fn(&str) -> Option<String>) -> BridgeResult<Self> {
        let raw: serde_json::Value = serde_json::from_str(text)
            .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?;
        let mut config: HubConfig = serde_json::from_value(raw.clone())
            .map_err(|error| BridgeError::new("invalid_config", error.to_string()))?;
        let invalid = |message: String| BridgeError::new("invalid_config", message);
        if let Some(mode) = env(TENANCY_ENV).filter(|mode| !mode.trim().is_empty()) {
            config.tenancy.mode = match mode.trim() {
                "single" => TenancyMode::Single,
                "multi" => TenancyMode::Multi,
                other => {
                    return Err(invalid(format!("{TENANCY_ENV}={other:?}: single or multi")));
                }
            };
        }
        if let Some(operators) = env(OPERATORS_ENV).filter(|list| !list.trim().is_empty()) {
            config.operators = operators
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect();
        }
        config.analytics = config.analytics.from_env(&env).map_err(invalid)?;
        config.signals.check().map_err(invalid)?;
        config.clickhouse =
            storage::clickhouse_from_env(config.clickhouse.take(), &env).map_err(invalid)?;
        config.downloads = crate::machines::downloads::choose(
            config.downloads.take(),
            env(crate::machines::downloads::DOWNLOADS_ENV),
            crate::machines::downloads::built_in(),
        );
        config.validate(&env)?;
        if let Some(dir) = env(crate::site::SITE_ENV).filter(|dir| !dir.trim().is_empty()) {
            config.site.dir = Some(dir.trim().to_string());
        }
        if let Some(dir) = &config.site.dir {
            // A site that does not check out stops the start, with what is wrong in it.
            let site = crate::site::Site::load(Path::new(dir))
                .map_err(|why| invalid(format!("site.dir {dir}: {why}")))?;
            config.loaded_site = Arc::new(crate::site::SiteCell::new(Arc::new(site)));
        }
        if let Some(owner) = &config.owner {
            let _ = config.owner_cell.set(owner.clone());
        }
        config.live = Live::new(LiveSettings::from_config(&config));
        config.pinned = Pinned::from(&raw, &env);
        Ok(config)
    }

    fn validate(&self, env: &dyn Fn(&str) -> Option<String>) -> BridgeResult<()> {
        self.github.check(&self.boards)?;
        let invalid = |message: String| BridgeError::new("invalid_config", message);
        if self.listen.parse::<std::net::SocketAddr>().is_err() {
            return Err(invalid(format!(
                "listen must be host:port, got {:?}",
                self.listen
            )));
        }
        if !self.public_url.starts_with("https://") && !self.public_url.starts_with("http://") {
            return Err(invalid("public_url must be an absolute http(s) URL".into()));
        }
        self.tenancy.check().map_err(invalid)?;
        self.analytics.check().map_err(invalid)?;
        self.workflows.check().map_err(invalid)?;
        self.shippin.check().map_err(invalid)?;
        for operator in &self.operators {
            check_operator(operator).map_err(invalid)?;
        }
        if (self.tenancy.multi() || !self.operators.is_empty())
            && !matches!(self.auth, AuthConfig::Hub { .. })
        {
            return Err(invalid(
                "tenancy.mode multi and operators need the hub's own accounts (auth.mode hub)"
                    .into(),
            ));
        }
        if self.tenancy.multi() && self.owner.is_some() {
            return Err(invalid(
                "a multi-tenant hub's owner is its operator, from operators (or devagotchi hub owner on its host); leave owner out".into(),
            ));
        }
        if let Some(storage) = &self.storage {
            storage.check().map_err(invalid)?;
        }
        self.check_auth(env)?;
        if let Some(owner) = &self.owner {
            valid_subject(owner).map_err(|why| invalid(format!("owner {why}")))?;
        }
        if let Some(dir) = &self.downloads
            && !std::path::Path::new(dir).is_absolute()
        {
            return Err(invalid(format!(
                "downloads {dir:?} must be an absolute path"
            )));
        }
        self.check_web()?;
        self.check_access()?;
        Ok(())
    }

    /// The `/admin` settings in force now (`config/live.rs`).
    pub fn live(&self) -> LiveSettings {
        self.live.get()
    }

    /// The handle to them, for what reads them later than now.
    pub fn live_handle(&self) -> Live {
        self.live.clone()
    }

    /// The settings the file or the environment set.
    pub fn pinned(&self) -> &Pinned {
        &self.pinned
    }

    /// Whether this subject is the hub's owner.
    pub fn is_owner(&self, subject: &str) -> bool {
        self.owner_id() == Some(subject)
    }

    /// The hub's owner: the configured one (after a devagotchi rekey, the `dg_` id),
    /// or the one its accounts store names when the file left `owner` empty.
    pub fn owner_id(&self) -> Option<&str> {
        self.owner
            .as_deref()
            .or_else(|| self.owner_cell.get().map(String::as_str))
    }

    /// The owner as every holder of this config sees it, for stores built from it.
    pub(crate) fn owner_handle(&self) -> Arc<OnceLock<String>> {
        self.owner_cell.clone()
    }

    /// Take the owner the accounts store names, for every holder of this config at once.
    /// There is only ever one: a different id than the one already known is refused.
    pub fn learn_owner(&self, id: &str) -> Result<(), String> {
        let known = self.owner_cell.get_or_init(|| id.to_string());
        if known == id {
            Ok(())
        } else {
            Err(format!(
                "the hub's owner is {known}, but its accounts name {id}"
            ))
        }
    }

    /// Whether this hub is a hosted service where everyone who connects gets an account
    /// and a tenant of their own (`tenancy.mode multi`).
    pub fn multi_tenant(&self) -> bool {
        self.tenancy.multi()
    }

    /// Whether this hub counts visits and its accounts' milestones: a hosted hub, its
    /// operator's page the one place they are read, with `analytics.enabled`.
    pub fn counts_visits(&self) -> bool {
        self.multi_tenant() && self.analytics.enabled
    }

    /// Whether a new hub may be claimed from the browser with the code in its log (16c):
    /// one owner's hub whose config names neither its owner nor its operators. A hosted
    /// hub is never claimed; an operator set at deploy has decided who runs it.
    pub fn claim_allowed(&self) -> bool {
        !self.multi_tenant() && self.operators.is_empty() && self.owner.is_none()
    }
}

pub(crate) use devagotchi_agenthub::valid_name;

#[cfg(test)]
mod tests;
