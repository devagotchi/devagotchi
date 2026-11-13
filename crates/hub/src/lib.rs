//! `devagotchi hub`: the bastion. Authenticates every caller through the identity
//! provider, routes typed operations to the broker that owns the target machine,
//! and audits each call on a hash chain. It never admits a job itself. This library
//! is its whole behaviour; `devagotchi hub` and `devagotchi board` are its names, and
//! `devagotchi login`, `logout` and `whoami` (`account`) are how a machine signs in to one.

#[cfg(test)]
mod access_tests;
pub mod account;
mod accounts;
pub mod agenthub;
#[cfg(test)]
mod ahead_tests;
pub mod ai;
mod analytics;
#[cfg(test)]
mod analytics_tests;
mod audit;
mod auth;
mod config;
mod connect;
mod devices;
mod downloads;
mod files;
mod hub;
#[cfg(test)]
mod hub_settings_tests;
mod identity;
mod join;
#[cfg(test)]
mod keys_tests;
mod link;
mod machine_update;
mod mail;
#[cfg(test)]
mod me_tests;
mod nomen;
#[cfg(test)]
mod notes_tests;
mod oauth;
mod offers;
#[cfg(test)]
mod offers_tests;
mod orgs;
#[cfg(test)]
mod orgs_tests;
#[cfg(test)]
mod output_schema;
#[cfg(test)]
mod plugin_tests;
mod preferences;
#[cfg(test)]
mod provider_tests;
mod push;
mod run;
mod sealed;
mod signals;
mod site;
mod store;
#[cfg(test)]
mod store_tests;
mod tenant_access;
mod tenant_machines;
#[cfg(test)]
mod tenant_tests;
#[cfg(test)]
mod test_keys;
mod warehouse;
mod web;
#[cfg(test)]
mod workflow_tests;
mod workflows;

use std::{
    env, ffi::OsString, net::SocketAddr, path::PathBuf, process::ExitCode, sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Request, State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::Deserialize;

use crate::{
    audit::Audit,
    auth::{AuthError, Verifier},
    config::HubConfig,
    devices::Devices,
    hub::Hub,
    link::Links,
};

pub const USAGE: &str = "Usage:\n  devagotchi hub serve --state-dir ABS_PATH --config ABS_JSON\n  devagotchi hub enroll --state-dir ABS_PATH --device NAME\n  devagotchi hub revoke --state-dir ABS_PATH --device NAME\n  devagotchi hub devices --state-dir ABS_PATH\n  devagotchi hub owner --state-dir ABS_PATH --config ABS_JSON [--name NAME] [--replace]\n  devagotchi hub setup-code --state-dir ABS_PATH --config ABS_JSON\n  devagotchi hub connect-nomen --state-dir ABS_PATH --config ABS_JSON --client-id ID --project ID [--issuer URL]\n  devagotchi hub check-site ABS_DIR\n  devagotchi hub check-mail --config ABS_JSON [--to ADDRESS]\n  devagotchi hub analytics-resend --config ABS_JSON\n  devagotchi board <command> …            the agents' board in this project (board --help)\n\nserve starts the bastion: an authenticated Streamable HTTP MCP endpoint at /mcp that routes\nrun and operation_start/get/read/cancel to brokers by name, with whoami, /healthz and RFC 9728\nprotected-resource metadata, and /link where machines' `devagotchi link` connects out.\nWith a `web` section, people sign in with Shippin at /account to see their machines;\n/api/v1/me returns the same view for a session or a bearer token. Put it behind the edge for TLS.\n\nenroll prints a one-time code (15 minutes) that binds one machine's key to NAME;\nrevoke cuts that machine off within seconds; devices lists enrolled machines.\n\nowner (auth.mode hub) prints a one-time link (15 minutes) that makes the owner's passkey:\nthe first time it makes the owner's account, with the config's owner id if it names one;\nagain, it adds a passkey to the owner and ends the owner's other sessions; with --replace the\nlink also removes every other passkey of the owner (to take the account back).\n\nsetup-code (auth.mode devagotchi) prints a one-time code (15 minutes) that makes the account\nclaimed with it the hub's owner at /claim, or the owner already on file.\n\nconnect-nomen joins this hub to a Nomen with its owner's approval: it shows a code to confirm\non any device through the devagotchi-cli app (--client-id) of the hub's Nomen project (--project),\nthen adds the hub's own apps to that project (the key Nomen makes is written to the state\ndirectory, owner-only, and nowhere else), sets auth, owner and web in the config, and keeps a\ncopy of the old one. Issuer: https://id.shippin.cloud unless --issuer.\n\ncheck-site reads and checks a site directory (crates/hub/README.md, \"The site\") as serve would, and\nsays what is wrong and where; serve follows its site.dir and serves a change once it checks out.\n\ncheck-mail (auth.mode hub) connects to auth.email's mail server as serve would, negotiates TLS and\nsigns in as its user, sending nothing; with --to it also sends one email that says this hub's mail\nworks, to read in that mailbox. It prints the server's refusal and never the password; the running\nhub reports the same in /healthz under mail.\n\nanalytics-resend marks every board event in the hub's Postgres to be sent to ClickHouse again,\nas after a ClickHouse that lost its data: the running hub sends them, and ClickHouse keeps one\nrow per event. /healthz reports the export under analytics.";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Serve {
        state: PathBuf,
        config: PathBuf,
    },
    Enroll {
        state: PathBuf,
        device: String,
    },
    Revoke {
        state: PathBuf,
        device: String,
    },
    Devices {
        state: PathBuf,
    },
    Owner {
        state: PathBuf,
        config: PathBuf,
        name: Option<String>,
        /// The link removes every other passkey of the owner when used.
        replace: bool,
    },
    /// One-time code that makes the first claimed account the owner. Refuses when
    /// `hub.json` already names an owner.
    SetupCode {
        state: PathBuf,
        config: PathBuf,
    },
    ConnectNomen(connect::Connect),
    /// Read and check a site directory as `serve` would, and say whether it checks out.
    CheckSite {
        dir: PathBuf,
    },
    /// Connect to `auth.email`'s mail server and sign in as `serve` would, sending nothing;
    /// with `to`, also send one email that says this hub's mail works.
    CheckMail {
        config: PathBuf,
        to: Option<String>,
    },
    /// Mark every board event to be sent to ClickHouse again (a ClickHouse that lost its
    /// data); the running hub sends them.
    AnalyticsResend {
        config: PathBuf,
    },
    ImportBoard {
        state: PathBuf,
        config: PathBuf,
        source: PathBuf,
        tenant: String,
        apply: bool,
    },
}

#[derive(Clone)]
struct AppState {
    verifier: Arc<Verifier>,
    hub: Hub,
    issue_store: Arc<agenthub::issue_lifecycle::IssueLifecycleStore>,
    /// Browser sign-in through Nomen, when the config has a `web` section.
    web: Option<Arc<web::WebLogin>>,
    /// Browser sign-in with the hub's own accounts and passkeys (`auth.mode hub`).
    passkeys: Option<Arc<web::HubSignIn>>,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> BridgeResult<Command> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(Command::Help);
    }
    // `serve` is the verb's name in the nested command line; the bare form still works.
    if args.first().is_some_and(|a| a == "serve") {
        args.remove(0);
    }
    if args.first().is_some_and(|a| a == "connect-nomen") {
        return parse_connect(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "owner") {
        return parse_owner(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "setup-code") {
        return parse_setup_code(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "import-board") {
        return parse_board_import(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "analytics-resend") {
        let rest = args.split_off(1);
        return match rest.as_slice() {
            [flag, config] if flag == "--config" && std::path::Path::new(config).is_absolute() => {
                Ok(Command::AnalyticsResend {
                    config: PathBuf::from(config),
                })
            }
            _ => Err(BridgeError::new(
                "invalid_cli",
                "analytics-resend needs --config ABS_JSON",
            )),
        };
    }
    if args.first().is_some_and(|a| a == "check-mail") {
        return parse_check_mail(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "check-site") {
        return match args.get(1).map(PathBuf::from) {
            Some(dir) if args.len() == 2 && dir.is_absolute() => Ok(Command::CheckSite { dir }),
            _ => Err(BridgeError::new(
                "invalid_cli",
                "check-site needs one ABS_DIR, the site directory",
            )),
        };
    }
    let verb = match args.first().and_then(|a| a.to_str()) {
        Some(v @ ("enroll" | "revoke" | "devices")) => {
            let v = v.to_string();
            args.remove(0);
            Some(v)
        }
        _ => None,
    };
    let (mut state, mut config, mut device) = (None, None, None);
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() && verb.is_none() => {
                config = Some(PathBuf::from(value))
            }
            Some("--device") if device.is_none() && verb.is_some() => {
                device = Some(value.to_string_lossy().into_owned())
            }
            _ => return Err(BridgeError::new("invalid_cli", "unexpected flag")),
        }
    }
    let state = state
        .filter(|s| s.is_absolute())
        .ok_or_else(|| BridgeError::new("invalid_cli", "--state-dir ABS_PATH is required"))?;
    match (verb.as_deref(), config, device) {
        (None, Some(config), None) if config.is_absolute() => Ok(Command::Serve { state, config }),
        (Some("enroll"), None, Some(device)) => Ok(Command::Enroll { state, device }),
        (Some("revoke"), None, Some(device)) => Ok(Command::Revoke { state, device }),
        (Some("devices"), None, None) => Ok(Command::Devices { state }),
        _ => Err(BridgeError::new(
            "invalid_cli",
            "see --help for the accepted forms",
        )),
    }
}

fn parse_board_import(args: Vec<OsString>) -> BridgeResult<Command> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(Command::Help);
    }
    let (mut state, mut config, mut source, mut tenant) = (None, None, None, None);
    let mut apply = false;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag == "--apply" && !apply {
            apply = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--source") if source.is_none() => source = Some(PathBuf::from(value)),
            Some("--tenant") if tenant.is_none() => {
                tenant = Some(value.to_string_lossy().into_owned())
            }
            _ => {
                return Err(BridgeError::new(
                    "invalid_cli",
                    "unexpected import-board flag",
                ));
            }
        }
    }
    match (state, config, source, tenant) {
        (Some(state), Some(config), Some(source), Some(tenant))
            if state.is_absolute()
                && config.is_absolute()
                && source.is_absolute()
                && accounts::tenants::valid_tenant_id(&tenant) =>
        {
            Ok(Command::ImportBoard {
                state,
                config,
                source,
                tenant,
                apply,
            })
        }
        _ => Err(BridgeError::new(
            "invalid_cli",
            "import-board needs --state-dir ABS_PATH --config ABS_JSON --source ABS_SQLITE --tenant t-UUID [--apply]",
        )),
    }
}

