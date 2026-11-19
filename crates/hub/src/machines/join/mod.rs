//! `/join/<code>`: the one line that adds a machine (increment 29b).
//!
//! "Add a machine" hands a person `curl -fsSL <hub>/join/<code> | sudo sh` (Linux) or
//! `irm <hub>/join/<code>.ps1 | iex` (Windows). The hub answers with a script made for that
//! code: the hub's URL, the machine's name, the build version, the SHA-256 of every file it
//! will download from this hub's `/dist`, and the code itself, which the script hands to the
//! installer on stdin (Linux) or in the environment (Windows), never on a command line.
//! Fetching the script does not spend the code; enrolling does, so a link preview cannot
//! burn it. A used, expired or unknown code gets a script that only says so. Every value
//! written into a script was checked against a strict pattern first, and goes in single
//! quotes, so nothing in it can become shell syntax. Docker gets a compose file instead
//! (`compose.rs`).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{AppState, config::valid_name, machines::downloads::Downloads};

pub mod compose;

const SH: &str = include_str!("join.sh");
const PS1: &str = include_str!("join.ps1");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Sh,
    PowerShell,
}

impl Shell {
    /// How the code was asked for, in the audit.
    fn way(self) -> &'static str {
        match self {
            Shell::Sh => "sh",
            Shell::PowerShell => "powershell",
        }
    }
}

/// `dvg-` and 32 lowercase hex digits, as the hub issues them.
pub fn valid_code(code: &str) -> bool {
    code.len() == 36
        && code.starts_with("dvg-")
        && code[4..]
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A hub URL safe to put in single quotes: https (or loopback http, for tests), host,
/// optional port, nothing else.
fn safe_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    rest.is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
    })
}

/// The script for one live code: `device` is the stored name the machine joins as, `shown`
/// the name `machines` shows its person (`tenancy/machines.rs`), the one the script tells
/// them to use. `None` when a value could not be written safely.
pub fn script(
    shell: Shell,
    hub: &str,
    device: &str,
    shown: &str,
    code: &str,
    downloads: &Downloads,
) -> Option<String> {
    let hub = hub.trim_end_matches('/');
    if !safe_url(hub) || !valid_name(device) || !valid_name(shown) || !valid_code(code) {
        return None;
    }
    let sum = |name: &str| downloads.sha256(name).unwrap_or("").to_string();
    let replacements = [
        ("@HUB@", hub.to_string()),
        (
            "@PROTO@",
            if hub.starts_with("https://") {
                "=https"
            } else {
                "=http"
            }
            .to_string(),
        ),
        ("@DEVICE@", device.to_string()),
        ("@NAME@", shown.to_string()),
        ("@VERSION@", downloads.version.clone()),
        ("@CODE@", code.to_string()),
        ("@SHA_INSTALLER@", sum("install-devagotchi.sh")),
        ("@SHA_INSTALLER_PS1@", sum("install-devagotchi.ps1")),
        (
            "@SHA_LINUX_X86_64@",
            sum("devagotchi-x86_64-unknown-linux-gnu.tar.xz"),
        ),
        (
            "@SHA_LINUX_AARCH64@",
            sum("devagotchi-aarch64-unknown-linux-gnu.tar.xz"),
        ),
        (
            "@SHA_WINDOWS_X86_64@",
            sum("devagotchi-x86_64-pc-windows-msvc.zip"),
        ),
        (
            "@SHA_WINDOWS_AARCH64@",
            sum("devagotchi-aarch64-pc-windows-msvc.zip"),
        ),
    ];
    let installer = match shell {
        Shell::Sh => "install-devagotchi.sh",
        Shell::PowerShell => "install-devagotchi.ps1",
    };
    downloads.sha256(installer)?;
    let mut text = match shell {
        Shell::Sh => SH,
        Shell::PowerShell => PS1,
    }
    .to_string();
    for (placeholder, value) in replacements {
        text = text.replace(placeholder, &value);
    }
    Some(text)
}

