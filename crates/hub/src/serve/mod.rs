//! `devagotchi hub serve`: the running hub. [`run`] reads the config, opens the store and
//! every domain the config turns on, prints the ready line and listens; [`app`] composes the
//! domains' routes into one router, with `/mcp` behind the bearer check.

use std::{env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{Router, middleware, routing::get};
use devagotchi_contracts::{BridgeError, BridgeResult};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::{
    AppState, agenthub, config, connections, extensions,
    identity::{self, auth::Verifier, bearer},
    machines::{self, devices::Devices, link::Links},
    mcp::Hub,
    notify, offers, ops,
    ops::audit::Audit,
    site, store, tenancy, web, workflows,
};

pub(crate) async fn run(
    state_dir: PathBuf,
    config_path: PathBuf,
    extensions: extensions::Extensions,
) -> BridgeResult<()> {
    let mut config = config::read(&config_path)?;
    config.extensions = extensions;
    let url = store::postgres_url(config.storage_url())?;
    let schema = store::schema_for(config.storage_schema());
    let board_store = store::open_in(&url, schema.as_deref()).await?;
    let config = Arc::new(config);
    // The running hub follows its site directory: a change is served without a restart,
    // and one that does not check out is refused while the site it had stays.
    if let Some(dir) = &config.site.dir {
        site::watch(config.site_cell(), PathBuf::from(dir), site::WATCH_EVERY);
    }
    // The state directory holds the hub's keys (VAPID, session, settings) and nothing else.
    config::prepare_state_dir(&state_dir)?;
    let devices = Arc::new(Devices::new(board_store.clone()));
    let audit = Arc::new(Audit::new(board_store.clone()));
    // A hub with its own accounts knows its owner from them, and refuses to start when its
    // config names someone else.
    let accounts = if config.has_accounts() {
        let accounts = Arc::new(identity::configured_accounts(board_store.clone(), &config).await?);
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
    // from the browser (`identity/accounts/claim/mod.rs`); its code is issued once the hub listens, below.
    // A hosted hub (`tenancy.mode multi`) is never claimed.
    let claimable = match (&passkeys, &accounts) {
        (Some(_), Some(accounts)) if config.claim_allowed() => Some(accounts.clone()),
        _ => None,
    };
    let push = match (&config.web, &passkeys) {
        (None, None) => None,
        _ => Some(Arc::new(notify::push::Push::open(
            &state_dir,
            &config.public_url,
            devices.clone(),
        )?)),
    };
    // A hosted hub counts visits and its accounts' milestones for its operator, in its store.
    let analytics = match &accounts {
        Some(accounts) if config.counts_visits() => Some(ops::analytics::Analytics::start(
            accounts.store().clone(),
            config.analytics,
            &config.public_url,
        )),
        _ => None,
    };
    // A downloads directory that does not check out is logged and left out: the hub still
    // serves everything else, and /healthz says why /join is off.
    let downloads = match &config.downloads {
        Some(dir) => match machines::downloads::Downloads::open(std::path::Path::new(dir)) {
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
    // Outside services the account's agents call through the hub (`connections.rs`): keys
    // sealed with the same key file as the settings page's secrets.
    let connections = match &accounts {
        Some(accounts) => Some(Arc::new(
            connections::Connections::new(
                accounts.clone(),
                identity::Sealer::open(&state_dir)
                    .map_err(|why| BridgeError::new("invalid_config", why))?,
            )
            .map_err(|why| BridgeError::new("invalid_config", why))?,
        )),
        None => None,
    };
    let verifier = Arc::new(Verifier::from_hub_config(&config, &board_store, accounts).await?);
    let web = web::WebLogin::for_hub(&config, &state_dir, &board_store)?.map(Arc::new);
    let listen: SocketAddr = config
        .listen
        .parse()
        .map_err(|_| BridgeError::new("invalid_config", "listen"))?;
    let warehouse_store = board_store.clone();
    let signals_store = board_store.clone();
    let boards = Arc::new(agenthub::mcp::Boards::new(&config, board_store)?);
    if let Some(preferences) = boards.preferences() {
        links.set_preferences(preferences.clone());
    }
    // Board events to ClickHouse for statistics (ADR 0001): reported in /healthz, never
    // required for the boards themselves.
    if let Some(clickhouse) = &config.clickhouse {
        let password = std::env::var(config::CLICKHOUSE_PASSWORD_ENV).ok();
        let warehouse = Arc::new(ops::warehouse::Warehouse::new(
            clickhouse,
            warehouse_store,
            password,
        )?);
        tokio::spawn(warehouse.clone().run());
        boards.set_warehouse(Some(warehouse));
        eprintln!(
            "devagotchi hub: exporting board events to ClickHouse at {} (database {})",
            clickhouse.url, clickhouse.database
        );
    } else if let Some(hub) = &passkeys {
        // Else the ClickHouse the owner set on the settings page, if any. One it cannot
        // start is said here and on the page; the boards work without it.
        match ops::warehouse::start_stored(&boards, hub.accounts(), hub.sealer(), warehouse_store)
            .await
        {
            Ok(true) => eprintln!(
                "devagotchi hub: exporting board events to the ClickHouse set on the settings page"
            ),
            Ok(false) => {}
            Err(error) => eprintln!("devagotchi hub: ClickHouse from the settings page: {error}"),
        }
    }
    // Signals to the operator's sinks (`notify/signals/`, docs/signals.md): each on its own task,
    // reported in /healthz, never holding back the hub or another sink.
    let mut sinks = Vec::new();
    for config in &config.signals.sinks {
        let sink = Arc::new(notify::signals::Sink::new(
            config,
            signals_store.clone(),
            audit.clone(),
            |name| env::var(name).ok(),
        )?);
        tokio::spawn(sink.clone().run());
        eprintln!(
            "devagotchi hub: signals ({}) to sink {} at {}",
            notify::signals::SCHEMA,
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
            connections,
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
        state
            .hub
            .audit
            .record(&ops::audit::AuditEntry {
                subject: "hub",
                via: "start",
                tool: "account.claim_code",
                broker: "",
                operation_id: "",
                outcome: "ok",
            })
            .await?;
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
pub(crate) fn app(state: AppState) -> Router {
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
    let router = Router::new()
        .route("/healthz", get(ops::health::healthz))
        .merge(machines::routes())
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
                    bearer::require_bearer,
                )),
        )
        // Board statistics from ClickHouse (`warehouse`), for anyone who reads the board.
        .merge(
            ops::warehouse::routes().layer(middleware::from_fn_with_state(
                state.clone(),
                tenancy::orgs::api::bearer_or_session,
            )),
        )
        // Organisations, members, teams, project roles and invites (`orgs::api`).
        .merge(
            tenancy::orgs::api::routes()
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    tenancy::orgs::api::bearer_or_session,
                )),
        )
        .merge(machines::join::routes())
        .route(
            "/.well-known/oauth-protected-resource",
            get(bearer::protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(bearer::protected_resource_mcp),
        )
        .nest_service(
            "/mcp",
            Router::new()
                .fallback_service(mcp)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    bearer::require_bearer,
                )),
        )
        .merge(
            // The bar runs first: it marks the operator's requests, which are not counted.
            web::router(true)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    web::counted::count,
                ))
                .layer(middleware::from_fn_with_state(state.clone(), web::bar)),
        );
    let routes = state.hub.config.extensions.routes.clone();
    routes.mount(router.with_state(state))
}

#[cfg(test)]
mod tests;
