//! `devagotchi hub …`: the hub's command line. It reads the verbs, runs the operator's
//! commands on the hub's host (each recorded on the audit chain), and hands `serve` to
//! [`crate::serve`]. The commands themselves live with their domains: `machines::cli`,
//! `identity::cli`, `identity::connect`.

use std::{ffi::OsString, path::PathBuf, process::ExitCode};

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::{
    config, extensions, identity, machines, notify, ops, ops::audit::Audit, site, store, web,
};

pub(crate) const USAGE: &str = "Usage:\n  devagotchi hub serve --state-dir ABS_PATH --config ABS_JSON\n  devagotchi hub enroll --state-dir ABS_PATH [--config ABS_JSON] --device NAME\n  devagotchi hub revoke --state-dir ABS_PATH [--config ABS_JSON] --device NAME\n  devagotchi hub devices --state-dir ABS_PATH [--config ABS_JSON]\n  devagotchi hub owner --state-dir ABS_PATH --config ABS_JSON [--name NAME] [--replace]\n  devagotchi hub set-password --state-dir ABS_PATH --config ABS_JSON --account ID --email ADDRESS --password-stdin\n  devagotchi hub test-accounts --state-dir ABS_PATH --config ABS_JSON [--admin-email ADDRESS] [--user-email ADDRESS]\n  devagotchi hub connect-nomen --state-dir ABS_PATH --config ABS_JSON --issuer URL --client-id ID --project ID\n  devagotchi hub check-site ABS_DIR\n  devagotchi hub check-mail --config ABS_JSON [--to ADDRESS]\n  devagotchi hub analytics-resend --config ABS_JSON\n  devagotchi board <command> …            the agents' board in this project (board --help)\n\nserve starts the bastion: an authenticated Streamable HTTP MCP endpoint at /mcp that routes\nrun and the operation, file, process, settings and read_url tools to machines by name. It answers\nwhoami, get_recent_tool_calls, get_usage_stats and the lexicon's tools (dictionary_get,\nlexicon_resolve, lexicon_propose, lexicon_review, lexicon_handoff) itself, and serves /healthz,\nRFC 9728 protected-resource metadata, and /link where machines' `devagotchi link` connects out.\nWith a `web` section, people sign in with Shippin at /account to see their machines;\n/api/v1/me returns the same view for a session or a bearer token. Put it behind the edge for TLS.\n\nenroll prints a one-time code (15 minutes) that binds one machine's key to NAME;\nrevoke cuts that machine off within seconds; devices lists enrolled machines. All three\nuse the hub's Postgres: the storage --config names, or DEVAGOTCHI_HUB_STORAGE_URL.\n\nowner (auth.mode hub) prints a one-time link (15 minutes) that makes the owner's passkey:\nthe first time it makes the owner's account, with the config's owner id if it names one;\nagain, it adds a passkey to the owner and ends the owner's other sessions; with --replace the\nlink also removes every other passkey of the owner (to take the account back).\n\nset-password (auth.mode hub) sets or replaces the local password of an existing account ID, for\nthe login address --email names (not a mailbox it proves). Run it on the hub's host with the hub's\nconfig and state, and pipe one UTF-8 password (8 to 4096 bytes) to stdin with --password-stdin:\nnever an argument, never typed into a terminal. An address another account uses is refused. The\naccount's browser sessions end; its passkeys, sign-ins and AI connections stay. No account is made.\n\ntest-accounts (auth.mode hub) gives two logins to develop and test with: admin, the hub's owner\n(made if there is none), and user, an ordinary account (made if there is none), at\nadmin@devagotchi.test and user@devagotchi.test unless named (.test reaches no mailbox). Each run\nprints two new passwords to stdout, once; the old ones stop working and the user's sessions end.\n\nconnect-nomen joins this hub to a Nomen with its owner's approval: it shows a code to confirm\non any device through the devagotchi-cli app (--client-id) of the hub's Nomen project (--project),\nthen adds the hub's own apps to that project (the key Nomen makes is written to the state\ndirectory, owner-only, and nowhere else), sets auth, owner and web in the config, and keeps a\ncopy of the old one. --issuer is that Nomen's https URL; there is no built-in one.\n\ncheck-site reads and checks a site directory (crates/hub/README.md, \"The site\") as serve would, and\nsays what is wrong and where; serve follows its site.dir and serves a change once it checks out.\n\ncheck-mail (auth.mode hub) connects to auth.email's mail server as serve would, negotiates TLS and\nsigns in as its user, sending nothing; with --to it also sends one email that says this hub's mail\nworks, to read in that mailbox. It prints the server's refusal and never the password; the running\nhub reports the same in /healthz under mail.\n\nanalytics-resend marks every board event in the hub's Postgres to be sent to ClickHouse again,\nas after a ClickHouse that lost its data: the running hub sends them, and ClickHouse keeps one\nrow per event. /healthz reports the export under analytics.";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Serve {
        state: PathBuf,
        config: PathBuf,
    },
    Enroll {
        state: PathBuf,
        config: Option<PathBuf>,
        device: String,
    },
    Revoke {
        state: PathBuf,
        config: Option<PathBuf>,
        device: String,
    },
    Devices {
        state: PathBuf,
        config: Option<PathBuf>,
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
    ConnectNomen(identity::connect::Connect),
    SetPassword(identity::cli::SetPassword),
    /// An admin (the owner) and a user login to develop and test with, new passwords each run.
    TestAccounts(identity::cli::TestAccounts),
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
}