fn parse_connect(args: Vec<OsString>) -> BridgeResult<Command> {
    let (mut state, mut config, mut client, mut project, mut issuer) =
        (None, None, None, None, None);
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        let text = value.to_string_lossy().into_owned();
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--client-id") if client.is_none() => client = Some(text),
            Some("--project") if project.is_none() => project = Some(text),
            Some("--issuer") if issuer.is_none() => issuer = Some(text),
            _ => return Err(BridgeError::new("invalid_cli", "unexpected flag")),
        }
    }
    match (state, config, client, project) {
        (Some(state), Some(config), Some(client_id), Some(project))
            if state.is_absolute() && config.is_absolute() =>
        {
            Ok(Command::ConnectNomen(connect::Connect {
                state,
                config,
                issuer: issuer.unwrap_or_else(|| account::DEFAULT_ISSUER.to_string()),
                client_id,
                project,
            }))
        }
        _ => Err(BridgeError::new(
            "invalid_cli",
            "connect-nomen needs --state-dir ABS_PATH --config ABS_JSON --client-id ID --project ID",
        )),
    }
}

fn parse_owner(args: Vec<OsString>) -> BridgeResult<Command> {
    let (mut state, mut config, mut name) = (None, None, None);
    let mut replace = false;
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        if flag == "--replace" && !replace {
            replace = true;
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--name") if name.is_none() => name = Some(value.to_string_lossy().into_owned()),
            _ => return Err(BridgeError::new("invalid_cli", "unexpected flag")),
        }
    }
    match (state, config) {
        (Some(state), Some(config)) if state.is_absolute() && config.is_absolute() => {
            Ok(Command::Owner {
                state,
                config,
                name,
                replace,
            })
        }
        _ => Err(BridgeError::new(
            "invalid_cli",
            "owner needs --state-dir ABS_PATH --config ABS_JSON",
        )),
    }
}

fn parse_setup_code(args: Vec<OsString>) -> BridgeResult<Command> {
    let (mut state, mut config) = (None, None);
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            _ => return Err(BridgeError::new("invalid_cli", "unexpected flag")),
        }
    }
    match (state, config) {
        (Some(state), Some(config)) if state.is_absolute() && config.is_absolute() => {
            Ok(Command::SetupCode { state, config })
        }
        _ => Err(BridgeError::new(
            "invalid_cli",
            "setup-code needs --state-dir ABS_PATH --config ABS_JSON",
        )),
    }
}

fn parse_check_mail(args: Vec<OsString>) -> BridgeResult<Command> {
    let (mut config, mut to) = (None, None);
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| BridgeError::new("invalid_cli", "flag without a value"))?;
        match flag.to_str() {
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--to") if to.is_none() => {
                let address = value.to_string_lossy().into_owned();
                if mail::address_of(&address).is_none() {
                    return Err(BridgeError::new(
                        "invalid_cli",
                        "--to needs an email address",
                    ));
                }
                to = Some(address);
            }
            _ => return Err(BridgeError::new("invalid_cli", "unexpected flag")),
        }
    }
    match config {
        Some(config) if config.is_absolute() => Ok(Command::CheckMail { config, to }),
        _ => Err(BridgeError::new(
            "invalid_cli",
            "check-mail needs --config ABS_JSON [--to ADDRESS]",
        )),
    }
}

/// Lines for `devagotchi hub setup-code`. An owner already on file is reported as
/// the Devagotchi account id plus `{public_url}/claim`. A new hub gets a code and
/// that same claim URL. The config file is not rewritten.
fn setup_code_lines(
    state: &std::path::Path,
    config_path: &std::path::Path,
) -> BridgeResult<Vec<String>> {
    let mut config = read_config(config_path)?;
    // Only `auth.mode devagotchi` spends a setup code (its `/claim`). A hub in `auth.mode hub`
    // prints its own claim link to its log, and `devagotchi hub owner` works on its host.
    if !matches!(config.auth, config::AuthConfig::Devagotchi { .. }) {
        return Err(BridgeError::new(
            "invalid_cli",
            "setup-code is for auth.mode devagotchi; in auth.mode hub use the claim link in the hub's log or devagotchi hub owner",
        ));
    }
    let store = identity::Store::open(&state.join("identities.sqlite3"))?;
    let public = config.public_url.trim_end_matches('/').to_string();
    let claim = format!("{public}/claim");
    identity::rekey_config(&store, &mut config)?;
    if let Some(owner) = config.owner.clone().or(store.hub_owner()?) {
        eprintln!(
            "devagotchi hub setup-code: owner {owner} already exists; no new code was issued"
        );
        return Ok(vec![format!("owner {owner}"), format!("claim {claim}")]);
    }
    let code = store.issue_setup_code()?;
    Ok(vec![format!("code {code}"), format!("claim {claim}")])
}

fn read_config(path: &std::path::Path) -> BridgeResult<HubConfig> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        BridgeError::new("invalid_config", format!("{}: {error}", path.display()))
    })?;
    HubConfig::parse_with(&text, |name| env::var(name).ok())
}

/// The accounts of a hub whose state is in `state_dir`: in the store `storage` names (see
/// `store.rs`: the environment's URL, the config's, or the state directory's SQLite file).
async fn accounts_at(
    state_dir: &std::path::Path,
    config: &HubConfig,
) -> BridgeResult<accounts::Accounts> {
    prepare_state_dir(state_dir)?;
    let url = store::url_for(config.storage_url(), state_dir);
    let schema = store::schema_for(config.storage_schema());
    let store = store::Store::open_in(&url, schema.as_deref()).await?;
    configured_accounts(store, config).await
}

async fn configured_accounts(
    store: store::Store,
    config: &HubConfig,
) -> BridgeResult<accounts::Accounts> {
    // A hosted hub on Postgres keeps its tenants apart in the database as well: its queries
    // run as a role row-level security holds, or it does not start.
    if config.multi_tenant() && store.backend() == store::Backend::Postgres && !store.enforced() {
        return Err(BridgeError::new(
            "account_store",
            "a hosted hub (tenancy.mode multi) on Postgres runs its queries as the role devagotchi_hub, held to row-level security, and its login role cannot switch to that role; see crates/hub/README.md, \"The store\"",
        ));
    }
    Ok(accounts::Accounts::open(store)
        .await?
        .with_tokens(config.token_config())
        .with_tenancy(config.tenancy.clone(), config.operators.clone())
        .with_site(config.site_cell(), &config.public_url)
        .with_analytics(config.counts_visits()))
}

