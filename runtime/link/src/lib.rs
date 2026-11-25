//! `devagotchi link`: lets a Devagotchi hub reach the broker on this machine without
//! opening any port here. It dials out to the hub, proves this machine's key, and hands
//! each routed request to the local broker over private IPC. Jobs stay with the broker.
//!
//! This library is the link's whole behaviour; `devagotchi link` is its name. This file is
//! the command line; `run/` enrolls and holds the link, `session/` is one connection, and
//! `update/` updates this machine when the hub asks.

mod run;
mod session;
mod state;
mod update;

use std::{ffi::OsString, path::PathBuf, process::ExitCode};

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::state::LinkConfig;

pub const USAGE: &str = "Usage:\n  devagotchi link enroll --state-dir ABS_PATH --hub URL --device NAME --broker ENDPOINT\n  devagotchi link run --state-dir ABS_PATH\n  devagotchi link --help\n\nenroll binds this machine's key to NAME on the hub with a one-time code from the hub\noperator (devagotchi hub enroll), then exits. The code is read from the\nDEVAGOTCHI_ENROLL_CODE environment variable, or else from the first line of stdin, never\nfrom the command line where other users could see it. Run again for the same hub and\ndevice with the key already here, enroll does nothing and succeeds. run holds the link open and\nreconnects on its own; start it at login. URL is the hub's https public URL. ENDPOINT is\nthe local broker's ready line (Unix socket path or named pipe). In a container, set\nDEVAGOTCHI_MACHINE_KIND=container: the machine then updates with its image, never by itself.";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Enroll { state: PathBuf, config: LinkConfig },
    Run { state: PathBuf },
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> BridgeResult<Command> {
    let mut args: Vec<String> = args
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.as_slice() == ["--help"] {
        return Ok(Command::Help);
    }
    let invalid = || BridgeError::new("invalid_cli", "see --help for the accepted forms");
    if args.is_empty() {
        return Err(invalid());
    }
    let verb = args.remove(0);
    let mut flags = std::collections::BTreeMap::new();
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let value = iter.next().ok_or_else(invalid)?;
        if flags.insert(flag, value).is_some() {
            return Err(invalid());
        }
    }
    let mut take = |name: &str| flags.remove(name);
    let state = take("--state-dir").map(PathBuf::from).ok_or_else(invalid)?;
    if !state.is_absolute() {
        return Err(BridgeError::new(
            "invalid_cli",
            "--state-dir must be absolute",
        ));
    }
    let command = match verb.as_str() {
        "enroll" => {
            let config = LinkConfig {
                hub: take("--hub").ok_or_else(invalid)?,
                device: take("--device").ok_or_else(invalid)?,
                broker: take("--broker").ok_or_else(invalid)?,
            };
            config.validate()?;
            Command::Enroll { state, config }
        }
        "run" => Command::Run { state },
        _ => return Err(invalid()),
    };
    if !flags.is_empty() {
        return Err(invalid());
    }
    Ok(command)
}

/// The link's command line: `enroll …`, `run …`, or `--help`. Exit 2 means the hub
/// refused this device; a supervisor should not hammer it. `build` is the name of the
/// build this binary is (`devagotchi --version`), which the link tells the hub.
pub fn cli(args: Vec<OsString>, build: &str) -> ExitCode {
    let command = match parse_args(args) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(command) => command,
        Err(error) => {
            eprintln!("devagotchi link: {error}\n\n{USAGE}");
            return ExitCode::from(64);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("devagotchi link: runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(run::dispatch(command, build))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    const STATE: &str = "/s";
    #[cfg(windows)]
    const STATE: &str = r"C:\s";
    #[cfg(unix)]
    const BROKER: &str = "/run/b.sock";
    #[cfg(windows)]
    const BROKER: &str = r"\\.\pipe\devagotchi-broker-x";

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn enroll_needs_every_flag_and_valid_values() {
        let base = [
            "enroll",
            "--state-dir",
            STATE,
            "--hub",
            "https://hub.example",
            "--device",
            "gus",
            "--broker",
            BROKER,
        ];
        let ok = parse_args(args(&base)).unwrap();
        assert!(matches!(ok, Command::Enroll { ref config, .. } if config.device == "gus"));
        // The code is never a command-line argument, where other users could read it.
        let mut with_code = base.to_vec();
        with_code.extend(["--code", "dvg-abc"]);
        assert!(parse_args(args(&with_code)).is_err());
        assert!(parse_args(args(&base[..5])).is_err());
        let mut plain_http = base.to_vec();
        plain_http[4] = "http://hub.example";
        assert!(parse_args(args(&plain_http)).is_err());
    }

    #[test]
    fn run_takes_only_a_state_dir() {
        assert_eq!(
            parse_args(args(&["run", "--state-dir", STATE])).unwrap(),
            Command::Run {
                state: PathBuf::from(STATE)
            }
        );
        assert!(parse_args(args(&["run", "--state-dir", "relative"])).is_err());
        assert!(parse_args(args(&["run", "--state-dir", STATE, "--hub", "x"])).is_err());
        assert_eq!(parse_args(args(&["--help"])).unwrap(), Command::Help);
    }
}