pub(crate) fn parse_args(args: impl IntoIterator<Item = OsString>) -> BridgeResult<Command> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
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
        return identity::cli::parse_owner(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "set-password") {
        return identity::cli::parse_set_password(args.split_off(1));
    }
    if args.first().is_some_and(|a| a == "test-accounts") {
        return identity::cli::parse_test_accounts(args.split_off(1));
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
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
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
        (Some(_), Some(config), _) if !config.is_absolute() => Err(BridgeError::new(
            "invalid_cli",
            "--config must be an absolute path",
        )),
        (Some("enroll"), config, Some(device)) => Ok(Command::Enroll {
            state,
            config,
            device,
        }),
        (Some("revoke"), config, Some(device)) => Ok(Command::Revoke {
            state,
            config,
            device,
        }),
        (Some("devices"), config, None) => Ok(Command::Devices { state, config }),
        _ => Err(BridgeError::new(
            "invalid_cli",
            "see --help for the accepted forms",
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
            Ok(Command::ConnectNomen(identity::connect::Connect {
                state,
                config,
                issuer: issuer.ok_or_else(|| {
                    BridgeError::new(
                        "invalid_cli",
                        "connect-nomen needs --issuer URL (the Nomen's https address)",
                    )
                })?,
                client_id,
                project,
            }))
        }
        _ => Err(BridgeError::new(
            "invalid_cli",
            "connect-nomen needs --state-dir ABS_PATH --config ABS_JSON --issuer URL --client-id ID --project ID",
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
                if notify::mail::address_of(&address).is_none() {
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

pub(crate) fn manage(command: Command) -> BridgeResult<()> {
    match command {
        Command::Enroll {
            state,
            config,
            device,
        } => machines::cli::enroll(&state, config.as_deref(), &device)?,
        Command::Revoke {
            state,
            config,
            device,
        } => machines::cli::revoke(&state, config.as_deref(), &device)?,
        Command::Devices { state, config } => machines::cli::devices(&state, config.as_deref())?,
        Command::Owner {
            state,
            config,
            name,
            replace,
        } => {
            let config = config::read(&config)?;
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
                let accounts = identity::accounts_at(&state, &config).await?;
                let owner = accounts
                    .ensure_owner(
                        config.owner_id(),
                        name.as_deref().unwrap_or("Owner"),
                        machines::random::<16>(),
                    )
                    .await?;
                let token = if replace {
                    accounts
                        .issue_replacing_link(identity::accounts::LinkPurpose::Owner, &owner.id)
                        .await?
                } else {
                    accounts
                        .issue_link(identity::accounts::LinkPurpose::Owner, &owner.id)
                        .await?
                };
                // The link itself is never audited or logged; only that one was issued.
                let audit = Audit::new(accounts.store().clone());
                ops::audit::operator_event(&audit, "account.owner_link", &owner.id, "ok").await?;
                Ok::<_, BridgeError>((owner, token))
            })?;
            // The link is the one secret this prints; it goes to stdout only, once.
            println!("{}/owner#{token}", config.public_url.trim_end_matches('/'));
            eprintln!(
                "One-time link for the owner ({}), valid {} minutes; any earlier unused one no longer works.\nOpen it to make a passkey or set a local email and password. Using it ends the owner's other sessions{}.",
                owner.id,
                identity::accounts::LINK_TTL_SECS / 60,
                if replace {
                    " and removes every other passkey of the owner"
                } else {
                    " (add --replace to also remove the owner's other passkeys)"
                }
            );
        }
        Command::SetPassword(options) => identity::cli::set_password(options)?,
        Command::TestAccounts(options) => identity::cli::test_accounts(options)?,
        Command::ConnectNomen(connect) => {
            config::prepare_state_dir(&connect.state)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            runtime.block_on(async {
                identity::connect::run(&connect, &mut std::io::stderr()).await?;
                let store = store::for_command(&connect.state, Some(&connect.config)).await?;
                ops::audit::operator_event(&Audit::new(store), "nomen.connect", "", "ok").await
            })?;
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
            let config = config::read(&config)?;
            let url = store::postgres_url(config.storage_url())?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
            runtime.block_on(async {
                let schema = store::schema_for(config.storage_schema());
                let store = store::open_in(&url, schema.as_deref()).await?;
                let marked = ops::warehouse::resend(&store).await?;
                println!(
                    "marked {marked} board events to send again; the running hub sends them to ClickHouse, which keeps one row per event"
                );
                Ok::<_, BridgeError>(())
            })?;
        }
        Command::CheckMail { config, to } => {
            let config = config::read(&config)?;
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
pub(crate) fn main(args: Vec<OsString>) -> ExitCode {
    let (state_dir, config_path) = match parse_args(args) {
        Ok(Command::Serve { state, config }) => (state, config),
        Ok(Command::Help) => {
            println!("{USAGE}");
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
    match runtime.block_on(crate::serve::run(
        state_dir,
        config_path,
        extensions::Extensions::core(),
    )) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("devagotchi hub: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