fn prepare_state_dir(state_dir: &std::path::Path) -> BridgeResult<()> {
    std::fs::create_dir_all(state_dir)
        .map_err(|error| BridgeError::new("state_dir", error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(state_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| BridgeError::new("state_dir", error.to_string()))?;
    }
    Ok(())
}

fn devices_at(state_dir: &std::path::Path) -> BridgeResult<Devices> {
    prepare_state_dir(state_dir)?;
    Devices::open(&state_dir.join("devices.sqlite3"))
}

/// Record an operator command on the hub's audit chain: who ran it and what it did.
fn operator_event(
    state: &std::path::Path,
    tool: &str,
    device: &str,
    outcome: &str,
) -> BridgeResult<()> {
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    let subject = format!("operator:{user}");
    Audit::open(&state.join("audit.sqlite3"))?.record(&audit::AuditEntry {
        subject: &subject,
        via: "cli",
        tool,
        broker: device,
        operation_id: "",
        outcome,
    })?;
    Ok(())
}

fn manage(command: Command) -> BridgeResult<()> {
    match command {
        Command::ImportBoard {
            state,
            config,
            source,
            tenant,
            apply,
        } => {
            let config = read_config(&config)?;
            let snapshot = agenthub::migration::Snapshot::read(&source)?;
            let receipt = if apply {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
                runtime.block_on(async {
                    let url = store::url_for(
                        config
                            .storage
                            .as_ref()
                            .and_then(|storage| storage.url.as_deref()),
                        &state,
                    );
                    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
                        return Err(BridgeError::new(
                            "invalid_config",
                            "import-board requires Postgres storage",
                        ));
                    }
                    let schema = store::schema_for(
                        config
                            .storage
                            .as_ref()
                            .and_then(|storage| storage.schema.as_deref()),
                    );
                    let store = store::Store::open_in(&url, schema.as_deref()).await?;
                    snapshot.import_postgres(&store, &tenant).await
                })?
            } else {
                snapshot.receipt(&tenant)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&receipt)
                    .map_err(|error| BridgeError::new("board_import", error.to_string()))?
            );
            eprintln!(
                "{}",
                if apply {
                    "Board rows imported into Postgres. Source retained; live board routing has NOT been switched."
                } else {
                    "Preview only; no destination opened or modified. Stop source writers and back up before --apply."
                }
            );
        }
        Command::Enroll { state, device } => {
            if !config::valid_name(&device) {
                return Err(BridgeError::new(
                    "invalid_cli",
                    "device must be [a-z0-9][a-z0-9-]{0,62}, the broker name in the hub config",
                ));
            }
            let code = devices_at(&state)?.issue_code(&device, link::random::<16>())?;
            // The code itself is never audited or logged; only that one was issued.
            operator_event(&state, "link.code_issued", &device, "ok")?;
            // The code is the one secret this prints; it goes to stdout only, once.
            println!("{code}");
            eprintln!(
                "One-time code for {device}, valid {} minutes; any earlier unused code for {device} no longer works.\nOn that machine, with the code in DEVAGOTCHI_ENROLL_CODE (or on stdin):\n  devagotchi link enroll --state-dir ... --hub ... --device {device} --broker ...",
                devices::CODE_TTL_SECS / 60
            );
        }
        Command::Revoke { state, device } => {
            let (revoked, cancelled) = devices_at(&state)?.revoke(&device)?;
            operator_event(
                &state,
                "link.revoke",
                &device,
                &format!("revoked={revoked} codes_cancelled={cancelled}"),
            )?;
            match (revoked, cancelled) {
                (false, 0) => {
                    return Err(BridgeError::new(
                        "not_enrolled",
                        format!("no active device or pending code named {device}"),
                    ));
                }
                (true, _) => eprintln!(
                    "{device} revoked; its link closes within seconds ({cancelled} pending codes cancelled)"
                ),
                (false, _) => eprintln!("{cancelled} pending codes for {device} cancelled"),
            }
        }
        Command::Devices { state } => {
            let list = devices_at(&state)?.list()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&list).unwrap_or_default()
            );
        }
        Command::SetupCode { state, config } => {
            for line in setup_code_lines(&state, &config)? {
                if let Some(code) = line.strip_prefix("code ") {
                    eprintln!("devagotchi hub setup-code: {code}");
                }
                println!("{line}");
            }
        }
        Command::Owner {
            state,
            config,
            name,
            replace,
        } => {
            let config = read_config(&config)?;
            if !config.has_accounts() {
                return Err(BridgeError::new(
                    "invalid_cli",
                    "owner is for a hub with its own accounts (auth.mode hub)",
                ));
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            let (owner, token) = runtime.block_on(async {
                let accounts = accounts_at(&state, &config).await?;
                let owner = accounts
                    .ensure_owner(
                        config.owner_id(),
                        name.as_deref().unwrap_or("Owner"),
                        link::random::<16>(),
                    )
                    .await?;
                let token = if replace {
                    accounts
                        .issue_replacing_link(accounts::LinkPurpose::Owner, &owner.id)
                        .await?
                } else {
                    accounts
                        .issue_link(accounts::LinkPurpose::Owner, &owner.id)
                        .await?
                };
                Ok::<_, BridgeError>((owner, token))
            })?;
            // The link itself is never audited or logged; only that one was issued.
            operator_event(&state, "account.owner_link", &owner.id, "ok")?;
            // The link is the one secret this prints; it goes to stdout only, once.
            println!("{}/owner#{token}", config.public_url.trim_end_matches('/'));
            eprintln!(
                "One-time link for the owner ({}), valid {} minutes; any earlier unused one no longer works.\nOpen it to make a passkey or set a local email and password. Using it ends the owner's other sessions{}.",
                owner.id,
                accounts::LINK_TTL_SECS / 60,
                if replace {
                    " and removes every other passkey of the owner"
                } else {
                    " (add --replace to also remove the owner's other passkeys)"
                }
            );
        }
        Command::ConnectNomen(connect) => {
            prepare_state_dir(&connect.state)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            runtime.block_on(connect::run(&connect, &mut std::io::stderr()))?;
            operator_event(&connect.state, "nomen.connect", "", "ok")?;
        }
        Command::CheckSite { dir } => {
            let site = site::Site::load(&dir).map_err(|why| {
                BridgeError::new("invalid_site", format!("{}: {why}", dir.display()))
            })?;
            println!(
                "site {} ({}) checks out: {}",
                site.name(),
                site.info.mcp.name,
                dir.display()
            );
        }
        Command::AnalyticsResend { config } => {
            let config = read_config(&config)?;
            let url = store::url_for(config.storage_url(), std::path::Path::new("."));
            if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
                return Err(BridgeError::new(
                    "invalid_config",
                    "analytics-resend reads the hub's Postgres: set storage.url or DEVAGOTCHI_HUB_STORAGE_URL",
                ));
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            runtime.block_on(async {
                let schema = store::schema_for(config.storage_schema());
                let store = store::Store::open_in(&url, schema.as_deref()).await?;
                let marked = warehouse::resend(&store).await?;
                println!(
                    "marked {marked} board events to send again; the running hub sends them to ClickHouse, which keeps one row per event"
                );
                Ok::<_, BridgeError>(())
            })?;
        }
        Command::CheckMail { config, to } => {
            let config = read_config(&config)?;
            let Some(email) = config.sign_ins().1 else {
                return Err(BridgeError::new(
                    "invalid_config",
                    "check-mail is for a hub that sends mail (auth.mode hub with auth.email); this config names no mail server",
                ));
            };
            // The URL and the password are read as serve reads them: a variable that is not
            // set is the first thing this reports.
            let mailer = web::Mailer::from_config(email)
                .map_err(|why| BridgeError::new("invalid_config", why))?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            runtime.block_on(async {
                mailer
                    .check()
                    .await
                    .map_err(|why| BridgeError::new("mail", why))?;
                println!("mail server {} accepts this hub", mailer.describe());
                if let Some(to) = &to {
                    mailer
                        .send_check(&config.site(), to, &config.public_url)
                        .await
                        .map_err(|why| BridgeError::new("mail", why))?;
                    println!("sent the mail check to {to}: read it there");
                }
                Ok::<_, BridgeError>(())
            })?;
        }
        Command::Help | Command::Serve { .. } => unreachable!(),
    }
    Ok(())
}

