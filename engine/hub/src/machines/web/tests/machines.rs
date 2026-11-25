//! A person's own machines through the web: add one and revoke it, the one line that joins
//! it, and the API the Connect page uses.

use std::sync::Arc;

use axum::http::{StatusCode, header};
use serde_json::Value;

use crate::web::tests::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_adds_a_machine_of_their_own_and_revokes_it() {
    let w = or_skip!(world());
    let other = w.sign_in("tok-other").await;
    let owner = w.sign_in("tok-owner").await;
    let post = |path: &'static str, cookie: &str, site: &'static str, name: &'static str| {
        w.http
            .post(format!("{}{path}", w.url))
            .header(header::COOKIE, cookie.to_string())
            .header("sec-fetch-site", site)
            .form(&[("name", name)])
    };
    let refused = post("/machines/add", &other, "cross-site", "studio")
        .send()
        .await
        .unwrap();
    assert_eq!(
        refused.status(),
        StatusCode::FORBIDDEN,
        "another site cannot mint codes"
    );

    let page = post("/machines/add", &other, "same-origin", "Studio")
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store");
    let body = page.text().await.unwrap();
    let code = body
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| word.starts_with("dvg-") && word.len() == 36)
        .expect("the code is on the page")
        .to_string();
    assert!(body.contains("Add studio") && body.contains("--device studio"));
    // A configured broker's name and an impossible name get no code.
    for name in ["gus", "no spaces"] {
        let taken = post("/machines/add", &other, "same-origin", name)
            .send()
            .await
            .unwrap();
        assert_eq!(taken.status(), StatusCode::CONFLICT, "{name}");
    }

    // The machine enrolls with the code (the link's own tests cover the handshake).
    let devices = w.devices.clone();
    devices
        .enroll("studio", &code, "studio-key", false)
        .await
        .unwrap();
    let mine: Value = w
        .get("/api/v1/me", Some(&other))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        mine["machines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"] == "studio" && m["owned"] == true && m["status"] == "offline"),
        "{mine}"
    );
    // It is that account's alone: not even the hub's owner sees or reaches it.
    let theirs: Value = w
        .get("/api/v1/me", Some(&owner))
        .await
        .json()
        .await
        .unwrap();
    assert!(!theirs.to_string().contains("studio"), "{theirs}");
    let subject = |id: &str| crate::identity::Subject {
        id: id.into(),
        via: "dev",
        tenant: None,
        name: None,
        org: None,
        provisional_until: None,
        agent: None,
        claim_code: None,
        client: None,
        provisional_warning: false,
        access: None,
    };
    assert!(matches!(
        w.hub.route(&subject(OTHER), "studio").await,
        Ok(crate::mcp::Route::Link(name)) if name == "studio"
    ));
    assert_eq!(
        w.hub
            .route(&subject(OWNER), "studio")
            .await
            .err()
            .unwrap()
            .0
            .code,
        "unknown_broker"
    );

    // Only its owner revokes it, and only from this hub's page.
    let not_theirs = post("/machines/revoke", &owner, "same-origin", "studio")
        .send()
        .await
        .unwrap();
    assert_eq!(not_theirs.status(), StatusCode::CONFLICT);
    let cross = post("/machines/revoke", &other, "cross-site", "studio")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::FORBIDDEN);
    let revoked = post("/machines/revoke", &other, "same-origin", "studio")
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::SEE_OTHER);
    assert!(devices.is_revoked("studio").await.unwrap());
    let html = w.get("/account", Some(&other)).await.text().await.unwrap();
    assert!(
        html.contains("<tr id=\"studio\">") && html.contains("Revoked"),
        "{html}"
    );
    // Machines, codes and phones are kept in the hub's Postgres (free-edition R3), never a file.
    assert!(
        !w.dir.path().join("devices.sqlite3").exists(),
        "the machine registry wrote a SQLite file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_one_line_joins_once_and_downloads_only_what_the_hub_vouches_for() {
    use sha2::{Digest, Sha256};
    let dist = tempfile::tempdir().unwrap();
    let mut sums = String::new();
    for (name, bytes) in [
        ("install-devagotchi.sh", b"#!/usr/bin/env bash\n".as_slice()),
        ("install-devagotchi.ps1", b"param()\n".as_slice()),
        (
            "devagotchi-x86_64-unknown-linux-gnu.tar.xz",
            b"linux build".as_slice(),
        ),
    ] {
        std::fs::write(dist.path().join(name), bytes).unwrap();
        sums.push_str(&format!("{}  {name}\n", hex::encode(Sha256::digest(bytes))));
    }
    std::fs::write(dist.path().join("SHA256SUMS"), sums).unwrap();
    std::fs::write(dist.path().join("VERSION"), "main-test123\n").unwrap();
    std::fs::write(dist.path().join("not-listed.txt"), "secret").unwrap();
    let downloads = Arc::new(crate::machines::downloads::Downloads::open(dist.path()).unwrap());
    let w = or_skip!(world_with(Some(downloads.clone())));
    let other = w.sign_in("tok-other").await;
    let page = w
        .http
        .post(format!("{}/machines/add", w.url))
        .header(header::COOKIE, &other)
        .header("sec-fetch-site", "same-origin")
        .form(&[("name", "studio")])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let code = page
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| word.starts_with("dvg-") && word.len() == 36)
        .unwrap()
        .to_string();
    assert!(
        page.contains(&format!("curl -fsSL {}/join/{code} | sudo sh", w.url)),
        "{page}"
    );
    assert!(
        page.contains(&format!("irm {}/join/{code}.ps1 | iex", w.url)),
        "{page}"
    );

    // Fetching the script, any number of times, does not spend the code.
    for _ in 0..2 {
        let script = w.get(&format!("/join/{code}"), None).await;
        assert_eq!(script.status(), StatusCode::OK);
        assert_eq!(script.headers()[header::CACHE_CONTROL], "no-store");
        let text = script.text().await.unwrap();
        assert!(text.contains("device='studio'") && text.contains("version='main-test123'"));
        assert!(
            text.contains(
                downloads
                    .sha256("devagotchi-x86_64-unknown-linux-gnu.tar.xz")
                    .unwrap()
            )
        );
    }
    let ps = w
        .get(&format!("/join/{code}.ps1"), None)
        .await
        .text()
        .await
        .unwrap();
    assert!(ps.contains("$device = 'studio'"), "{ps}");
    // A build never published as an image has no Docker machine to hand out, and says so.
    let docker = w.get(&format!("/join/{code}.compose.yml"), None).await;
    assert_eq!(docker.status(), StatusCode::CONFLICT);
    let said = docker.text().await.unwrap();
    assert!(
        said.contains("no published image") && !said.contains(&code),
        "{said}"
    );

    // Only what SHA256SUMS lists is served, plus the checksum list itself.
    let file = w
        .get("/dist/devagotchi-x86_64-unknown-linux-gnu.tar.xz", None)
        .await;
    assert_eq!(file.status(), StatusCode::OK);
    assert_eq!(file.bytes().await.unwrap().as_ref(), b"linux build");
    let sums = w.get("/dist/SHA256SUMS", None).await;
    assert_eq!(sums.status(), StatusCode::OK);
    let sums = sums.text().await.unwrap();
    assert!(
        sums.contains("devagotchi-x86_64-unknown-linux-gnu.tar.xz"),
        "{sums}"
    );
    for name in ["not-listed.txt", "..%2FSHA256SUMS"] {
        assert_eq!(
            w.get(&format!("/dist/{name}"), None).await.status(),
            StatusCode::NOT_FOUND,
            "{name}"
        );
    }

    // Enrolling spends it; the same line then only says so.
    let devices = w.devices.clone();
    devices
        .enroll("studio", &code, "studio-key", false)
        .await
        .unwrap();
    let spent = w
        .get(&format!("/join/{code}"), None)
        .await
        .text()
        .await
        .unwrap();
    assert!(
        spent.contains("already used, has expired or is unknown") && !spent.contains("device="),
        "{spent}"
    );
    let unknown = w
        .get("/join/dvg-00000000000000000000000000000000", None)
        .await
        .text()
        .await
        .unwrap();
    assert!(unknown.contains("exit 1"));
}

