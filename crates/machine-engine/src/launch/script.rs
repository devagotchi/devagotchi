//! A found file to a launch: run it directly when the system runs it as a program, else
//! with the interpreter its `#!` line or its extension names. Never through a file
//! association: on Windows that opens whatever app the extension is associated with (an
//! editor, often) and captures nothing.

use super::{Host, Kind, Launch, Os, on_path};
use crate::execution::platform::git_bash;
use devagotchi_contracts::BridgeError;

pub(super) fn plan(path: &str, found: &str, os: Os, host: &impl Host) -> Result<Launch, BridgeError> {
    match os {
        Os::Unix => unix(path, found, host),
        Os::Windows => windows(path, found, host),
    }
}

fn extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn direct(path: &str, how: String) -> Launch {
    Launch {
        program: path.to_owned(),
        lead: Vec::new(),
        how,
    }
}

fn unix(path: &str, found: &str, host: &impl Host) -> Result<Launch, BridgeError> {
    let executable = matches!(host.kind(path), Kind::File { executable: true, .. });
    let head = host.head(path);
    if let Some(interpreter) = shebang(&head) {
        if executable {
            return Ok(direct(path, format!("{path} ({found}) runs itself: it is executable and starts with #!{interpreter}")));
        }
        let (program, flags) = shebang_program(&interpreter, host)?;
        let mut lead = flags;
        lead.push(path.to_owned());
        return Ok(Launch {
            how: format!("{program} runs {path} ({found}): the file has no execute permission, so its #! line names the interpreter"),
            program,
            lead,
        });
    }
    let image = head.starts_with(b"\x7fELF") || head.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]);
    if executable && image {
        return Ok(direct(path, format!("{path} ({found}) runs directly: an executable program")));
    }
    let extension = extension(path);
    if let Some(names) = unix_interpreters(&extension) {
        let (program, dir) = first_on_path(names, Os::Unix, host).map_err(|error| {
            BridgeError::new(
                "interpreter_not_found",
                format!("{path} is a .{extension} script with no #! line, and no interpreter for it was found: {}", error.message),
            )
        })?;
        return Ok(Launch {
            how: format!("{program} (found on PATH in {dir}) runs {path} ({found}): it ends in .{extension} and has no #! line"),
            program,
            lead: vec![path.to_owned()],
        });
    }
    if executable {
        return Ok(direct(path, format!("{path} ({found}) runs directly: it has execute permission")));
    }
    Err(BridgeError::new(
        "not_executable",
        format!(
            "{path} is not a program: it has no execute permission, no #! line, and its extension names no interpreter (known: .py .sh .bash .js .mjs .cjs .rb .pl .php). Make it executable (chmod +x), or run its interpreter with the file as an argument."
        ),
    ))
}

fn unix_interpreters(extension: &str) -> Option<&'static [&'static str]> {
    Some(match extension {
        "py" => &["python3", "python"],
        "sh" => &["sh"],
        "bash" => &["bash"],
        "js" | "mjs" | "cjs" => &["node"],
        "rb" => &["ruby"],
        "pl" => &["perl"],
        "php" => &["php"],
        _ => return None,
    })
}

