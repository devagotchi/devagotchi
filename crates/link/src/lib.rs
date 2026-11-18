//! `devagotchi link`: lets a Devagotchi hub reach the broker on this machine without
//! opening any port here. It dials out to the hub, proves this machine's key, and hands
//! each routed request to the local broker over private IPC. Jobs stay with the broker.

//! This library is the link's whole behaviour; `devagotchi link` is its name.

mod handoff;
mod idle;
mod session;
mod state;
mod update;

use std::{env, ffi::OsString, path::PathBuf, process::ExitCode, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult};

use crate::{
    session::{Ended, Mode},
    state::LinkConfig,
};

pub const USAGE: &str = "Usage:\n  devagotchi link enroll --state-dir ABS_PATH --hub URL --device NAME --broker ENDPOINT\n  devagotchi link run --state-dir ABS_PATH\n  devagotchi link --help\n\nenroll binds this machine's key to NAME on the hub with a one-time code from the hub\noperator (devagotchi hub enroll), then exits. The code is read from the\nDEVAGOTCHI_ENROLL_CODE environment variable, or else from the first line of stdin, never\nfrom the command line where other users could see it. Run again for the same hub and\ndevice with the key already here, enroll does nothing and succeeds. run holds the link open and\nreconnects on its own; start it at login. URL is the hub's https public URL. ENDPOINT is\nthe local broker's ready line (Unix socket path or named pipe). In a container, set\nDEVAGOTCHI_MACHINE_KIND=container: the machine then updates with its image, never by itself.";

/// Where `enroll` reads the one-time code from first.
const CODE_ENV: &str = "DEVAGOTCHI_ENROLL_CODE";
/// `container` on a machine that runs in a container (the hub's compose file sets it): the
/// hub learns it from the hello, and this link refuses to update itself.
const KIND_ENV: &str = "DEVAGOTCHI_MACHINE_KIND";

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
    runtime.block_on(dispatch(command, build))
}

async fn dispatch(command: Command, build: &str) -> ExitCode {
    let result = match command {
        Command::Enroll { state, config } => match already_enrolled(&state, &config) {
            Ok(true) => Ok(()),
            Ok(false) => match enrollment_code() {
                Ok(code) => enroll(state, config, code).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        },
        Command::Run { state } => run(state, build).await,
        Command::Help => unreachable!(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("devagotchi link: {error}");
            // 2 = the hub refused this device; a supervisor should not hammer it.
            ExitCode::from(2)
        }
    }
}

/// The one-time code: from the environment, else the first line of stdin.
fn enrollment_code() -> BridgeResult<String> {
    let code = match env::var(CODE_ENV) {
        Ok(code) => code,
        Err(_) => {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| BridgeError::new("invalid_cli", e.to_string()))?;
            line
        }
    };
    let code = code.trim().to_string();
    if code.is_empty() || code.len() > 128 {
        return Err(BridgeError::new(
            "invalid_cli",
            format!("no enrollment code: set {CODE_ENV} or pipe the code on stdin"),
        ));
    }
    Ok(code)
}

/// Whether this state directory already holds a key enrolled for the same hub and device,
/// with no network call and no code: then a second `enroll` (a compose file brought up
/// again) changes nothing but the broker endpoint, and succeeds.
fn already_enrolled(state_dir: &std::path::Path, config: &LinkConfig) -> BridgeResult<bool> {
    state::prepare(state_dir)?;
    let Ok(saved) = state::load_config(state_dir) else {
        return Ok(false);
    };
    if saved.hub != config.hub || saved.device != config.device || !state::has_key(state_dir) {
        return Ok(false);
    }
    state::device_key(state_dir)?;
    if saved.broker != config.broker {
        state::save_config(state_dir, config)?;
    }
    eprintln!(
        "devagotchi link: {} is already enrolled on {}; nothing to do",
        config.device, config.hub
    );
    Ok(true)
}

