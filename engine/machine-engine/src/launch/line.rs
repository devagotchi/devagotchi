//! A shell line (`run`, `operation_start` with `command`) whose first word is a script, or
//! Python on a Windows machine whose `python` is only the Store alias, rewritten to name the
//! interpreter, so the shell never hands the script to a file association or a stub.
//! Every other line is left exactly as written. (`.sh` lines on Windows go to Git Bash in
//! `execution::platform`, before this.)

use super::script::{plan, windows_python};
use super::{Host, Kind, Os, on_path};
use crate::execution::platform::git_bash::first_token;
use devagotchi_contracts::BridgeError;

/// The rewritten line and the sentence saying why, or `None` to run the line as written.
pub(crate) fn route(
    line: &str,
    cwd: &str,
    os: Os,
    host: &impl Host,
) -> Result<Option<(String, String)>, BridgeError> {
    let (token, _, rest) = first_token(line);
    if token.is_empty() {
        return Ok(None);
    }
    match os {
        Os::Unix => unix(line, token, cwd, host),
        Os::Windows => windows(line, token, rest, host),
    }
}

fn unix(
    line: &str,
    token: &str,
    cwd: &str,
    host: &impl Host,
) -> Result<Option<(String, String)>, BridgeError> {
    if !token.contains('/') {
        return Ok(None);
    }
    let path = if Os::Unix.is_absolute(token) {
        token.to_owned()
    } else {
        Os::Unix.join(cwd, token.strip_prefix("./").unwrap_or(token))
    };
    // An executable file runs as the shell runs it; a missing one fails there with the
    // shell's own message.
    if !matches!(
        host.kind(&path),
        Kind::File {
            executable: false,
            ..
        }
    ) {
        return Ok(None);
    }
    let launch = plan(&path, "named first on the line", Os::Unix, host)?;
    let mut prefix = vec![sh_quote(&launch.program)];
    // The lead ends with the script itself, which the line already names.
    prefix.extend(
        launch.lead[..launch.lead.len() - 1]
            .iter()
            .map(|flag| sh_quote(flag)),
    );
    Ok(Some((
        format!("{} {}", prefix.join(" "), line.trim_start()),
        launch.how,
    )))
}

fn windows(
    line: &str,
    token: &str,
    rest: &str,
    host: &impl Host,
) -> Result<Option<(String, String)>, BridgeError> {
    let lower = token.to_ascii_lowercase();
    let name = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    if matches!(name, "python" | "python3" | "python.exe" | "python3.exe")
        && !token.contains(['\\', '/'])
    {
        if on_path(name.trim_end_matches(".exe"), Os::Windows, host).is_ok() {
            return Ok(None);
        }
        let (program, how) = windows_python(host).map_err(|error| {
            BridgeError::new(
                "interpreter_not_found",
                format!("the line runs {token}, and {}", error.message),
            )
        })?;
        return Ok(Some((
            format!("& {}{rest}", ps_quote(&program)),
            format!(
                "{how} runs this line's Python: {token} on PATH is missing or only the Microsoft Store alias"
            ),
        )));
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or_default();
    let interpreter = match extension {
        "py" | "pyw" => windows_python(host).map_err(|error| {
            BridgeError::new(
                "interpreter_not_found",
                format!("{token} is a Python script and {}", error.message),
            )
        })?,
        "js" | "mjs" | "cjs" => {
            let (program, dir) = on_path("node", Os::Windows, host).map_err(|error| {
                BridgeError::new(
                    "interpreter_not_found",
                    format!("{token} is a JavaScript file and {}", error.message),
                )
            })?;
            (
                program.clone(),
                format!("node ({program}, found on PATH in {dir})"),
            )
        }
        _ => return Ok(None),
    };
    let (program, how) = interpreter;
    Ok(Some((
        format!("& {} {}", ps_quote(&program), line.trim_start()),
        format!(
            "{how} runs {token}: it ends in .{extension}, and PowerShell would otherwise open it with the file's associated app"
        ),
    )))
}

/// POSIX single quotes: `'` becomes `'\''`.
fn sh_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// PowerShell single quotes: `'` becomes `''`.
fn ps_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "''"))
}