/// The Connect page's call: a bearer gets a one-time code and the hub's own join lines;
/// the hub's cookie alone gets nothing (no cross-site code minting); the code is spent by
/// one enroll and the machine then shows under that account's `/api/v1/me` only.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_add_machine_is_bearer_only_and_the_code_joins_for_that_account() {
    use sha2::{Digest, Sha256};
    let dist = tempfile::tempdir().unwrap();
    let mut sums = String::new();
    for (name, bytes) in [
        ("install-devagotchi.sh", b"#!/usr/bin/env bash\n".as_slice()),
        ("install-devagotchi.ps1", b"param()\n".as_slice()),
    ] {
        std::fs::write(dist.path().join(name), bytes).unwrap();
        sums.push_str(&format!("{}  {name}\n", hex::encode(Sha256::digest(bytes))));
    }
    std::fs::write(dist.path().join("SHA256SUMS"), sums).unwrap();
    std::fs::write(dist.path().join("VERSION"), "main-test123\n").unwrap();
    let downloads = Arc::new(crate::machines::downloads::Downloads::open(dist.path()).unwrap());
    let w = or_skip!(world_with(Some(downloads)));

    let cookie = w.sign_in("tok-other").await;
    let by_cookie = w
        .http
        .post(format!("{}/api/v1/machines", w.url))
        .header(header::COOKIE, &cookie)
        .json(&serde_json::json!({ "name": "laptop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(by_cookie.status(), StatusCode::UNAUTHORIZED);

    let refused = w
        .http
        .post(format!("{}/api/v1/machines", w.url))
        .bearer_auth("nope")
        .json(&serde_json::json!({ "name": "laptop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);

    let issued = w
        .http
        .post(format!("{}/api/v1/machines", w.url))
        .bearer_auth("tok-other")
        .json(&serde_json::json!({ "name": "Laptop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::CREATED);
    assert_eq!(issued.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value = issued.json().await.unwrap();
    let code = body["code"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "laptop");
    assert_eq!(
        body["commands"]["linux"],
        format!(
            "curl -fsSL {}/join/{code} | sudo sh",
            w.url.trim_end_matches('/')
        )
    );
    assert_eq!(
        body["commands"]["windows"],
        format!("irm {}/join/{code}.ps1 | iex", w.url.trim_end_matches('/'))
    );

    let devices = w.devices.clone();
    devices
        .enroll("laptop", &code, "laptop-key", false)
        .await
        .unwrap();
    assert!(
        devices
            .enroll("laptop", &code, "again-key", false)
            .await
            .is_err(),
        "a code works once"
    );
    let mine: Value = w
        .http
        .get(format!("{}/api/v1/me", w.url))
        .bearer_auth("tok-other")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(mine["machines"].to_string().contains("laptop"));
    let theirs: Value = w
        .http
        .get(format!("{}/api/v1/me", w.url))
        .bearer_auth("tok-owner")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !theirs["machines"].to_string().contains("laptop"),
        "another account never sees it: {theirs}"
    );
}

/// The Docker line's file (increment 209): the image of the hub's build, the code in the
/// enroll service's environment only, any number of fetches until enrolling spends it, and
/// then a refusal that is never a 200, so `curl -f` stops the line before `docker compose`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_docker_line_gets_a_compose_file_for_the_builds_image_until_the_code_is_spent() {
    use sha2::{Digest, Sha256};
    let dist = tempfile::tempdir().unwrap();
    let bytes = b"#!/usr/bin/env bash
"
    .as_slice();
    std::fs::write(dist.path().join("install-devagotchi.sh"), bytes).unwrap();
    std::fs::write(
        dist.path().join("SHA256SUMS"),
        format!(
            "{}  install-devagotchi.sh
",
            hex::encode(Sha256::digest(bytes))
        ),
    )
    .unwrap();
    std::fs::write(
        dist.path().join("VERSION"),
        "main-abc1234
",
    )
    .unwrap();
    let downloads = Arc::new(crate::machines::downloads::Downloads::open(dist.path()).unwrap());
    let w = or_skip!(world_with(Some(downloads)));
    let other = w.sign_in("tok-other").await;
    let page = w
        .http
        .post(format!("{}/machines/add", w.url))
        .header(header::COOKIE, &other)
        .header("sec-fetch-site", "same-origin")
        .form(&[("name", "docked")])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let code = page
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| word.starts_with("dvg-") && word.len() == 36)
        .unwrap()
        .to_string();
    for _ in 0..2 {
        let file = w.get(&format!("/join/{code}.compose.yml"), None).await;
        assert_eq!(file.status(), StatusCode::OK);
        assert_eq!(file.headers()[header::CACHE_CONTROL], "no-store");
        let file = file.text().await.unwrap();
        assert!(
            file.contains("FROM ghcr.io/devagotchi/devagotchi:sha-abc1234 AS hub"),
            "{file}"
        );
        assert!(file.contains(&format!("DEVAGOTCHI_ENROLL_CODE: \"{code}\"")));
        assert_eq!(file.matches(&code).count(), 1, "{file}");
        assert!(file.contains(&format!("--hub, \"{}\", --device, \"docked\"", w.url)));
    }
    w.devices
        .enroll("docked", &code, "docked-key", false)
        .await
        .unwrap();
    for asked in [
        code.as_str(),
        "dvg-00000000000000000000000000000000",
        "nope",
    ] {
        let spent = w.get(&format!("/join/{asked}.compose.yml"), None).await;
        assert_eq!(spent.status(), StatusCode::GONE, "{asked}");
        let said = spent.text().await.unwrap();
        assert!(
            said.contains("already used, has expired or is unknown"),
            "{said}"
        );
    }
}
