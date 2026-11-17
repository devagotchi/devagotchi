//! `devagotchi login`, `logout` and `whoami`: the arguments, the prompt for a hub, and what each prints.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use super::{
    DEFAULT_CLIENT_ID, Standing, Stored, client,
    file::{current, read, remove, write},
    hub::{HUB_CLIENT, hub_device_login, hub_revoke},
    issuer::Issuer,
    unix_now,
};

pub const USAGE: &str = "Usage:
  devagotchi login [--hub URL]                         sign in through the hub (device code)
  devagotchi login --issuer URL [--client-id ID]       sign in directly with an upstream issuer
  devagotchi login --local                              run without an account on this machine
  devagotchi logout                                     revoke the tokens and forget the account
  devagotchi whoami [--json]                            who this machine is signed in as

login shows a code and a URL; open the URL on any device, check or enter the code, approve
it there (on a hub, signed in to your account), and the sign-in lands here (OAuth device
authorization, RFC 8628). --hub is the hub's https URL. When it is omitted, login uses
DEVAGOTCHI_HUB if that is set (@HUB@), otherwise the hub saved by the last login here,
otherwise it asks.
Tokens are kept in account.json under the config directory, readable by you alone. A
hosted hub requires an account; a local or self-hosted run does not, and --local records
that choice.

With --issuer, sign in at that issuer directly. The client id is devagotchi unless
--client-id names the public client that issuer registered for this command (device
authorization and refresh tokens). The hub's own device flow uses a different built-in
client, devagotchi-cli.";

/// The help, naming the hub a previous login or `DEVAGOTCHI_HUB` would use.
pub(super) fn help() -> String {
    let hub = client::configured_hub().unwrap_or_else(|| "(none saved)".to_string());
    USAGE.replace("@HUB@", &hub)
}

/// `devagotchi login|logout|whoami`, with the account file's path resolved by the caller.
pub fn cli(verb: &str, args: Vec<std::ffi::OsString>, path: PathBuf) -> std::process::ExitCode {
    use std::process::ExitCode;

    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", help());
        return ExitCode::SUCCESS;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("devagotchi {verb}: runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = match verb {
        "login" => runtime.block_on(login(&args, &path)),
        "logout" => runtime.block_on(logout(&args, &path)),
        "whoami" => runtime.block_on(whoami(&args, &path)),
        _ => Err((64, format!("unknown command {verb}"))),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("devagotchi {verb}: {message}");
            if code == 64 {
                eprintln!("\n{}", help());
            }
            ExitCode::from(code)
        }
    }
}

pub(super) type Outcome = Result<(), (u8, String)>;

pub(super) fn usage(message: impl Into<String>) -> (u8, String) {
    (64, message.into())
}

/// Ask for a hub URL on a terminal. A pipe or a service gets the flag it should have passed.
pub(super) fn prompt_hub() -> Result<String, (u8, String)> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Err(usage(
            "pass --hub https://hub.example.com (or set DEVAGOTCHI_HUB)",
        ));
    }
    eprint!("Hub URL (https://…): ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| (1, error.to_string()))?;
    let hub = line.trim().to_string();
    if hub.is_empty() {
        return Err(usage("a hub URL is required"));
    }
    Ok(hub)
}

pub(super) async fn login(args: &[String], path: &Path) -> Outcome {
    // The hub `devagotchi login` talks to. Nothing is assumed: a flag, then
    // DEVAGOTCHI_HUB, then the hub the last login saved, then a prompt on a terminal.
    let mut hub = client::configured_hub().or_else(|| client::saved_hub(path));
    let mut issuer: Option<String> = None;
    let mut client_id = DEFAULT_CLIENT_ID.to_string();
    let mut local = false;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--local" => local = true,
            "--hub" => {
                hub = Some(
                    iter.next()
                        .ok_or_else(|| usage("--hub needs a URL"))?
                        .clone(),
                );
            }
            "--issuer" => {
                issuer = Some(
                    iter.next()
                        .ok_or_else(|| usage("--issuer needs a URL"))?
                        .clone(),
                );
            }
            "--client-id" => {
                client_id = iter
                    .next()
                    .ok_or_else(|| usage("--client-id needs a value"))?
                    .clone();
            }
            other => return Err(usage(format!("unknown option {other}"))),
        }
    }
    if local {
        if let Some(Stored::Account(account)) = read(path).map_err(|e| (1, e))? {
            return Err((
                1,
                format!(
                    "signed in as {}; `devagotchi logout` first, then `login --local`",
                    account.display_name()
                ),
            ));
        }
        write(
            path,
            &Stored::Local {
                chosen_at: unix_now(),
            },
        )
        .map_err(|e| (1, e))?;
        eprintln!(
            "devagotchi: this machine runs without an account ({}). `devagotchi login --hub URL` changes that.",
            path.display()
        );
        return Ok(());
    }
    let mut stderr = std::io::stderr();
    let account = if let Some(issuer) = issuer {
        let nomen = Issuer::new(&issuer, &client_id).map_err(usage)?;
        nomen
            .login(&mut stderr)
            .await
            .map_err(|why| (1, why.to_string()))?
    } else {
        let hub = match hub {
            Some(hub) => hub,
            None => prompt_hub()?,
        };
        hub_device_login(&hub, &mut stderr)
            .await
            .map_err(|why| (1, why.to_string()))?
    };
    write(path, &Stored::Account(account.clone())).map_err(|e| (1, e))?;
    eprintln!(
        "devagotchi: signed in as {} at {} (account {}); tokens in {}",
        account.display_name(),
        account.issuer,
        account.subject,
        path.display()
    );
    Ok(())
}

