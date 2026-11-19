//! The link's two verbs: `enroll` binds this machine's key to its name on a hub once, and
//! `run` holds the link open, reconnecting with a capped, jittered backoff.

use std::{env, path::PathBuf, process::ExitCode, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult, MACHINE_KIND_CONTAINER};

use crate::{
    Command,
    session::{self, Ended, Mode},
    state::{self, LinkConfig},
    update,
};

/// Where `enroll` reads the one-time code from first.
const CODE_ENV: &str = "DEVAGOTCHI_ENROLL_CODE";
/// [`MACHINE_KIND_CONTAINER`] on a machine that runs in a container (the hub's compose file
/// sets it): the hub learns it from the hello, and this link refuses to update itself.
const KIND_ENV: &str = "DEVAGOTCHI_MACHINE_KIND";
/// The first pause after a lost connection, and the longest one: it doubles in between.
const FIRST_PAUSE: Duration = Duration::from_secs(1);
const LONGEST_PAUSE: Duration = Duration::from_secs(60);
/// A link that held this long starts its backoff over.
const HELD: Duration = Duration::from_secs(60);

pub(crate) async fn dispatch(command: Command, build: &str) -> ExitCode {
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
        Command::Help => unreachable!("cli answers --help before dispatching"),
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
    let container = env::var(KIND_ENV).is_ok_and(|kind| kind == MACHINE_KIND_CONTAINER);
    let updater = update::Updater::new(&config, &state_dir, build, container);
    let mut delay = FIRST_PAUSE;
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
                if started.elapsed() > HELD {
                    delay = FIRST_PAUSE;
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
                delay = (delay * 2).min(LONGEST_PAUSE);
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
mod tests;
