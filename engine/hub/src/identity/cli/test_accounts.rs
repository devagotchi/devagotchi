//! `devagotchi hub test-accounts`: two logins to develop and test with, run on the hub's own
//! host. The admin is the hub's owner (made if there is none yet) with a password login; the
//! user is a separate, ordinary account. Each run gives both new passwords, printed once to
//! stdout and nowhere else, so running it again is how they are rotated. The addresses are at
//! `devagotchi.test` unless given: `.test` is reserved (RFC 2606), so no mail can reach them.

use std::{ffi::OsString, path::PathBuf};

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::{
    cli::Command,
    config::{HubConfig, read as read_config},
    identity::accounts::{Account, Accounts},
    identity::accounts_at,
    machines,
    ops::audit::{Audit, operator_event},
};

pub(crate) const ADMIN_EMAIL: &str = "admin@devagotchi.test";
pub(crate) const USER_EMAIL: &str = "user@devagotchi.test";

/// `hub test-accounts`: where the hub keeps its accounts, and the two login addresses.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TestAccounts {
    pub(crate) state: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) admin_email: String,
    pub(crate) user_email: String,
}

fn usage() -> BridgeError {
    BridgeError::new(
        "invalid_cli",
        "test-accounts needs --state-dir ABS_PATH --config ABS_JSON [--admin-email ADDRESS] [--user-email ADDRESS]",
    )
}

pub(crate) fn parse(args: Vec<OsString>) -> BridgeResult<Command> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return Ok(Command::Help);
    }
    let (mut state, mut config, mut admin, mut user) = (None, None, None, None);
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(usage)?;
        match flag.to_str() {
            Some("--state-dir") if state.is_none() => state = Some(PathBuf::from(value)),
            Some("--config") if config.is_none() => config = Some(PathBuf::from(value)),
            Some("--admin-email") if admin.is_none() => admin = value.into_string().ok(),
            Some("--user-email") if user.is_none() => user = value.into_string().ok(),
            _ => return Err(usage()),
        }
    }
    let email = |given: Option<String>, default: &str| {
        let email = given.unwrap_or_else(|| default.to_string());
        crate::identity::accounts::email::normalize_email(&email).ok_or_else(usage)
    };
    let (admin_email, user_email) = (email(admin, ADMIN_EMAIL)?, email(user, USER_EMAIL)?);
    match (state, config) {
        (Some(state), Some(config))
            if state.is_absolute() && config.is_absolute() && admin_email != user_email =>
        {
            Ok(Command::TestAccounts(TestAccounts {
                state,
                config,
                admin_email,
                user_email,
            }))
        }
        _ => Err(usage()),
    }
}

/// A password of 16 random characters (80 bits) in four groups of four, from 32 lowercase
/// letters and digits with no `l`, `o`, `0` or `1` to misread.
pub(crate) fn password(random: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let chars: Vec<char> = random
        .iter()
        .map(|byte| ALPHABET[usize::from(byte % 32)] as char)
        .collect();
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// The two accounts, with the passwords they were given.
pub(crate) struct Made {
    pub(crate) admin: Account,
    pub(crate) user: Account,
}

/// Give the owner (made if there is none) a password login at `admin_email`, and the
/// ordinary account at `user_email` (made if there is none) its password. The owner keeps
/// their sessions; the test user's end, since its password changed.
pub(crate) async fn make(
    accounts: &Accounts,
    config: &HubConfig,
    options: &TestAccounts,
    admin_password: &str,
    user_password: &str,
) -> BridgeResult<Made> {
    let admin = accounts
        .ensure_owner(config.owner_id(), "Admin", machines::random::<16>())
        .await?;
    accounts
        .set_password_for_email(&admin.id, &options.admin_email, admin_password)
        .await?;
    let user = match accounts.password_account_of(&options.user_email).await? {
        Some(id) if id == admin.id => {
            return Err(BridgeError::new(
                "invalid_cli",
                format!(
                    "{} signs in to the hub's owner; give the test user another address",
                    options.user_email
                ),
            ));
        }
        Some(id) => {
            accounts
                .reset_password_for_email(&id, &options.user_email, user_password)
                .await?;
            accounts
                .get(&id)
                .await?
                .ok_or_else(|| BridgeError::new("no_account", id))?
        }
        // A hosted hub makes anyone's account at sign-up, in a tenant of its own. A one-owner
        // hub makes no account at sign-up but the owner's, so the test user is made as a
        // person the owner adds: an ordinary account on the owner's hub.
        None if config.multi_tenant() => {
            accounts
                .sign_up_password(&options.user_email, user_password)
                .await?
        }
        None => {
            let id = crate::identity::accounts::new_account_id(machines::random::<16>());
            let user = accounts.create(&id, "Test user", false).await?;
            accounts
                .set_password_for_email(&user.id, &options.user_email, user_password)
                .await?;
            user
        }
    };
    let audit = Audit::new(accounts.store().clone());
    operator_event(&audit, "account.test_accounts", &admin.id, "", "ok").await?;
    Ok(Made { admin, user })
}

pub(crate) fn run(options: TestAccounts) -> BridgeResult<()> {
    let config = read_config(&options.config)?;
    if !config.has_accounts() {
        return Err(BridgeError::new(
            "invalid_cli",
            "test-accounts is for a hub with its own accounts (auth.mode hub)",
        ));
    }
    let (admin_password, user_password) = (
        password(machines::random::<16>()),
        password(machines::random::<16>()),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| BridgeError::new("runtime", error.to_string()))?;
    let made = runtime.block_on(async {
        let accounts = accounts_at(&options.state, &config).await?;
        make(
            &accounts,
            &config,
            &options,
            &admin_password,
            &user_password,
        )
        .await
    })?;
    // The passwords are the secrets this prints: to stdout only, once.
    println!("admin  {}  {admin_password}", options.admin_email);
    println!("user   {}  {user_password}", options.user_email);
    eprintln!(
        "Sign in at {}/auth/login.\nadmin is the hub's owner ({}): it sees /admin. user ({}) is an ordinary account.\nRun this again for new passwords; the old ones stop working, and the test user's sessions end.",
        config.public_url.trim_end_matches('/'),
        made.admin.id,
        made.user.id,
    );
    Ok(())
}