/// The hub's command line: `serve …`, `enroll …`, `revoke …`, `devices …`, `board …`, or `--help`.
pub fn cli(args: Vec<OsString>) -> ExitCode {
    let (state_dir, config_path) = match parse_args(args) {
        Ok(Command::Serve { state, config }) => (state, config),
        Ok(Command::Help) => {
            println!("{USAGE}\n\n{}", agenthub::migration::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(command) => {
            return match manage(command) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("devagotchi hub: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        Err(error) => {
            eprintln!("devagotchi hub: {error}\n\n{USAGE}");
            return ExitCode::from(64);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("devagotchi hub: runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(state_dir, config_path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("devagotchi hub: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The board's command line on this process's stdio, as the OS user, from the current
/// directory: what `devagotchi board …` runs.
pub fn board_cli(args: Vec<OsString>, account_file: Option<PathBuf>) -> ExitCode {
    use std::io::IsTerminal;
    let invocation = agenthub::cli::Invocation {
        args,
        cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        actor_env: env::var("DEVAGOTCHI_ACTOR")
            .ok()
            .filter(|a| !a.trim().is_empty()),
        session_env: env::var("DEVAGOTCHI_SESSION")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        os_user: env::var("USER").or_else(|_| env::var("USERNAME")).ok(),
        stdout_is_terminal: std::io::stdout().is_terminal(),
        account_file,
        plugin_host: agenthub::cli::plugin_host(|name| env::var_os(name)),
    };
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    ExitCode::from(agenthub::cli::run(invocation, &mut out, &mut err))
}

async fn run(state_dir: PathBuf, config_path: PathBuf) -> BridgeResult<()> {
    let mut config = read_config(&config_path)?;
    let url = store::url_for(config.storage_url(), &state_dir);
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err(BridgeError::new(
            "invalid_config",
            "hub serve requires Postgres for accounts and boards; set storage.url or DEVAGOTCHI_HUB_STORAGE_URL. SQLite is only for standalone local work and offline client storage.",
        ));
    }
    let schema = store::schema_for(config.storage_schema());
    let board_store = store::Store::open_in(&url, schema.as_deref()).await?;
    // Old owner and grant subjects (identity-provider subs) become Devagotchi
    // account ids in memory. The file is left as the operator wrote it.
    let rekey = if matches!(config.auth, config::AuthConfig::Devagotchi { .. }) {
        let store = identity::Store::open(&state_dir.join("identities.sqlite3"))?;
        let pairs = identity::rekey_config(&store, &mut config)?;
        Some(pairs)
    } else {
        None
    };
    let config = Arc::new(config);
    // The running hub follows its site directory: a change is served without a restart,
    // and one that does not check out is refused while the site it had stays.
    if let Some(dir) = &config.site.dir {
        site::watch(config.site_cell(), PathBuf::from(dir), site::WATCH_EVERY);
    }
    let devices = Arc::new(devices_at(&state_dir)?);
    if let Some(pairs) = &rekey {
        let boards = config
            .account_boards
            .as_ref()
            .and_then(|accounts| accounts.dir.as_deref().map(std::path::PathBuf::from))
            .unwrap_or_else(|| state_dir.join("account-boards"));
        for (from, to) in pairs {
            devices.reassign_owner(from, to)?;
            identity::rename_account_dir(&boards, from, to)?;
        }
    }
    let audit = Arc::new(Audit::open(&state_dir.join("audit.sqlite3"))?);
    // A hub with its own accounts knows its owner from them, and refuses to start when its
    // config names someone else.
    let accounts = if config.has_accounts() {
        let accounts = Arc::new(configured_accounts(board_store.clone(), &config).await?);
        if let Some(owner) = accounts.owner().await? {
            config
                .learn_owner(&owner.id)
                .map_err(|why| BridgeError::new("invalid_config", why))?;
        }
        Some(accounts)
    } else {
        None
    };
    // Sign in with Google, GitHub, OpenID Connect and the email link: their secrets are read
    // now, so one that is missing stops the start rather than a sign-in later.
    let providers = if config.has_accounts() {
        Some(
            web::Providers::from_config(&config)
                .map_err(|why| BridgeError::new("invalid_config", why))?
                .sealing_in(&state_dir)
                .map_err(|why| BridgeError::new("invalid_config", why))?,
        )
    } else {
        None
    };
    // A tenant let in hears so by email when the hub can send mail (early access).
    if let (Some(accounts), Some(mailer)) = (
        &accounts,
        providers
            .as_ref()
            .and_then(|providers| providers.email.clone()),
    ) {
        accounts.on_admitted(web::announce_admissions(mailer, config.clone()));
    }
    let passkeys = accounts.clone().and_then(|accounts| {
        web::HubSignIn::new(config.clone(), accounts)
            .map(|hub| match providers {
                Some(providers) => hub.with_providers(providers),
                None => hub,
            })
            .map(Arc::new)
    });
    // What the owner set on the settings page, over what the config file says.
    if let Some(hub) = &passkeys {
        hub.reload_providers().await?;
    }
    // A one-owner hub whose config names neither its owner nor its operators may be claimed
    // from the browser (`accounts/claim.rs`); its code is issued once the hub listens, below.
    // A hosted hub (`tenancy.mode multi`) is never claimed.
    let claimable = match (&passkeys, &accounts) {
        (Some(_), Some(accounts)) if config.claim_allowed() => Some(accounts.clone()),
        _ => None,
    };
    let push = match (&config.web, &passkeys) {
        (None, None) => None,
        _ => Some(Arc::new(push::Push::open(
            &state_dir,
            &config.public_url,
            devices.clone(),
        )?)),
    };
    // A hosted hub counts visits and its accounts' milestones for its operator, in its store.
    let analytics = match &accounts {
        Some(accounts) if config.counts_visits() => Some(analytics::Analytics::start(
            accounts.store().clone(),
            config.analytics,
            &config.public_url,
        )),
        _ => None,
    };
    // A downloads directory that does not check out is logged and left out: the hub still
    // serves everything else, and /healthz says why /join is off.
    let downloads = match &config.downloads {
        Some(dir) => match downloads::Downloads::open(std::path::Path::new(dir)) {
            Ok(downloads) => {
                eprintln!(
                    "devagotchi hub: handing out build {} to joining machines ({} files checked in {dir})",
                    downloads.version,
                    downloads.files().count()
                );
                Some(Arc::new(downloads))
            }
            Err(error) => {
                eprintln!("devagotchi hub: /join is off: {error}");
                None
            }
        },
        None => None,
    };
    // What a workflow's wait wakes on: a machine connecting or dropping, a phone turned on.
    let changes = Arc::new(tokio::sync::Notify::new());
    let links = Links::new(config.clone(), devices.clone(), audit.clone())
        .notifying(push.clone())
        .counting(analytics.clone())
        .signalling(changes.clone())
        .offering(downloads.as_ref().map(|d| d.version.clone()));
    // A machine joins for an account only once its tenant is let in (early access).
    let links = Arc::new(match &accounts {
        Some(accounts) => links.gated(accounts.enroll_gate()),
        None => links,
    });
    // Machines on another build update themselves, checked hourly (`machine_update.rs`).
    tokio::spawn(links.clone().auto_update_hourly());
    let workflows = accounts.clone().map(|accounts| {
        Arc::new(workflows::Workflows::new(
            accounts,
            changes,
            config.workflows.link_ttl_secs,
        ))
    });
    // The site's offers on a hub with its own accounts: a provisioning token that is
    // missing stops the start (`config/shippin.rs`).
    let offers = match &accounts {
        Some(accounts) => Some(Arc::new(
            offers::Offers::from_config(&config, accounts.clone())
                .map_err(|why| BridgeError::new("invalid_config", why))?,
        )),
        None => None,
    };
    let verifier = Arc::new(Verifier::from_hub_config(&config, &state_dir, accounts).await?);
    let web = match (&config.web, &config.auth) {
        (
            Some(web),
            config::AuthConfig::Nomen {
                issuer, audiences, ..
            },
        ) => Some(Arc::new(web::WebLogin::new(
            issuer,
            audiences,
            &web.client_id,
            &config.public_url,
            &state_dir,
        )?)),
        _ => None,
    };
    let listen: SocketAddr = config
        .listen
        .parse()
        .map_err(|_| BridgeError::new("invalid_config", "listen"))?;
    let warehouse_store = board_store.clone();
    let signals_store = board_store.clone();
    let boards = Arc::new(
        agenthub::mcp::Boards::from_config(&config, &state_dir).using_postgres(board_store)?,
    );
    if let Some(preferences) = boards.preferences() {
        links.set_preferences(preferences.clone());
    }
    // Board events to ClickHouse for statistics (ADR 0001): reported in /healthz, never
    // required for the boards themselves.
    if let Some(clickhouse) = &config.clickhouse {
        let password = std::env::var(config::CLICKHOUSE_PASSWORD_ENV).ok();
        let warehouse = Arc::new(warehouse::Warehouse::new(
            clickhouse,
            warehouse_store,
            password,
        )?);
        tokio::spawn(warehouse.clone().run());
        boards.set_warehouse(warehouse);
        eprintln!(
            "devagotchi hub: exporting board events to ClickHouse at {} (database {})",
            clickhouse.url, clickhouse.database
        );
    }
    // Signals to the operator's sinks (`signals.rs`, docs/signals.md): each on its own task,
    // reported in /healthz, never holding back the hub or another sink.
    let mut sinks = Vec::new();
    for config in &config.signals.sinks {
        let sink = Arc::new(signals::Sink::new(
            config,
            signals_store.clone(),
            audit.clone(),
            |name| env::var(name).ok(),
        )?);
        tokio::spawn(sink.clone().run());
        eprintln!(
            "devagotchi hub: signals ({}) to sink {} at {}",
            signals::SCHEMA,
            config.name,
            config.url
        );
        sinks.push(sink);
    }
    boards.set_sinks(sinks);
    let listener = agenthub::mcp::Boards::postgres_listener(&url).await?;
    tokio::spawn(boards.clone().listen_postgres(listener));
    if !boards.is_empty() {
        // Lapsed leases go back to their queue when they lapse: the reaper sleeps until
        // the next lease ends or a board changes. Claims also expire them lazily.
        tokio::spawn(boards.clone().reap_on_time());
    }
    let issue_store = Arc::new(agenthub::issue_lifecycle::IssueLifecycleStore::from_config(
        &config.github,
    )?);
    let state = AppState {
        verifier,
        hub: Hub {
            push,
            downloads,
            config: config.clone(),
            audit,
            links,
            boards,
            analytics,
            workflows,
            offers,
            routed: Default::default(),
        },
        issue_store,
        web,
        passkeys,
    };

    let app = app(state.clone());

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|error| BridgeError::new("listen", format!("{listen}: {error}")))?;
    // Unclaimed, the hub prints a new claim code each time it starts, replacing the last
    // one: only once it listens, so a start that fails never replaces the code a running
    // hub printed.
    let claim_code = match &claimable {
        Some(accounts) => accounts.issue_claim_code().await?,
        None => None,
    };
    if claim_code.is_some() {
        // That a code was issued, and when; never the code.
        state.hub.audit.record(&audit::AuditEntry {
            subject: "hub",
            via: "start",
            tool: "account.claim_code",
            broker: "",
            operation_id: "",
            outcome: "ok",
        })?;
    }
    // Everything worth reading at start comes before the ready line callers wait for.
    if let Some(owner) = config.owner_id() {
        eprintln!("devagotchi hub owner: {owner} reaches every broker");
    }
    if config.multi_tenant() {
        // How many operators, never who: they are people's sign-ins.
        eprintln!(
            "devagotchi hub is hosted (tenancy multi): each person's account and tenant are made when they first connect; {} operator sign-ins configured",
            config.operators.len()
        );
    }
    if state.web.is_some() || state.passkeys.is_some() {
        eprintln!(
            "devagotchi hub web sign-in: {}/account",
            config.public_url.trim_end_matches('/')
        );
    }
    if state.passkeys.is_some() && !config.public_url.starts_with("https://") {
        eprintln!(
            "devagotchi hub: signing people in over plain http on loopback, for tests only: its cookies reach any other service on this host's loopback"
        );
    }
    // The claim link is the one secret the hub writes to its log, on purpose: reading the
    // log takes the host or its container platform, which is what owning the hub takes.
    if let Some(claim) = &claim_code {
        eprintln!(
            "devagotchi hub is unclaimed: nobody can sign in as its owner yet. Claim it in a browser before {}:\n  {}/claim#{}\nRestarting the hub prints a new code. Or, on this host: devagotchi hub owner --state-dir {} --config {}",
            web::utc(claim.expires_at),
            config.public_url.trim_end_matches('/'),
            claim.code,
            state_dir.display(),
            config_path.display()
        );
    }
    eprintln!(
        "devagotchi hub ready: http://{listen}/mcp ({} brokers, auth {})",
        config.brokers.len(),
        match &config.auth {
            config::AuthConfig::Nomen { issuer, .. } => format!("nomen {issuer}"),
            config::AuthConfig::Oidc { issuer, .. } => format!("oidc {issuer}"),
            config::AuthConfig::Dev { .. } => "dev tokens".to_string(),
            config::AuthConfig::Devagotchi { .. } => "devagotchi".to_string(),
            config::AuthConfig::Hub { .. } => match config.auth.nomen() {
                Some(nomen) => format!("hub accounts, and nomen {}", nomen.issuer),
                None => "hub accounts".to_string(),
            },
        }
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .map_err(|error| BridgeError::new("serve", error.to_string()))?;
    eprintln!("devagotchi hub stopped");
    Ok(())
}

/// The whole hub as one router: `/mcp` behind the bearer check, `/link`, `/healthz`, the
/// protected-resource metadata and the web pages.
fn app(state: AppState) -> Router {
    let config = &state.hub.config;
    let public_host = config
        .public_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_string();
    let mcp_config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_allowed_hosts([
            public_host,
            config.listen.clone(),
            "localhost".into(),
            "127.0.0.1".into(),
        ])
        .with_sse_keep_alive(Some(Duration::from_secs(15)));
    let mcp = {
        let hub = state.hub.clone();
        StreamableHttpService::new(
            move || Ok(hub.for_session()),
            Arc::new(LocalSessionManager::default()),
            mcp_config,
        )
    };
    // `auth.mode devagotchi` is its own authorization server (`oauth.rs`), which serves
    // `/claim`, `/.well-known/oauth-authorization-server` and the sign-in pages; every other
    // mode gets the hub's (32b, `web/oauth.rs`) and the browser claim (16c). Each path is
    // mounted once, so the router builds in every mode (`app_builds_in_every_auth_mode`).
    let authority = state.verifier.authority();
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/enroll", axum::routing::post(enroll_with_code))
        .route("/link", get(link_upgrade))
        .route(
            "/webhooks/github/issues",
            axum::routing::post(agenthub::issue_lifecycle::webhooks_routes).layer(
                axum::extract::DefaultBodyLimit::max(agenthub::issue_lifecycle::MAX_BODY),
            ),
        )
        .merge(
            Router::new()
                .route(
                    "/api/v1/agenthub/issue",
                    get(agenthub::issue_lifecycle::issue_state_route),
                )
                .route(
                    "/api/v1/agenthub/state",
                    get(agenthub::issue_lifecycle::shared_state_read_route)
                        .post(agenthub::issue_lifecycle::shared_state_write_route),
                )
                .layer(axum::extract::DefaultBodyLimit::max(
                    agenthub::issue_lifecycle::MAX_BODY,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    require_bearer,
                )),
        )
        // Board statistics from ClickHouse (`warehouse`), for anyone who reads the board.
        .merge(warehouse::routes().layer(middleware::from_fn_with_state(
            state.clone(),
            orgs::api::bearer_or_session,
        )))
        // Organisations, members, teams, project roles and invites (`orgs::api`).
        .merge(
            orgs::api::routes()
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    orgs::api::bearer_or_session,
                )),
        )
        .merge(join::routes())
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_mcp),
        )
        .nest_service(
            "/mcp",
            Router::new()
                .fallback_service(mcp)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    require_bearer,
                )),
        )
        .merge(
            // The bar runs first: it marks the operator's requests, which are not counted.
            web::router(authority.is_none())
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    web::counted::count,
                ))
                .layer(middleware::from_fn_with_state(state.clone(), web::bar)),
        );
    let router = match authority {
        Some(authority) => router.merge(oauth::http_router(authority)).route(
            "/api/v1/admin/settings",
            get(admin_settings_get).put(admin_settings_put),
        ),
        None => router,
    };
    router.with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminSettingsBody {
    provisional: oauth::ProvisionalPatch,
}

