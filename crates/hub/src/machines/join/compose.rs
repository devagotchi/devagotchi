//! `/join/<code>.compose.yml`: the Docker shape of one join code (increment 209).
//!
//! The file (`compose.yml`) leaves a running machine: a broker, its link to this hub, and a
//! one-shot `enroll` that binds the machine's key with the code, held only in that service's
//! environment. The link and `enroll` run the hub's own image, the one this hub's build was
//! published as (`image`); the broker runs the same binary on Debian, since a machine needs
//! a shell and the hub's image has none. A build that was never published (`dev`) has no
//! image, and the file says so instead of naming one that does not exist.

use axum::{http::StatusCode, response::Response};

use super::{safe_url, shown_name, text, valid_code};
use crate::{AppState, config::valid_name, machines::downloads::Downloads};

const TEMPLATE: &str = include_str!("compose.yml");

/// Where the hub's image is published (`.github/workflows/image.yml`).
pub const REPOSITORY: &str = "ghcr.io/devagotchi/devagotchi";

/// The image of the hub's build `version`, as image.yml tags it: a main build `main-<sha7>`
/// (and a pull request's `ci-<sha7>`) is `:sha-<sha7>`, a release `X.Y.Z` is `:X.Y.Z`. A
/// build with no published image (`dev`) has none.
pub fn image(version: &str) -> Option<String> {
    let tag = match version
        .strip_prefix("main-")
        .or_else(|| version.strip_prefix("ci-"))
    {
        Some(sha) if sha.len() == 7 && sha.bytes().all(|b| b.is_ascii_hexdigit()) => {
            format!("sha-{}", sha.to_ascii_lowercase())
        }
        Some(_) => return None,
        None if version.as_bytes().first().is_some_and(u8::is_ascii_digit)
            && version.len() <= 64
            && version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) =>
        {
            version.to_string()
        }
        None => return None,
    };
    Some(format!("{REPOSITORY}:{tag}"))
}

/// The compose file for one live code. `None` when the build has no image or a value could
/// not be written safely; every value was checked against a strict pattern first.
pub fn file(hub: &str, device: &str, shown: &str, code: &str, version: &str) -> Option<String> {
    let hub = hub.trim_end_matches('/');
    let image = image(version)?;
    if !safe_url(hub) || !valid_name(device) || !valid_name(shown) || !valid_code(code) {
        return None;
    }
    let mut text = TEMPLATE.to_string();
    for (placeholder, value) in [
        ("@HUB@", hub),
        ("@DEVICE@", device),
        ("@NAME@", shown),
        ("@CODE@", code),
        ("@IMAGE@", image.as_str()),
        ("@VERSION@", version),
    ] {
        text = text.replace(placeholder, value);
    }
    Some(text)
}

/// `/join/<code>.compose.yml`. A refusal is never a 200, so the join line's `curl -f` stops
/// before `docker compose` runs.
pub async fn serve(state: &AppState, code: &str) -> Response {
    let hub = state.hub.config.public_url.trim_end_matches('/');
    let Some(downloads) = state.hub.downloads.clone() else {
        return text(
            StatusCode::NOT_FOUND,
            "this hub hands out no builds\n".into(),
        );
    };
    if let Some(refused) = unpublished(&downloads) {
        return text(StatusCode::CONFLICT, refused);
    }
    let dead = || {
        text(
            StatusCode::GONE,
            format!(
                "devagotchi join: this join code was already used, has expired or is unknown. Get a new one under Add a machine at {hub}/account\n"
            ),
        )
    };
    if !valid_code(code) {
        return dead();
    }
    match state.hub.links.join_script(code, "compose").await {
        Ok(Some(device)) => {
            let shown = shown_name(state, &device).await;
            match file(hub, &device, &shown, code, &downloads.version) {
                Some(body) => text(StatusCode::OK, body),
                None => text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "this hub could not write a compose file for that machine\n".into(),
                ),
            }
        }
        Ok(None) => dead(),
        Err(_) => text(
            StatusCode::SERVICE_UNAVAILABLE,
            "the hub could not check that code right now; run the same line again in a minute\n"
                .into(),
        ),
    }
}