/// The interpreter of a `#!` line, without the `#!`.
fn shebang(head: &[u8]) -> Option<String> {
    let rest = head.strip_prefix(b"#!")?;
    let end = rest.iter().position(|&byte| byte == b'\n').unwrap_or(rest.len());
    let line = String::from_utf8_lossy(&rest[..end]).trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// `/usr/bin/env python3 -u` names `python3` on PATH; `/usr/bin/python3 -u` names itself.
fn shebang_program(interpreter: &str, host: &impl Host) -> Result<(String, Vec<String>), BridgeError> {
    let mut words = interpreter.split_whitespace();
    let first = words.next().unwrap_or_default();
    let mut flags: Vec<String> = words.map(str::to_owned).collect();
    if first.ends_with("/env") {
        if flags.first().is_some_and(|flag| flag == "-S") {
            flags.remove(0);
        }
        if flags.is_empty() {
            return Err(BridgeError::new("not_executable", format!("#!{interpreter} names no program after env")));
        }
        let name = flags.remove(0);
        let (program, _) = on_path(&name, Os::Unix, host)?;
        return Ok((program, flags));
    }
    match host.kind(first) {
        Kind::File { .. } => Ok((first.to_owned(), flags)),
        _ => Err(BridgeError::new(
            "interpreter_not_found",
            format!("#!{interpreter} names {first}, which this machine does not have"),
        )),
    }
}

fn first_on_path(names: &[&str], os: Os, host: &impl Host) -> Result<(String, String), BridgeError> {
    let mut last = None;
    for name in names {
        match on_path(name, os, host) {
            Ok(found) => return Ok(found),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| BridgeError::new("program_not_found", "no interpreter named")))
}

fn windows(path: &str, found: &str, host: &impl Host) -> Result<Launch, BridgeError> {
    let extension = extension(path);
    match extension.as_str() {
        "exe" | "com" => Ok(direct(path, format!("{path} ({found}) runs directly: a Windows program"))),
        // The standard library runs a batch file through cmd.exe with its own safe quoting
        // (and refuses arguments it cannot quote safely).
        "bat" | "cmd" => Ok(direct(path, format!("{path} ({found}) runs through cmd.exe: a batch file"))),
        "ps1" => {
            let shell = powershell(host);
            Ok(Launch {
                how: format!("Windows PowerShell ({shell}) runs {path} ({found}) with -File: it ends in .ps1"),
                program: shell,
                lead: ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", path]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            })
        }
        "py" | "pyw" => {
            let (program, how) = windows_python(host).map_err(|error| {
                BridgeError::new("interpreter_not_found", format!("{path} is a Python script and {}", error.message))
            })?;
            Ok(Launch {
                how: format!("{how} runs {path} ({found}): it ends in .{extension}"),
                program,
                lead: vec![path.to_owned()],
            })
        }
        "js" | "mjs" | "cjs" => {
            let (program, dir) = on_path("node", Os::Windows, host).map_err(|error| {
                BridgeError::new("interpreter_not_found", format!("{path} is a JavaScript file and {}", error.message))
            })?;
            Ok(Launch {
                how: format!("node ({program}, found on PATH in {dir}) runs {path} ({found}): it ends in .{extension}"),
                program,
                lead: vec![path.to_owned()],
            })
        }
        "sh" | "bash" => {
            let bash = git_bash::find_bash(|name| host.var(name), |path| matches!(host.kind(path), Kind::File { .. }))
                .ok_or_else(|| BridgeError::new("shell_unavailable", git_bash::NO_GIT_BASH))?;
            Ok(Launch {
                how: format!("Git Bash ({bash}) runs {path} ({found}): it ends in .{extension}"),
                program: bash,
                lead: vec![path.to_owned()],
            })
        }
        _ => Err(BridgeError::new(
            "not_executable",
            format!(
                "{path} is not a program Windows runs: .exe and .com run directly, and .py .pyw .ps1 .bat .cmd .js .mjs .cjs .sh run with their interpreter; this file ends in {}. Run its interpreter with the file as an argument.",
                if extension.is_empty() { "nothing".to_owned() } else { format!(".{extension}") }
            ),
        )),
    }
}

/// Windows PowerShell from System32, never one found on PATH.
pub(crate) fn powershell(host: &impl Host) -> String {
    let root = host.var("SystemRoot").unwrap_or_else(|| "C:\\Windows".to_owned());
    Os::Windows.join(&root, "System32\\WindowsPowerShell\\v1.0\\powershell.exe")
}

/// Python on Windows: the Python launcher (`py.exe`, which honours a script's `#!` line and
/// picks an installed Python), else a real `python.exe` on PATH. The Store alias is never
/// one. `(program, how it was found)`.
pub(crate) fn windows_python(host: &impl Host) -> Result<(String, String), BridgeError> {
    let mut places = Vec::new();
    if let Some(root) = host.var("SystemRoot") {
        places.push(Os::Windows.join(&root, "py.exe"));
    }
    if let Some(local) = host.var("LOCALAPPDATA") {
        places.push(Os::Windows.join(&local, "Programs\\Python\\Launcher\\py.exe"));
    }
    for place in &places {
        if matches!(host.kind(place), Kind::File { size, .. } if size > 0) {
            return Ok((place.clone(), format!("the Python launcher ({place})")));
        }
    }
    if let Ok((program, dir)) = on_path("py", Os::Windows, host) {
        return Ok((program.clone(), format!("the Python launcher ({program}, found on PATH in {dir})")));
    }
    match on_path("python", Os::Windows, host) {
        Ok((program, dir)) => Ok((program.clone(), format!("python ({program}, found on PATH in {dir})"))),
        Err(error) => Err(BridgeError::new(
            "interpreter_not_found",
            format!("this machine has no Python: no Python launcher (py.exe) and {}", lowercase_first(&error.message)),
        )),
    }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}
