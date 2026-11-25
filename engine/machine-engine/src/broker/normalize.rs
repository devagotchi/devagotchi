//! Canonical executable, arguments and cwd, shared by local and granted admission.
use super::*;
impl Broker {
    pub(super) fn normalize(&self, input: &StartInput) -> BridgeResult<(ExecutionSpec, String)> {
        if !(1..=MAX_OPERATION_KEY_BYTES).contains(&input.idempotency_key.len()) {
            return Err(invalid("idempotency_key must contain 1..=128 UTF-8 bytes"));
        }
        if input.args.len() > 128
            || input.args.iter().any(|arg| arg.as_bytes().contains(&0))
            || input.args.iter().map(String::len).sum::<usize>() > 16 * 1024
        {
            return Err(invalid(
                "args must contain at most 128 entries and 16384 total UTF-8 bytes, without NUL bytes",
            ));
        }
        if !(1..=MAX_DEADLINE_MS).contains(&input.deadline_ms) {
            return Err(invalid(format!(
                "deadline_ms must be 1..={MAX_DEADLINE_MS} (a day)"
            )));
        }
        let account = self.shared.config.exec_policy == ExecPolicy::Account;
        let cwd = match &input.cwd {
            Some(cwd) => {
                canonical_existing(cwd, Some(&self.shared.workspace), "cwd", "invalid_request")?
            }
            None => self.shared.workspace.clone(),
        };
        if !cwd.is_dir() {
            return Err(invalid("cwd must be an existing directory"));
        }
        let cwd_text = path_string(&cwd)?;
        // What runs is decided once, here: the program spawned, the arguments, and the
        // sentence the operation's detail and the broker log carry.
        let (executable, canonical_executable, args, launch) = match &input.command {
            Some(command) => {
                if !account {
                    return Err(BridgeError::new(
                        "command_not_allowed",
                        "This machine's broker runs only operator-granted programs with exact arguments: pass executable and args instead of command",
                    ));
                }
                if !input.executable.is_empty() || !input.args.is_empty() {
                    return Err(invalid(
                        "Give either command, or executable with args, not both",
                    ));
                }
                if command.trim().is_empty() || command.len() > 16 * 1024 || command.contains('\0')
                {
                    return Err(invalid(
                        "command must be 1..=16384 UTF-8 bytes without NUL bytes",
                    ));
                }
                let (line, launch) =
                    match launch::line::route(command, &cwd_text, Os::HERE, &launch::Machine)? {
                        Some((line, how)) => (line, Some(how)),
                        None => (command.clone(), None),
                    };
                let (shell, args) = platform::shell_command(&line)?;
                (shell.clone(), shell, args, launch)
            }
            None => {
                let found =
                    launch::resolve(&input.executable, &cwd_text, Os::HERE, &launch::Machine)?;
                let canonical =
                    canonical_existing(&found.program, None, "executable", "invalid_request")?;
                if !account && !self.shared.executables.contains(&canonical) {
                    return Err(BridgeError::new(
                        "executable_not_allowed",
                        format!(
                            "{} has no operator grant on this broker ({})",
                            path_string(&canonical)?,
                            found.how
                        ),
                    ));
                }
                let mut args = found.lead;
                args.extend(input.args.iter().cloned());
                if args.len() > 136 {
                    return Err(invalid("args must contain at most 128 entries"));
                }
                (
                    PathBuf::from(found.program),
                    canonical,
                    args,
                    Some(found.how),
                )
            }
        };
        if !account && !cwd.starts_with(&self.shared.workspace) {
            return Err(BridgeError::new(
                "cwd_outside_workspace",
                "The canonical cwd is outside the configured workspace",
            ));
        }
        if let Some(interactive) = &input.interactive {
            check_size(interactive.cols, interactive.rows)?;
            if !platform::supports_interactive(interactive.mode) {
                return Err(BridgeError::new(
                    "unsupported_platform",
                    "Interactive operations are not available on this machine's platform yet",
                ));
            }
            if input.caller.is_none() {
                return Err(invalid(
                    "An interactive operation needs a caller to own it: start it through devagotchi mcp or a hub",
                ));
            }
        }
        // The request half: what runs, with which arguments, where, for how long, and whether
        // it takes input.
        let bytes = serde_json::to_vec(&(
            &self.shared.config_identity,
            path_string(&canonical_executable)?,
            &args,
            path_string(&cwd)?,
            input.deadline_ms,
            &input.interactive,
        ))
        .map_err(|error| invalid(error.to_string()))?;
        Ok((
            ExecutionSpec {
                executable,
                args,
                cwd,
                deadline_ms: input.deadline_ms,
                account_environment: account,
                interactive: input.interactive.clone(),
                combined_output: input.combined_output && input.interactive.is_none(),
                launch,
            },
            hex::encode(Sha256::digest(bytes)),
        ))
    }
}