/// What a dead or unknown code gets: a script that only says so.
fn refusal(shell: Shell, hub: &str) -> String {
    let message = format!(
        "devagotchi join: this join code was already used, has expired or is unknown. Get a new one under Add a machine at {}/account",
        hub.trim_end_matches('/')
    );
    match shell {
        Shell::Sh => format!("#!/bin/sh\necho '{message}' >&2\nexit 1\n"),
        Shell::PowerShell => format!("throw '{message}'\n"),
    }
}

fn text(status: StatusCode, body: String) -> Response {
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// `/join/<code>` and `/dist/<file>`: open to anyone, since a machine joining has no
/// account yet; the code is what lets it in, and only enrolling spends it.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/join/{code}", axum::routing::get(join))
        .route("/dist/{name}", axum::routing::get(download))
}

pub async fn join(State(state): State<AppState>, Path(asked): Path<String>) -> Response {
    let (code, shell) = if let Some(code) = asked.strip_suffix(".compose.yml") {
        return compose::serve(&state, code).await;
    } else if let Some(code) = asked.strip_suffix(".host.sh") {
        (code, Shell::Sh)
    } else if let Some(code) = asked.strip_suffix(".ps1") {
        (code, Shell::PowerShell)
    } else {
        (asked.strip_suffix(".sh").unwrap_or(&asked), Shell::Sh)
    };
    let hub = &state.hub.config.public_url;
    let Some(downloads) = state.hub.downloads.clone() else {
        return text(
            StatusCode::NOT_FOUND,
            "this hub hands out no builds\n".into(),
        );
    };
    if !valid_code(code) {
        return text(StatusCode::OK, refusal(shell, hub));
    }
    match state.hub.links.join_script(code, shell.way()).await {
        Ok(Some(device)) => match script(
            shell,
            hub,
            &device,
            &shown_name(&state, &device).await,
            code,
            &downloads,
        ) {
            Some(body) => text(StatusCode::OK, body),
            None => text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "this hub could not write a join script for that machine\n".into(),
            ),
        },
        Ok(None) => text(StatusCode::OK, refusal(shell, hub)),
        Err(_) => text(
            StatusCode::SERVICE_UNAVAILABLE,
            "the hub could not check that code right now; run the same line again in a minute\n"
                .into(),
        ),
    }
}

/// The name `machines` shows the person whose machine `device` (its stored name) is.
async fn shown_name(state: &AppState, device: &str) -> String {
    let owner = state
        .hub
        .links
        .device(device)
        .await
        .ok()
        .flatten()
        .and_then(|found| found.owner);
    let (Some(owner), Some(hub)) = (owner, state.passkeys.as_ref()) else {
        return device.to_string();
    };
    match hub.accounts().get(&owner).await {
        Ok(Some(account)) => state
            .hub
            .shown_name(&hub.accounts().subject(&account, "passkey"), device),
        _ => device.to_string(),
    }
}

/// One file of the build this hub hands out, streamed; only names `SHA256SUMS` lists.
pub async fn download(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    if name == "SHA256SUMS"
        && let Some(downloads) = &state.hub.downloads
    {
        return text(StatusCode::OK, downloads.sha256sums_text());
    }
    let found = state
        .hub
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.path(&name));
    let Some(path) = found else {
        return text(StatusCode::NOT_FOUND, "no such file\n".into());
    };
    let Ok(file) = tokio::fs::File::open(&path).await else {
        return text(StatusCode::NOT_FOUND, "no such file\n".into());
    };
    let length = file.metadata().await.map(|m| m.len()).ok();
    let chunks = futures_util::stream::unfold(file, |mut file| async move {
        use tokio::io::AsyncReadExt;
        let mut buffer = vec![0u8; 64 * 1024];
        match file.read(&mut buffer).await {
            Ok(0) => None,
            Ok(read) => {
                buffer.truncate(read);
                Some((
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(buffer)),
                    file,
                ))
            }
            Err(error) => Some((Err(error), file)),
        }
    });
    let mut response = Response::new(Body::from_stream(chunks));
    let headers = response.headers_mut();
    let kind = if name.ends_with(".sh") || name.ends_with(".ps1") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    };
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    response
}

#[cfg(test)]
mod tests;
