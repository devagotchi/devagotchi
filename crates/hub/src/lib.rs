//! `devagotchi hub`: the bastion. Authenticates every caller through the identity
//! provider, routes typed operations to the broker that owns the target machine,
//! and audits each call on a hash chain. It never admits a job itself. It hosts the kernel's
//! lexicon (`kernel/`). This library is its whole behaviour; `devagotchi hub` and `devagotchi
//! board` are its names; `devagotchi login`, `logout` and `whoami` sign a machine in to one.

// Tests that need the test Postgres skip without one: `or_skip!` (devagotchi-store).
#[cfg(test)]
#[macro_use]
extern crate devagotchi_store;
pub mod agenthub;
mod cli;
mod config;
mod connections;
pub mod extensions;
mod identity;
mod kernel;
pub mod login;
mod machines;
mod mcp;
mod notify;
mod offers;
mod ops;
mod serve;
mod site;
mod store;
mod tenancy;
#[cfg(test)]
mod test_keys;
mod web;
mod workflows;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode, sync::Arc};

use devagotchi_contracts::BridgeResult;

use crate::{identity::auth::Verifier, mcp::Hub};

/// What every route of the composed hub shares: the verifier, the MCP surface with each
/// domain it reaches, and the two browser sign-ins. [`serve::run`] builds it.
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

/// The hub's command line: `serve …`, `enroll …`, `revoke …`, `devices …`, `board …`, or
/// `--help` ([`cli`]).
pub fn cli(args: Vec<OsString>) -> ExitCode {
    cli::main(args)
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
        hub: Some(Arc::new(login::client::AccountHub)),
    };
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    ExitCode::from(agenthub::cli::run(invocation, &mut out, &mut err))
}

/// Start the hub the way `devagotchi hub serve` does, with `extensions` in place of the
/// core defaults. The command line passes [`extensions::Extensions::core`].
pub async fn serve_with(
    state_dir: PathBuf,
    config_path: PathBuf,
    extensions: extensions::Extensions,
) -> BridgeResult<()> {
    serve::run(state_dir, config_path, extensions).await
}