fn settings_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "error_description": description,
        })),
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

fn audit_settings(state: &AppState, subject: &auth::Subject, outcome: &str) {
    let _ = state.hub.audit.record(&audit::AuditEntry {
        subject: &subject.id,
        via: subject.via,
        tool: "admin.settings",
        broker: "",
        operation_id: "",
        outcome,
    });
}

async fn require_operator(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<auth::Subject, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(settings_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "bearer token required",
        ));
    };
    let subject = match state.verifier.verify(&token).await {
        Ok(subject) => subject,
        Err(_) => {
            return Err(settings_error(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "bearer token was refused",
            ));
        }
    };
    if !state.hub.config.is_operator(&subject.id) {
        audit_settings(state, &subject, "denied");
        return Err(settings_error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "owner or an operator grant is required",
        ));
    }
    Ok(subject)
}

async fn admin_settings_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_operator(&state, &headers).await {
        return response;
    }
    let Some(authority) = state.verifier.authority() else {
        return settings_error(
            StatusCode::NOT_FOUND,
            "not_available",
            "admin settings require auth.mode devagotchi",
        );
    };
    Json(authority.operator_settings()).into_response()
}

async fn admin_settings_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AdminSettingsBody>,
) -> Response {
    let subject = match require_operator(&state, &headers).await {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let Some(authority) = state.verifier.authority() else {
        return settings_error(
            StatusCode::NOT_FOUND,
            "not_available",
            "admin settings require auth.mode devagotchi",
        );
    };
    match authority.update_operator_settings(&body.provisional) {
        Ok(settings) => {
            let outcome = format!(
                "ok ttl_days={} starts_per_hour={} unclaimed_cap={}",
                settings.provisional.ttl_days,
                settings.provisional.starts_per_hour,
                settings.provisional.unclaimed_cap
            );
            audit_settings(&state, &subject, &outcome);
            Json(settings).into_response()
        }
        Err(error) => {
            let outcome = format!("rejected {}", error.message);
            audit_settings(&state, &subject, &outcome);
            settings_error(StatusCode::BAD_REQUEST, &error.code, &error.message)
        }
    }
}

async fn require_bearer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // `auth.mode devagotchi`: accounts merged since the last call take their machines along.
    if let Some(authority) = state.verifier.authority() {
        for (from, into) in authority.drain_merges() {
            let _ = state.hub.links.reassign_owner(&from, &into);
        }
    }
    let origin = state.hub.config.public_url.trim_end_matches('/');
    let (metadata_url, scope) = if state.verifier.authority().is_some() {
        (
            format!("{origin}/.well-known/oauth-protected-resource/mcp"),
            ", scope=\"openid email profile offline_access mcp\"".to_string(),
        )
    } else {
        let scope = match state.hub.config.auth.nomen() {
            _ if state.hub.config.has_accounts() => {
                format!(", scope=\"{}\"", web::oauth::SCOPES.join(" "))
            }
            Some(nomen) => format!(
                ", scope=\"{}\"",
                crate::auth::sign_in_scopes(nomen.audiences)
            ),
            None => String::new(),
        };
        (
            format!("{origin}/.well-known/oauth-protected-resource"),
            scope,
        )
    };
    let metadata = format!("Bearer resource_metadata=\"{metadata_url}\"{scope}");
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .map(str::to_string);
    let Some(token) = token else {
        return challenge(StatusCode::UNAUTHORIZED, &metadata, None);
    };
    // An API key reaches this endpoint only, and only with the limits it was made with.
    if token.starts_with("dgk_") {
        let found = match &state.passkeys {
            Some(hub) => hub.accounts().verify_key(&token).await,
            None => Ok(None),
        };
        return match found {
            Ok(Some((subject, limits))) => {
                request.extensions_mut().insert(subject);
                request.extensions_mut().insert(limits);
                next.run(request).await
            }
            Ok(None) => {
                log_refusal("unknown, revoked or expired API key");
                challenge(StatusCode::UNAUTHORIZED, &metadata, Some("invalid_token"))
            }
            Err(error) => {
                log_unavailable(&error.message);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::RETRY_AFTER, "5")],
                    "the hub's store did not answer",
                )
                    .into_response()
            }
        };
    }
    match state.verifier.verify(&token).await {
        Ok(subject) => {
            request.extensions_mut().insert(subject);
            next.run(request).await
        }
        Err(AuthError::Invalid(why)) => {
            log_refusal(why);
            challenge(StatusCode::UNAUTHORIZED, &metadata, Some("invalid_token"))
        }
        Err(AuthError::Unavailable(why)) => {
            log_unavailable(&why);
            // Clients retry on their own; say when, and why, so an outage reads as one.
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "5")],
                format!("identity provider unavailable: {why}"),
            )
                .into_response()
        }
    }
}

/// An identity-provider outage, for the operator: the verifier logs when it starts and
/// ends; per request, at most one line every 30 s with how many were left out.
pub(crate) fn log_unavailable(why: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    static SUPPRESSED: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now >= last + 30
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let suppressed = SUPPRESSED.swap(0, Ordering::Relaxed);
        if suppressed > 0 {
            eprintln!(
                "devagotchi hub: issuer unavailable: {why} ({suppressed} more since the last line)"
            );
        } else {
            eprintln!("devagotchi hub: issuer unavailable: {why}");
        }
    } else {
        SUPPRESSED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Why a token was refused, for the operator: a wrong audience or client in the config
/// looks to the caller exactly like a bad token. At most one line a second, never the token.
fn log_refusal(why: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    static SUPPRESSED: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now > last
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let quiet = SUPPRESSED.swap(0, Ordering::Relaxed);
        let more = if quiet > 0 {
            format!(" ({quiet} more refused since the last line)")
        } else {
            String::new()
        };
        eprintln!("devagotchi hub: refused a bearer token: {why}{more}");
    } else {
        SUPPRESSED.fetch_add(1, Ordering::Relaxed);
    }
}

fn challenge(status: StatusCode, metadata: &str, error: Option<&str>) -> Response {
    let value = match error {
        Some(error) => format!("{metadata}, error=\"{error}\""),
        None => metadata.to_string(),
    };
    let mut response = (status, "authentication required").into_response();
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, header_value);
    }
    response
}

async fn protected_resource(State(state): State<AppState>) -> Response {
    if let Some(authority) = state.verifier.authority() {
        return axum::Json(authority.protected_resource(false)).into_response();
    }
    let resource = state
        .hub
        .config
        .public_url
        .trim_end_matches('/')
        .to_string();
    resource_metadata(&state, resource)
}

/// RFC 9728 §3.1: the metadata at `/.well-known/oauth-protected-resource/mcp` is for the
/// resource `<public_url>/mcp`, the URL an MCP client was given.
async fn protected_resource_mcp(State(state): State<AppState>) -> Response {
    if let Some(authority) = state.verifier.authority() {
        return axum::Json(authority.protected_resource(true)).into_response();
    }
    let resource = format!("{}/mcp", state.hub.config.public_url.trim_end_matches('/'));
    resource_metadata(&state, resource)
}

fn resource_metadata(state: &AppState, resource: String) -> Response {
    let Some(issuer) = state.verifier.issuer() else {
        return (StatusCode::NOT_FOUND, "no authorization server in dev mode").into_response();
    };
    let mut body = serde_json::json!({
        "resource": resource,
        "authorization_servers": [issuer],
        "bearer_methods_supported": ["header"],
    });
    resource_documents(
        &mut body,
        &state.hub.config.site(),
        &state.hub.config.public_url,
    );
    if state.hub.config.has_accounts() {
        // The hub is its own authorization server (32b).
        body["scopes_supported"] = serde_json::json!(web::oauth::SCOPES);
    } else if let Some(nomen) = state.hub.config.auth.nomen() {
        // MCP clients ask for exactly these (RFC 9728 `scopes_supported`), so the account's
        // org reaches the hub with every call.
        body["scopes_supported"] = crate::auth::sign_in_scopes(nomen.audiences)
            .split(' ')
            .collect::<Vec<_>>()
            .into();
    }
    let mut response = axum::Json(body).into_response();
    web::oauth::allow_any_origin(&mut response);
    response
}

/// RFC 9728's human-readable fields: the product's name, and where its docs, privacy
/// policy and terms are on this hub.
pub(crate) fn resource_documents(
    body: &mut serde_json::Value,
    site: &site::Site,
    public_url: &str,
) {
    let hub = public_url.trim_end_matches('/');
    body["resource_name"] = site.name().into();
    body["resource_documentation"] = format!("{hub}/docs").into();
    body["resource_policy_uri"] = format!("{hub}/privacy").into();
    body["resource_tos_uri"] = format!("{hub}/terms").into();
}

/// Machines dial in here; the device proves its key inside the socket, not with a bearer.
async fn link_upgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let links = state.hub.links.clone();
    // Refuse at the door when too many handshakes are already in flight.
    let Some(permit) = links.handshake_permit() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "too many link handshakes").into_response();
    };
    upgrade
        .max_message_size(devagotchi_contracts::MAX_LINK_MESSAGE_BYTES)
        .max_frame_size(devagotchi_contracts::MAX_LINK_MESSAGE_BYTES)
        .on_upgrade(move |socket| links.serve(socket, peer, permit))
}

#[derive(serde::Deserialize)]
struct EnrollBody {
    name: String,
    code: String,
    public_key: String,
}