/// Why this hub's build has no Docker machine to hand out, when it has none.
fn unpublished(downloads: &Downloads) -> Option<String> {
    image(&downloads.version).is_none().then(|| {
        format!(
            "this hub runs {}, a build with no published image, so it has no Docker machine to hand out; join this machine with the Linux line instead\n",
            downloads.version
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "dvg-0123456789abcdef0123456789abcdef";

    #[test]
    fn the_image_follows_the_tags_image_yml_publishes() {
        assert_eq!(
            image("main-9842d39").as_deref(),
            Some("ghcr.io/devagotchi/devagotchi:sha-9842d39")
        );
        assert_eq!(
            image("ci-abc1234").as_deref(),
            Some("ghcr.io/devagotchi/devagotchi:sha-abc1234")
        );
        assert_eq!(
            image("0.4.0").as_deref(),
            Some("ghcr.io/devagotchi/devagotchi:0.4.0")
        );
        assert_eq!(
            image("0.4.0-alpha.1").as_deref(),
            Some("ghcr.io/devagotchi/devagotchi:0.4.0-alpha.1")
        );
        for unpublished in ["dev", "main-", "main-9842d3", "main-zzzzzzz", "v0.4.0", ""] {
            assert_eq!(image(unpublished), None, "{unpublished}");
        }
    }

    #[test]
    fn the_compose_file_names_the_builds_image_and_holds_the_code_only_in_its_environment() {
        let file = file(
            "https://app.devagotchi.com/",
            "t0123456789ab-dabo",
            "dabo",
            CODE,
            "main-9842d39",
        )
        .unwrap();
        assert!(!file.contains('@'), "every placeholder is filled:\n{file}");
        let image = "ghcr.io/devagotchi/devagotchi:sha-9842d39";
        assert!(file.contains(&format!("FROM {image} AS hub")), "{file}");
        assert_eq!(
            file.matches(&format!("image: {image}")).count(),
            2,
            "{file}"
        );
        // The code: once, as the value of the enroll service's environment, nowhere else.
        let lines: Vec<&str> = file.lines().collect();
        let at: Vec<usize> = (0..lines.len())
            .filter(|&i| lines[i].contains(CODE))
            .collect();
        assert_eq!(at.len(), 1, "{file}");
        assert_eq!(
            lines[at[0]].trim(),
            format!("DEVAGOTCHI_ENROLL_CODE: \"{CODE}\"")
        );
        assert_eq!(lines[at[0] - 1].trim(), "environment:", "{file}");
        assert!(lines[..at[0]].iter().rev().any(|l| *l == "  enroll:"));
        // Enrolls under the stored name, runs the link marked as a container, the broker
        // with a workspace, and the project carries the name the person knows.
        assert!(
            file.contains(
                "--hub, \"https://app.devagotchi.com\", --device, \"t0123456789ab-dabo\""
            )
        );
        assert!(file.contains("DEVAGOTCHI_MACHINE_KIND: container"));
        assert!(file.contains("command: [link, run, --state-dir, /state/link]"));
        assert!(file.contains("--broker, /state/broker/broker.sock"));
        assert!(file.contains("command: [start, --state-dir, /state/broker"));
        assert!(file.contains("condition: service_completed_successfully"));
        assert!(
            file.contains("\nname: devagotchi-dabo\n")
                || file.contains("\r\nname: devagotchi-dabo\r\n")
        );
        assert!(!file.contains('$'), "nothing for compose to interpolate");
    }

    #[test]
    fn nothing_unsafe_or_unpublished_is_written() {
        let hub = "https://app.devagotchi.com";
        for (hub, device, code, version) in [
            ("https://evil.example/\"; x", "dabo", CODE, "main-9842d39"),
            (hub, "Da\"bo", CODE, "main-9842d39"),
            (hub, "dabo", "dvg-\"$(id)\"", "main-9842d39"),
            (hub, "dabo", CODE, "dev"),
        ] {
            assert!(
                file(hub, device, "dabo", code, version).is_none(),
                "{hub} {device} {code} {version}"
            );
        }
    }
}