async fn enroll(state_dir: PathBuf, config: LinkConfig, code: String) -> BridgeResult<()> {
    state::prepare(&state_dir)?;
    let key = state::device_key(&state_dir)?;
    match session::connect(&config, &key, Mode::Enroll { code: &code }).await {
        Ended::Enrolled => {
            state::save_config(&state_dir, &config)?;
            eprintln!(
                "devagotchi link enrolled: {} on {}. Start it with: devagotchi link run --state-dir {}",
                config.device,
                config.hub,
                state_dir.display()
            );
            Ok(())
        }
        Ended::Refused(error) => Err(error),
        Ended::Retry(why) => Err(BridgeError::new(
            "enroll_failed",
            format!("{why}; the code is still unused if the hub never answered"),
        )),
    }
}

async fn run(state_dir: PathBuf, build: &str) -> BridgeResult<()> {
    state::prepare(&state_dir)?;
    let config = state::load_config(&state_dir)?;
    let key = state::device_key(&state_dir)?;
    let container = env::var(KIND_ENV).is_ok_and(|kind| kind == "container");
    let updater = update::Updater::new(&config, &state_dir, build, container);
    let mut delay = Duration::from_secs(1);
    loop {
        let started = tokio::time::Instant::now();
        let ended = tokio::select! {
            ended = session::connect(&config, &key, Mode::Serve { updater: &updater, state_dir: &state_dir }) => ended,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("devagotchi link stopped");
                return Ok(());
            }
        };
        match ended {
            Ended::Refused(error) => return Err(error),
            Ended::Enrolled => unreachable!("serve mode never enrolls"),
            Ended::Retry(why) => {
                // A link that held for a while starts its backoff over.
                if started.elapsed() > Duration::from_secs(60) {
                    delay = Duration::from_secs(1);
                }
                let pause = jitter(delay);
                eprintln!(
                    "devagotchi link: {why}; reconnecting in {:.1}s",
                    pause.as_secs_f32()
                );
                tokio::select! {
                    _ = tokio::time::sleep(pause) => {}
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("devagotchi link stopped");
                        return Ok(());
                    }
                }
                delay = (delay * 2).min(Duration::from_secs(60));
            }
        }
    }
}

/// Up to 25% random spread so a fleet that lost the hub together does not return together.
fn jitter(delay: Duration) -> Duration {
    let mut byte = [0u8; 1];
    let _ = getrandom::fill(&mut byte);
    delay + delay.mul_f32(f32::from(byte[0]) / 255.0 * 0.25)
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod update_tests;

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

    /// A compose file brought up a second time runs `enroll` again with a code already
    /// spent: a key enrolled for the same hub and device makes that a success, decided before
    /// any code is read or any connection made (this hub's port answers nothing).
    #[tokio::test]
    async fn re_enrolling_with_an_existing_key_exits_0_without_a_network_call() {
        let parent = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let parent_path = std::fs::canonicalize(parent.path()).unwrap();
        #[cfg(windows)]
        let parent_path = parent.path().to_path_buf();
        let dir = parent_path.join("link");
        let config = LinkConfig {
            hub: "http://127.0.0.1:9".into(),
            device: "docked".into(),
            broker: BROKER.into(),
        };
        // Nothing enrolled yet: the code and the hub are needed.
        assert!(!already_enrolled(&dir, &config).unwrap());
        // A key and this very config on disk, as the first enroll leaves them.
        state::device_key(&dir).unwrap();
        state::save_config(&dir, &config).unwrap();
        assert!(already_enrolled(&dir, &config).unwrap());
        let started = std::time::Instant::now();
        let exit = dispatch(
            Command::Enroll {
                state: dir.clone(),
                config: config.clone(),
            },
            "test",
        )
        .await;
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no connection was tried"
        );
        // Another device or hub is a new enrollment, never a silent success.
        for other in [
            LinkConfig {
                device: "other".into(),
                ..config.clone()
            },
            LinkConfig {
                hub: "https://hub.example".into(),
                ..config.clone()
            },
        ] {
            assert!(!already_enrolled(&dir, &other).unwrap());
        }
    }

    #[test]
    fn jitter_stays_within_a_quarter() {
        let base = Duration::from_secs(8);
        for _ in 0..50 {
            let pause = jitter(base);
            assert!(pause >= base && pause <= base.mul_f32(1.25));
        }
    }
}