/// Bind a machine key to a one-time code from `machine_add`. The code is the secret.
async fn enroll_with_code(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<EnrollBody>,
) -> Response {
    // An account waiting to be let in enrolls nothing (`tenant_access.rs`).
    if let Err(error) = state.hub.links.may_enroll(&body.code).await {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": error.code, "message": error.message })),
        )
            .into_response();
    }
    match state
        .hub
        .links
        .enroll_owned(&body.name, &body.code, &body.public_key)
    {
        Ok(device) => (
            StatusCode::CREATED,
            axum::Json(serde_json::json!({
                "name": device.name,
                "owner": device.owner,
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": error.code, "message": error.message })),
        )
            .into_response(),
    }
}

async fn healthz(State(state): State<AppState>) -> Response {
    let mut brokers = Vec::new();
    let mut all_up = true;
    for broker in &state.hub.config.brokers {
        match &broker.socket {
            Some(socket) => {
                let reachable = reachable(socket).await;
                all_up &= reachable;
                brokers.push(serde_json::json!({ "name": broker.name, "reachable": reachable }));
            }
            // A laptop that is asleep does not make the hub unhealthy; it is reported.
            None => brokers.push(serde_json::json!({
                "name": broker.name,
                "link": true,
                "connected": state.hub.links.connected(&broker.name),
            })),
        }
    }
    let audit = state.hub.audit.verify();
    let chain_ok = audit.as_ref().map(|status| status.ok).unwrap_or(false);
    let auth = if let Some(authority) = state.verifier.authority() {
        authority.readiness().await
    } else {
        serde_json::Value::Null
    };
    let signing_ok = auth
        .get("signing_key")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // The identity provider is reported, not counted: an outage there leaves the hub, its
    // links and confirmed sign-ins working (see nomen.rs).
    let mut body = serde_json::json!({
        "ok": all_up && chain_ok && signing_ok,
        "brokers": brokers,
        "audit": audit.ok(),
        "issuer": state.verifier.issuer_health(),
        "downloads": match (&state.hub.downloads, &state.hub.config.downloads) {
            (Some(downloads), _) => serde_json::json!({ "version": downloads.version, "files": downloads.files().count() }),
            (None, Some(_)) => serde_json::json!({ "off": "the downloads directory did not check out; see the hub's log" }),
            (None, None) => serde_json::Value::Null,
        },
    });
    // The hub's own accounts: whether the database holds its queries to one tenant's rows
    // (Postgres row-level security; false on SQLite, a standalone hub's own file).
    if let Some(passkeys) = &state.passkeys {
        body["store"] = serde_json::json!({ "tenant_rls": passkeys.tenant_rls() });
        // Whether it can send mail and how the last send went: reported, not counted, like
        // the issuer. Without mail people still sign in by password, passkey and provider.
        body["mail"] = passkeys.mail_status();
    }
    // The export to ClickHouse: reported, not counted. While ClickHouse is down the boards
    // work, the events wait in Postgres, and this says how many and since when.
    body["analytics"] = match state.hub.boards.warehouse() {
        Some(warehouse) => warehouse.health().await,
        None => serde_json::json!({ "configured": false }),
    };
    // The signal sinks: reported, not counted. One that refuses falls behind; nothing else does.
    body["signals"] = serde_json::Value::Array(
        state
            .hub
            .boards
            .sinks()
            .iter()
            .map(|sink| sink.health())
            .collect(),
    );
    // Only `auth.mode devagotchi` reports its authorization server's readiness here.
    if !auth.is_null() {
        body["auth"] = auth;
    }
    let status = if all_up && chain_ok && signing_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, axum::Json(body)).into_response()
}

/// A broker is reachable when it answers a real request over its own IPC: the same
/// owner checks, framing and endpoint a routed call uses, on every platform.
async fn reachable(socket: &str) -> bool {
    let body = devagotchi_contracts::RequestBody::OperationList(devagotchi_contracts::ListInput {
        offset: 0,
        limit: 1,
    });
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            devagotchi_ipc::request(std::path::Path::new(socket), body)
        )
        .await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    const STATE: &str = "/s";
    #[cfg(windows)]
    const STATE: &str = r"C:\s";
    #[cfg(unix)]
    const CONFIG: &str = "/c.json";
    #[cfg(windows)]
    const CONFIG: &str = r"C:\c.json";

    #[test]
    fn cli_requires_both_absolute_paths() {
        let ok =
            parse_args(["--state-dir", STATE, "--config", CONFIG].map(OsString::from)).unwrap();
        assert_eq!(
            ok,
            Command::Serve {
                state: PathBuf::from(STATE),
                config: PathBuf::from(CONFIG)
            }
        );
        assert!(parse_args(["--config", CONFIG].map(OsString::from)).is_err());
        assert!(parse_args(["--state-dir", "s", "--config", CONFIG].map(OsString::from)).is_err());
        assert_eq!(
            parse_args(["--help"].map(OsString::from)).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(["serve", "--state-dir", STATE, "--config", CONFIG].map(OsString::from))
                .unwrap(),
            ok,
            "the nested spelling and the bare one are the same command"
        );
    }

    #[test]
    fn analytics_resend_takes_the_config_alone() {
        assert_eq!(
            parse_args(["analytics-resend", "--config", CONFIG].map(OsString::from)).unwrap(),
            Command::AnalyticsResend {
                config: PathBuf::from(CONFIG)
            }
        );
        for bad in [
            vec!["analytics-resend"],
            vec!["analytics-resend", "--config", "c.json"],
            vec!["analytics-resend", "--config", CONFIG, "--to", "x"],
        ] {
            assert!(parse_args(bad.into_iter().map(OsString::from)).is_err());
        }
    }

    #[test]
    fn check_mail_takes_the_config_and_one_address() {
        assert_eq!(
            parse_args(["check-mail", "--config", CONFIG].map(OsString::from)).unwrap(),
            Command::CheckMail {
                config: PathBuf::from(CONFIG),
                to: None
            }
        );
        assert_eq!(
            parse_args(
                ["check-mail", "--config", CONFIG, "--to", "you@example.com"].map(OsString::from)
            )
            .unwrap(),
            Command::CheckMail {
                config: PathBuf::from(CONFIG),
                to: Some("you@example.com".into())
            }
        );
        for wrong in [
            vec!["check-mail", "--config", "c.json"],
            vec!["check-mail", "--to", "you@example.com"],
            vec!["check-mail", "--config", CONFIG, "--to", "nobody"],
            vec!["check-mail", "--config", CONFIG, "--state-dir", STATE],
        ] {
            assert!(
                parse_args(wrong.iter().map(OsString::from)).is_err(),
                "{wrong:?}"
            );
        }
    }

    #[test]
    fn check_mail_signs_in_and_sends_the_check_to_one_address() {
        // The stub mail server lives on its own runtime; `manage` builds one of its own.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let stub = runtime.block_on(mail::stub::start("hub"));
        let dir = tempfile::tempdir().unwrap();
        let password = dir.path().join("smtp-password");
        std::fs::write(&password, "mail-pass\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&password, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let config_path = dir.path().join("hub.json");
        let config = |email: serde_json::Value| {
            std::fs::write(
                &config_path,
                serde_json::json!({
                    "listen": "127.0.0.1:9",
                    "public_url": "https://hub.example.com",
                    "auth": { "mode": "hub", "rp_id": "hub.example.com", "email": email },
                    "brokers": [],
                    "grants": []
                })
                .to_string(),
            )
            .unwrap();
        };
        config(serde_json::json!({
            "smtp_url": stub.url,
            "password_file": password.to_string_lossy(),
            "from": "Devagotchi <sign-in@hub.example.com>"
        }));
        manage(Command::CheckMail {
            config: config_path.clone(),
            to: None,
        })
        .unwrap();
        assert!(
            stub.received.lock().unwrap().is_empty(),
            "check sends nothing"
        );
        manage(Command::CheckMail {
            config: config_path.clone(),
            to: Some("you@example.com".into()),
        })
        .unwrap();
        let received = stub.received.lock().unwrap().clone();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].to, "you@example.com");
        assert!(
            received[0].data.contains("mail check"),
            "{}",
            received[0].data
        );
        assert!(
            !received[0].data.contains("mail-pass"),
            "{}",
            received[0].data
        );
        // A variable the config names that is not set is reported first, by name.
        config(serde_json::json!({
            "smtp_url": stub.url,
            "password_env": "DEVAGOTCHI_TEST_SMTP_PASSWORD_THAT_IS_NOT_SET",
            "from": "Devagotchi <sign-in@hub.example.com>"
        }));
        let error = manage(Command::CheckMail {
            config: config_path.clone(),
            to: None,
        })
        .unwrap_err();
        assert!(
            error
                .message
                .contains("DEVAGOTCHI_TEST_SMTP_PASSWORD_THAT_IS_NOT_SET is not set"),
            "{error}"
        );
        // A config without mail is told so.
        config(serde_json::Value::Null);
        let error = manage(Command::CheckMail {
            config: config_path,
            to: None,
        })
        .unwrap_err();
        assert!(error.message.contains("no mail server"), "{error}");
    }

    #[test]
    fn board_import_is_preview_first_and_requires_explicit_target_and_absolute_source() {
        let tenant = "t-00000000-0000-4000-8000-000000000001";
        let args = [
            "import-board",
            "--state-dir",
            STATE,
            "--config",
            CONFIG,
            "--source",
            STATE,
            "--tenant",
            tenant,
        ];
        let parsed = parse_args(args.map(OsString::from)).unwrap();
        assert!(matches!(parsed, Command::ImportBoard { apply: false, .. }));
        let mut apply: Vec<_> = args.into_iter().map(OsString::from).collect();
        apply.push("--apply".into());
        assert!(matches!(
            parse_args(apply).unwrap(),
            Command::ImportBoard { apply: true, .. }
        ));
        let mut relative = args;
        relative[6] = "relative.sqlite3";
        assert!(parse_args(relative.map(OsString::from)).is_err());
        let mut missing_tenant = args.to_vec();
        missing_tenant.truncate(7);
        assert!(parse_args(missing_tenant.into_iter().map(OsString::from)).is_err());
        let mut bad_tenant = args;
        bad_tenant[8] = "account-is-not-a-tenant";
        assert!(parse_args(bad_tenant.map(OsString::from)).is_err());
        for extra in [
            vec!["--apply", "--apply"],
            vec!["--source", STATE],
            vec!["--unknown", "value"],
        ] {
            let arguments = args.into_iter().chain(extra).map(OsString::from);
            assert!(parse_args(arguments).is_err());
        }
    }

    #[test]
    fn device_commands_take_a_state_dir_and_a_device() {
        let enroll =
            parse_args(["enroll", "--state-dir", STATE, "--device", "gus"].map(OsString::from))
                .unwrap();
        assert_eq!(
            enroll,
            Command::Enroll {
                state: PathBuf::from(STATE),
                device: "gus".into()
            }
        );
        assert!(matches!(
            parse_args(["devices", "--state-dir", STATE].map(OsString::from)).unwrap(),
            Command::Devices { .. }
        ));
        assert!(parse_args(["enroll", "--state-dir", STATE].map(OsString::from)).is_err());
        assert!(
            parse_args(["revoke", "--state-dir", STATE, "--config", CONFIG].map(OsString::from))
                .is_err()
        );
    }

    #[test]
    fn owner_takes_a_state_dir_a_config_and_maybe_a_name() {
        let owner = |extra: &[&str]| {
            let mut args = vec!["owner", "--state-dir", STATE, "--config", CONFIG];
            args.extend_from_slice(extra);
            parse_args(args.into_iter().map(OsString::from))
        };
        assert_eq!(
            owner(&[]).unwrap(),
            Command::Owner {
                state: STATE.into(),
                config: CONFIG.into(),
                name: None,
                replace: false
            }
        );
        assert!(matches!(
            owner(&["--name", "Jesse"]).unwrap(),
            Command::Owner { name: Some(n), .. } if n == "Jesse"
        ));
        assert!(matches!(
            owner(&["--replace"]).unwrap(),
            Command::Owner { replace: true, .. }
        ));
        assert!(owner(&["--replace", "--replace"]).is_err());
        assert!(owner(&["--device", "x"]).is_err());
        assert!(
            parse_args(["owner", "--state-dir", STATE].map(OsString::from)).is_err(),
            "the config says where the link points"
        );
    }

    #[test]
    fn the_owner_command_makes_the_owner_once_and_only_in_hub_mode() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let config = dir.path().join("hub.json");
        let write = |auth: serde_json::Value, owner: Option<&str>| {
            let mut value = serde_json::json!({
                "listen": "127.0.0.1:1",
                "public_url": "http://127.0.0.1:8787",
                "auth": auth,
                "brokers": [],
                "grants": []
            });
            if let Some(owner) = owner {
                value["owner"] = owner.into();
            }
            std::fs::write(&config, value.to_string()).unwrap();
        };
        let run = |name: Option<&str>| {
            manage(Command::Owner {
                state: state.clone(),
                config: config.clone(),
                name: name.map(str::to_string),
                replace: false,
            })
        };
        let tokens = dir.path().join("tokens.json");
        std::fs::write(&tokens, "{}").unwrap();
        write(
            serde_json::json!({ "mode": "dev", "tokens_file": tokens }),
            None,
        );
        assert_eq!(run(None).unwrap_err().code, "invalid_cli");

        // The Texas case: the config's owner id becomes the owner's account.
        write(
            serde_json::json!({ "mode": "hub" }),
            Some("395290028818825218"),
        );
        run(Some("Jesse")).unwrap();
        run(None).unwrap();
        let owner = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let url = store::sqlite_url(&state.join("accounts.sqlite3"));
                let accounts = accounts::Accounts::open(store::Store::open(&url).await.unwrap())
                    .await
                    .unwrap();
                accounts.owner().await.unwrap().unwrap()
            });
        assert_eq!(
            (owner.id.as_str(), owner.name.as_str()),
            ("395290028818825218", "Jesse")
        );
        // Each run is on the audit chain, without the link.
        let audit = Audit::open(&state.join("audit.sqlite3")).unwrap();
        assert!(audit.verify().unwrap().ok);

        // A config naming someone else than the hub's owner is refused.
        write(
            serde_json::json!({ "mode": "hub" }),
            Some("acct_aaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        assert_eq!(run(None).unwrap_err().code, "owner_mismatch");
    }

    #[test]
    fn connect_nomen_needs_both_ids_and_absolute_paths() {
        let connect = |extra: &[&str]| {
            let mut args = vec![
                "connect-nomen",
                "--state-dir",
                STATE,
                "--config",
                CONFIG,
                "--client-id",
                "7",
            ];
            args.extend_from_slice(extra);
            parse_args(args.into_iter().map(OsString::from))
        };
        assert_eq!(
            connect(&["--project", "5"]).unwrap(),
            Command::ConnectNomen(connect::Connect {
                state: STATE.into(),
                config: CONFIG.into(),
                issuer: account::DEFAULT_ISSUER.into(),
                client_id: "7".into(),
                project: "5".into(),
            })
        );
        assert!(matches!(
            connect(&["--project", "5", "--issuer", "https://id.example"]).unwrap(),
            Command::ConnectNomen(c) if c.issuer == "https://id.example"
        ));
        assert!(connect(&[]).is_err(), "no project");
        assert!(connect(&["--project", "5", "--device", "x"]).is_err());
        assert!(
            parse_args(
                [
                    "connect-nomen",
                    "--state-dir",
                    "rel",
                    "--config",
                    CONFIG,
                    "--client-id",
                    "7",
                    "--project",
                    "5"
                ]
                .map(OsString::from)
            )
            .is_err()
        );
    }

    #[test]
    fn setup_code_reports_the_dg_account_and_the_claim_url() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hub.json");
        let subject = "395290028818825218";
        std::fs::write(
            &config_path,
            serde_json::json!({
                "listen": "127.0.0.1:9",
                "public_url": "https://app.devagotchi.com",
                "auth": {
                    "mode": "devagotchi",
                    "legacy": { "issuer": "https://id.shippin.cloud" }
                },
                "owner": subject,
                "brokers": [],
                "grants": []
            })
            .to_string(),
        )
        .unwrap();
        let lines = setup_code_lines(dir.path(), &config_path).unwrap();
        let owner = lines
            .iter()
            .find_map(|line| line.strip_prefix("owner "))
            .unwrap();
        assert!(owner.starts_with("dg_"), "{lines:?}");
        assert_ne!(owner, subject);
        assert!(
            lines
                .iter()
                .any(|line| line == "claim https://app.devagotchi.com/claim"),
            "{lines:?}"
        );
        let again = setup_code_lines(dir.path(), &config_path).unwrap();
        assert_eq!(again[0], format!("owner {owner}"));

        // A hub in auth.mode hub has its own claim; setup-code leaves its state alone.
        let hub_state = dir.path().join("hub-mode");
        std::fs::write(
            &config_path,
            serde_json::json!({
                "listen": "127.0.0.1:9",
                "public_url": "https://app.devagotchi.com",
                "auth": { "mode": "hub" },
                "brokers": [],
                "grants": []
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            setup_code_lines(&hub_state, &config_path).unwrap_err().code,
            "invalid_cli"
        );
        assert!(!hub_state.join("identities.sqlite3").exists());
    }

    fn dg(nibble: char) -> String {
        format!("dg_{}", nibble.to_string().repeat(32))
    }

    fn access_for(authority: &oauth::Authority, account: &str) -> String {
        let started = authority.start_device("devagotchi-cli").unwrap();
        authority
            .approve_device(started["user_code"].as_str().unwrap(), account)
            .unwrap();
        authority
            .token(&oauth::TokenRequest {
                grant_type: "urn:ietf:params:oauth:grant-type:device_code".into(),
                code: None,
                redirect_uri: None,
                client_id: Some("devagotchi-cli".into()),
                code_verifier: None,
                refresh_token: None,
                device_code: Some(started["device_code"].as_str().unwrap().into()),
            })
            .unwrap()
            .access_token
    }

    /// Owner and an operator grant can read and change provisional limits. A stranger
    /// cannot. Out-of-range values are refused. A saved value is what the next account
    /// and the expiry warning use, and it survives a new open of the state directory.
    #[tokio::test]
    async fn admin_settings_are_authorized_bounded_audited_and_live() {
        let owner = dg('a');
        let operator = dg('b');
        let stranger = dg('c');
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let socket = "/tmp/devagotchi-admin-settings.sock";
        #[cfg(windows)]
        let socket = r"\\.\pipe\devagotchi-admin-settings";
        let config = Arc::new(
            config::HubConfig::parse(
                &serde_json::json!({
                    "listen": "127.0.0.1:1",
                    "public_url": "http://127.0.0.1:9",
                    "auth": { "mode": "devagotchi" },
                    "owner": owner,
                    "brokers": [{ "name": "lab", "socket": socket }],
                    "grants": [{ "subject": operator, "operator": true }]
                })
                .to_string(),
            )
            .unwrap(),
        );
        let devices =
            Arc::new(devices::Devices::open(&dir.path().join("devices.sqlite3")).unwrap());
        let audit = Arc::new(Audit::open(&dir.path().join("audit.sqlite3")).unwrap());
        let links = Arc::new(link::Links::new(config.clone(), devices, audit.clone()));
        let authority = Arc::new(oauth::Authority::open(dir.path(), "http://127.0.0.1:9").unwrap());
        let owner_token = access_for(&authority, &owner);
        let operator_token = access_for(&authority, &operator);
        let stranger_token = access_for(&authority, &stranger);
        let state = AppState {
            verifier: Arc::new(auth::Verifier::Devagotchi(authority.clone())),
            hub: hub::Hub {
                boards: Arc::new(agenthub::mcp::Boards::from_config(&config, dir.path())),
                config,
                audit: audit.clone(),
                links,
                push: None,
                downloads: None,
                analytics: None,
                workflows: None,
                offers: None,
                routed: Default::default(),
            },
            issue_store: Arc::new(
                crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled(),
            ),
            web: None,
            passkeys: None,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/api/v1/admin/settings",
            listener.local_addr().unwrap()
        );
        let app = Router::new()
            .route(
                "/api/v1/admin/settings",
                get(admin_settings_get).put(admin_settings_put),
            )
            .with_state(state);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = reqwest::Client::new();

        let call =
            |method: reqwest::Method, token: Option<&str>, body: Option<serde_json::Value>| {
                let http = http.clone();
                let url = url.clone();
                let token = token.map(str::to_string);
                async move {
                    let mut request = http.request(method, &url);
                    if let Some(token) = token {
                        request = request.bearer_auth(token);
                    }
                    if let Some(body) = body {
                        request = request.json(&body);
                    }
                    let response = request.send().await.unwrap();
                    let status = response.status().as_u16();
                    let json: serde_json::Value =
                        response.json().await.unwrap_or(serde_json::Value::Null);
                    (status, json)
                }
            };

        let (status, _) = call(reqwest::Method::GET, None, None).await;
        assert_eq!(status, 401);
        let (status, _) = call(reqwest::Method::GET, Some("not-a-token"), None).await;
        assert_eq!(status, 401);
        let (status, body) = call(reqwest::Method::GET, Some(&stranger_token), None).await;
        assert_eq!(status, 403, "{body}");
        let (status, body) = call(reqwest::Method::GET, Some(&owner_token), None).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["provisional"]["ttl_days"], 14);
        assert_eq!(body["provisional"]["starts_per_hour"], 8);
        assert_eq!(body["provisional"]["unclaimed_cap"], 500);

        for (field, value) in [
            ("ttl_days", 0),
            ("ttl_days", 91),
            ("starts_per_hour", 0),
            ("unclaimed_cap", 1_000_001),
        ] {
            let (status, body) = call(
                reqwest::Method::PUT,
                Some(&owner_token),
                Some(serde_json::json!({ "provisional": { field: value } })),
            )
            .await;
            assert_eq!(status, 400, "{field}={value}: {body}");
            assert_eq!(body["error"], "invalid_settings");
        }
        let (status, _) = call(
            reqwest::Method::PUT,
            Some(&owner_token),
            Some(serde_json::json!({ "provisional": {} })),
        )
        .await;
        assert_eq!(status, 400);
        let (status, body) = call(reqwest::Method::GET, Some(&operator_token), None).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["provisional"]["ttl_days"], 14,
            "a rejected write leaves the live value"
        );

        let (status, body) = call(
            reqwest::Method::PUT,
            Some(&operator_token),
            Some(serde_json::json!({
                "provisional": { "ttl_days": 1, "starts_per_hour": 2, "unclaimed_cap": 10 }
            })),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["provisional"]["ttl_days"], 1);

        let first = authority
            .agent_start("devagotchi-cli", "claude", "203.0.113.40")
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let left = first.provisional_until.saturating_sub(now);
        assert!((86_390..=86_400).contains(&left), "left={left}");
        authority
            .agent_start("devagotchi-cli", "claude", "203.0.113.40")
            .unwrap();
        assert_eq!(
            authority
                .agent_start("devagotchi-cli", "claude", "203.0.113.40")
                .unwrap_err()
                .code,
            "rate_limited"
        );
        let (status, _) = call(
            reqwest::Method::PUT,
            Some(&operator_token),
            Some(serde_json::json!({ "provisional": { "unclaimed_cap": 2 } })),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(
            authority
                .agent_start("devagotchi-cli", "claude", "203.0.113.41")
                .unwrap_err()
                .code,
            "provisional_quota"
        );
        let before = authority.verify_access(&first.access_token).unwrap();
        assert!(!before.provisional_warning);
        let (status, _) = call(
            reqwest::Method::PUT,
            Some(&operator_token),
            Some(serde_json::json!({ "provisional": { "ttl_days": 90 } })),
        )
        .await;
        assert_eq!(status, 200);
        let after = authority.verify_access(&first.access_token).unwrap();
        assert!(after.provisional_warning);
        assert_eq!(after.provisional_until, before.provisional_until);

        let rows = audit.recorded();
        assert!(
            rows.iter().any(|(subject, tool, outcome)| {
                subject == &stranger && tool == "admin.settings" && outcome == "denied"
            }),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|(_, tool, outcome)| tool == "admin.settings"
                    && outcome.starts_with("rejected")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|(_, tool, outcome)| {
                tool == "admin.settings"
                    && outcome.contains("ttl_days=1")
                    && outcome.contains("starts_per_hour=2")
                    && outcome.contains("unclaimed_cap=10")
            }),
            "{rows:?}"
        );
        assert!(audit.verify().unwrap().ok);

        let reloaded = oauth::Authority::open(dir.path(), "http://127.0.0.1:9").unwrap();
        assert_eq!(reloaded.operator_settings().provisional.ttl_days, 90);
        assert_eq!(reloaded.operator_settings().provisional.starts_per_hour, 2);
        assert_eq!(reloaded.operator_settings().provisional.unclaimed_cap, 2);
        reloaded.apply_config_limits(&config::ProvisionalPolicy::default());
        assert_eq!(
            reloaded.operator_settings().provisional.ttl_days,
            90,
            "a saved row wins over hub.json"
        );
    }

    /// What `run` builds for an `auth` block, short of listening: the accounts and passkeys
    /// of `auth.mode hub`, and the verifier (the authority of `auth.mode devagotchi`).
    async fn state_for(
        dir: &std::path::Path,
        auth: serde_json::Value,
        owner: Option<&str>,
    ) -> AppState {
        let mut value = serde_json::json!({
            "listen": "127.0.0.1:1",
            "public_url": "http://127.0.0.1:9",
            "auth": auth,
            "brokers": [],
            "grants": []
        });
        if let Some(owner) = owner {
            value["owner"] = owner.into();
        }
        let config = Arc::new(HubConfig::parse(&value.to_string()).unwrap());
        prepare_state_dir(dir).unwrap();
        let accounts = if config.has_accounts() {
            let url = store::sqlite_url(&dir.join("accounts.sqlite3"));
            Some(Arc::new(
                accounts::Accounts::open(store::Store::open(&url).await.unwrap())
                    .await
                    .unwrap()
                    .with_tokens(config.token_config()),
            ))
        } else {
            None
        };
        let passkeys = accounts
            .clone()
            .and_then(|accounts| web::HubSignIn::new(config.clone(), accounts).map(Arc::new));
        let devices = Arc::new(devices_at(dir).unwrap());
        let audit = Arc::new(Audit::open(&dir.join("audit.sqlite3")).unwrap());
        let links = Arc::new(Links::new(config.clone(), devices, audit.clone()));
        AppState {
            verifier: Arc::new(
                Verifier::from_hub_config(&config, dir, accounts)
                    .await
                    .unwrap(),
            ),
            hub: Hub {
                boards: Arc::new(agenthub::mcp::Boards::from_config(&config, dir)),
                config,
                audit,
                links,
                push: None,
                downloads: None,
                analytics: None,
                workflows: None,
                offers: None,
                routed: Default::default(),
            },
            issue_store: Arc::new(
                crate::agenthub::issue_lifecycle::IssueLifecycleStore::disabled(),
            ),
            web: None,
            passkeys,
        }
    }

    /// Both account systems mount `/claim` and `/.well-known/oauth-authorization-server`,
    /// and axum panics on a path mounted twice when the router is built. Each mode mounts
    /// its own: the hub's (32b, 16c) in every mode but devagotchi, the authority's
    /// (`oauth.rs`) in devagotchi mode only, with its admin settings and `/healthz` `auth`.
    #[tokio::test]
    async fn app_builds_in_every_auth_mode() {
        let dir = tempfile::tempdir().unwrap();
        let private = |name: &str, text: String| -> String {
            let path = dir.path().join(name);
            std::fs::write(&path, text).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            path.to_string_lossy().into_owned()
        };
        let tokens = private(
            "tokens.json",
            serde_json::json!({ "dev-token-0123456789": "jesse" }).to_string(),
        );
        let key = test_keys::rsa();
        let key_file = private(
            "nomen-key.json",
            serde_json::json!({
                "type": "application", "keyId": "k", "key": key.pem, "appId": "1", "clientId": "c"
            })
            .to_string(),
        );
        let nomen = serde_json::json!({
            "issuer": "https://id.example.test",
            "audiences": ["project"],
            "clients": ["client"],
            "key_file": key_file
        });
        let mut nomen_mode = nomen.clone();
        nomen_mode["mode"] = "nomen".into();
        let cases = [
            (
                "dev",
                serde_json::json!({ "mode": "dev", "tokens_file": tokens }),
                None,
            ),
            ("nomen", nomen_mode, Some("300000000000000001")),
            ("hub", serde_json::json!({ "mode": "hub" }), None),
            (
                "hub-nomen",
                serde_json::json!({ "mode": "hub", "nomen": nomen }),
                None,
            ),
            (
                "devagotchi",
                serde_json::json!({ "mode": "devagotchi" }),
                None,
            ),
        ];
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        for (mode, auth, owner) in cases {
            let state = state_for(&dir.path().join(mode), auth, owner).await;
            let devagotchi = mode == "devagotchi";
            let hub_mode = mode.starts_with("hub");
            assert_eq!(state.verifier.authority().is_some(), devagotchi, "{mode}");
            assert_eq!(state.passkeys.is_some(), hub_mode, "{mode}");
            // Building the router is the check that matters: a path mounted twice panics.
            let app = app(state);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .unwrap()
            });

            let health = http.get(format!("{base}/healthz")).send().await.unwrap();
            assert_eq!(health.status(), StatusCode::OK, "{mode}");
            let health: serde_json::Value = health.json().await.unwrap();
            assert_eq!(health.get("auth").is_some(), devagotchi, "{mode}: {health}");

            let metadata = http
                .get(format!("{base}/.well-known/oauth-authorization-server"))
                .send()
                .await
                .unwrap();
            let status = metadata.status();
            let metadata: serde_json::Value = metadata.json().await.unwrap();
            let token_endpoint = metadata["token_endpoint"].as_str().unwrap_or_default();
            if hub_mode {
                assert_eq!(status, StatusCode::OK, "{mode}: {metadata}");
                assert_eq!(token_endpoint, "http://127.0.0.1:9/oauth/token", "{mode}");
            } else if devagotchi {
                assert_eq!(status, StatusCode::OK, "{mode}: {metadata}");
                assert_eq!(token_endpoint, "http://127.0.0.1:9/token", "{mode}");
            } else {
                assert_eq!(status, StatusCode::NOT_FOUND, "{mode}: {metadata}");
            }

            let claim = http.get(format!("{base}/claim")).send().await.unwrap();
            let claim_status = claim.status();
            let claim = claim.text().await.unwrap();
            if hub_mode {
                assert_eq!(claim_status, StatusCode::OK, "{mode}");
                assert!(claim.contains("Claim this hub"), "{mode}: {claim}");
            } else if devagotchi {
                assert_eq!(claim_status, StatusCode::OK, "{mode}");
                assert!(!claim.contains("Claim this hub"), "{mode}: {claim}");
            }

            // The hub's own token endpoint answers only in hub mode (elsewhere it is mounted
            // and says not_found, as before); devagotchi mode has `/token` and no `/oauth/*`.
            let token = http
                .post(format!("{base}/oauth/token"))
                .form(&[("grant_type", "authorization_code")])
                .send()
                .await
                .unwrap();
            assert_eq!(
                token.status() != StatusCode::NOT_FOUND,
                hub_mode,
                "{mode}: /oauth/token {}",
                token.status()
            );
            let token = http
                .post(format!("{base}/token"))
                .form(&[("grant_type", "authorization_code")])
                .send()
                .await
                .unwrap();
            assert_eq!(
                token.status() != StatusCode::NOT_FOUND,
                devagotchi,
                "{mode}: /token {}",
                token.status()
            );
            let settings = http
                .get(format!("{base}/api/v1/admin/settings"))
                .send()
                .await
                .unwrap();
            let expected = if devagotchi {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::NOT_FOUND
            };
            assert_eq!(settings.status(), expected, "{mode}: admin settings");
        }
    }
}
