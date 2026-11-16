//! The identity domain's operator commands, run on the hub's own host with its config and
//! state: `devagotchi hub owner` (a one-time link for the owner, printed once) and
//! `devagotchi hub set-password` (a password for an existing account, read from stdin). A
//! secret never comes in an argument and never reaches a log or the audit chain.

use std::{
    ffi::OsString,
    io::{IsTerminal, Read},
    path::PathBuf,
};

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::{Command, accounts_at, operator_event, ops::audit::Audit, read_config};

pub(crate) fn parse_owner(args: Vec<OsString>) -> BridgeResult<Command> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(Command::Help);
    }
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

/// `hub set-password`: which account, which login address, and where the hub keeps them.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SetPassword {
    pub(crate) state: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) account: String,
    pub(crate) email: String,
}

pub(crate) fn parse_set_password(args: Vec<OsString>) -> BridgeResult<Command> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(Command::Help);
    }
    let (mut state, mut config, mut account, mut email) = (None, None, None, None);
    let mut stdin = false;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag == "--password-stdin" && !stdin {
            stdin = true;
            continue;
        }
        let value = args.next().ok_or_else(set_password_usage)?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--account") if account.is_none() => account = value.into_string().ok(),
            Some("--email") if email.is_none() => email = value.into_string().ok(),
            _ => return Err(set_password_usage()),
        }
    }
    match (state, config, account, email) {
        (Some(state), Some(config), Some(account), Some(email))
            if stdin
                && state.is_absolute()
                && config.is_absolute()
                && crate::identity::accounts::email::normalize_email(&email).is_some() =>
        {
            Ok(Command::SetPassword(SetPassword {
                state,
                config,
                account,
                email,
            }))
        }
        _ => Err(set_password_usage()),
    }
}

fn set_password_usage() -> BridgeError {
    BridgeError::new(
        "invalid_cli",
        "set-password needs --state-dir ABS_PATH --config ABS_JSON --account ID --email ADDRESS --password-stdin",
    )
}

/// One password from stdin: at most 4096 bytes, one trailing newline (a pipe's or a file's)
/// taken off, spaces kept, nothing else that a second line or a NUL could smuggle in.
fn read_password(reader: impl Read) -> BridgeResult<String> {
    let mut bytes = Vec::new();
    reader
        .take(4099)
        .read_to_end(&mut bytes)
        .map_err(|_| BridgeError::new("password_input", "could not read password from stdin"))?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    if bytes.len() > 4096 || bytes.contains(&b'\n') || bytes.contains(&b'\r') || bytes.contains(&0)
    {
        return Err(BridgeError::new(
            "password_input",
            "supply one password, at most 4096 bytes",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| BridgeError::new("password_input", "password must be UTF-8"))
}

/// Set or replace the password of an existing account and end its browser sessions; its
/// passkeys, sign-ins and AI connections stay. Audited as `account.password_reset`.
pub(crate) fn set_password(options: SetPassword) -> BridgeResult<()> {
    let config = read_config(&options.config)?;
    if !config.has_accounts() {
        return Err(BridgeError::new(
            "invalid_cli",
            "set-password is for a hub with its own accounts (auth.mode hub)",
        ));
    }
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(BridgeError::new(
            "password_input",
            "pipe the password to stdin: typed into a terminal it would echo",
        ));
    }
    set_password_from(options, &config, stdin.lock())
}

/// What `set_password` does once it has the config and a source of the password.
fn set_password_from(
    options: SetPassword,
    config: &crate::config::HubConfig,
    input: impl Read,
) -> BridgeResult<()> {
    let password = read_password(input)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
    runtime.block_on(async {
        let accounts = accounts_at(&options.state, config).await?;
        accounts
            .reset_password_for_email(&options.account, &options.email, &password)
            .await?;
        let audit = Audit::new(accounts.store().clone());
        operator_event(&audit, "account.password_reset", &options.account, "ok").await
    })?;
    println!(
        "Password set for {}; its browser sessions ended.",
        options.account
    );
    Ok(())
}

#[cfg(test)]
mod tests;