pub(super) async fn logout(args: &[String], path: &Path) -> Outcome {
    if !args.is_empty() {
        return Err(usage(format!("unknown option {}", args[0])));
    }
    match read(path).map_err(|e| (1, e))? {
        Some(Stored::Account(account)) if account.client_id == HUB_CLIENT => {
            match hub_revoke(&account).await {
                Ok(()) => eprintln!("devagotchi: tokens revoked at {}", account.issuer),
                Err(why) => eprintln!(
                    "devagotchi: could not revoke the tokens at {} ({why}); they expire on their own",
                    account.issuer
                ),
            }
            remove(path).map_err(|e| (1, e))?;
            eprintln!("devagotchi: signed out ({} removed)", path.display());
        }
        Some(Stored::Account(account)) => {
            match Issuer::new(&account.issuer, &account.client_id) {
                Ok(nomen) => match nomen.revoke(&account).await {
                    Ok(()) => eprintln!("devagotchi: tokens revoked at {}", account.issuer),
                    Err(why) => eprintln!(
                        "devagotchi: could not revoke the tokens at {} ({why}); they expire on their own",
                        account.issuer
                    ),
                },
                Err(why) => eprintln!("devagotchi: not revoking: {why}"),
            }
            remove(path).map_err(|e| (1, e))?;
            eprintln!("devagotchi: signed out ({} removed)", path.display());
        }
        Some(Stored::Local { .. }) => {
            remove(path).map_err(|e| (1, e))?;
            eprintln!("devagotchi: the local choice is forgotten; `devagotchi login` signs in");
        }
        None => eprintln!("devagotchi: not signed in"),
    }
    Ok(())
}

pub(super) async fn whoami(args: &[String], path: &Path) -> Outcome {
    let json = match args {
        [] => false,
        [flag] if flag == "--json" => true,
        [other, ..] => return Err(usage(format!("unknown option {other}"))),
    };
    let stored = current(path).await.map_err(|e| (1, e))?;
    let now = unix_now();
    if json {
        let value = match &stored {
            None => serde_json::json!({ "signed_in": false, "local": false }),
            Some(Stored::Local { chosen_at }) => {
                serde_json::json!({ "signed_in": false, "local": true, "chosen_at": chosen_at })
            }
            Some(Stored::Account(account)) => serde_json::json!({
                "signed_in": true,
                "local": false,
                "issuer": account.issuer,
                "subject": account.subject,
                "username": account.username,
                "email": account.email,
                "expires_at": account.expires_at,
                "standing": standing_name(account.standing(now)),
            }),
        };
        println!("{value}");
        return Ok(());
    }
    match stored {
        None => println!("not signed in (devagotchi login, or devagotchi login --local)"),
        Some(Stored::Local { .. }) => {
            println!("no account: this machine runs locally (devagotchi login signs in)")
        }
        Some(Stored::Account(account)) => {
            let standing = match account.standing(now) {
                Standing::Fresh => "signed in".to_string(),
                Standing::Refresh => "signed in (token due for refresh)".to_string(),
                Standing::Offline { grace_ends_at } => format!(
                    "signed in (offline: token expired, grace for {} more days)",
                    (grace_ends_at - now) / 86_400
                ),
                Standing::Expired => "signed in, but expired (devagotchi login again)".to_string(),
            };
            println!(
                "{standing}: {} at {} (account {})",
                account.display_name(),
                account.issuer,
                account.subject
            );
        }
    }
    Ok(())
}

pub(super) fn standing_name(standing: Standing) -> &'static str {
    match standing {
        Standing::Fresh => "fresh",
        Standing::Refresh => "refresh",
        Standing::Offline { .. } => "offline",
        Standing::Expired => "expired",
    }
}
