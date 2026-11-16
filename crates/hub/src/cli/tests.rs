//! The hub's command line: each verb's accepted forms, and the operator commands that reach
//! the store (`owner`, `check-mail`).

use super::*;

#[cfg(unix)]
const STATE: &str = "/s";
#[cfg(windows)]
const STATE: &str = r"C:\s";
#[cfg(unix)]
const CONFIG: &str = "/c.json";
#[cfg(windows)]
const CONFIG: &str = r"C:\c.json";

#[test]
fn cli_requires_both_absolute_paths() {
    let ok = parse_args(["--state-dir", STATE, "--config", CONFIG].map(OsString::from)).unwrap();
    assert_eq!(
        ok,
        Command::Serve {
            state: PathBuf::from(STATE),
            config: PathBuf::from(CONFIG)
        }
    );
    assert!(parse_args(["--config", CONFIG].map(OsString::from)).is_err());
    assert!(parse_args(["--state-dir", "s", "--config", CONFIG].map(OsString::from)).is_err());
    assert_eq!(
        parse_args(["--help"].map(OsString::from)).unwrap(),
        Command::Help
    );
    assert_eq!(
        parse_args(["serve", "--state-dir", STATE, "--config", CONFIG].map(OsString::from))
            .unwrap(),
        ok,
        "the nested spelling and the bare one are the same command"
    );
}

#[test]
fn analytics_resend_takes_the_config_alone() {
    assert_eq!(
        parse_args(["analytics-resend", "--config", CONFIG].map(OsString::from)).unwrap(),
        Command::AnalyticsResend {
            config: PathBuf::from(CONFIG)
        }
    );
    for bad in [
        vec!["analytics-resend"],
        vec!["analytics-resend", "--config", "c.json"],
        vec!["analytics-resend", "--config", CONFIG, "--to", "x"],
    ] {
        assert!(parse_args(bad.into_iter().map(OsString::from)).is_err());
    }
}

#[test]
fn check_mail_takes_the_config_and_one_address() {
    assert_eq!(
        parse_args(["check-mail", "--config", CONFIG].map(OsString::from)).unwrap(),
        Command::CheckMail {
            config: PathBuf::from(CONFIG),
            to: None
        }
    );
    assert_eq!(
        parse_args(
            ["check-mail", "--config", CONFIG, "--to", "you@example.com"].map(OsString::from)
        )
        .unwrap(),
        Command::CheckMail {
            config: PathBuf::from(CONFIG),
            to: Some("you@example.com".into())
        }
    );
    for wrong in [
        vec!["check-mail", "--config", "c.json"],
        vec!["check-mail", "--to", "you@example.com"],
        vec!["check-mail", "--config", CONFIG, "--to", "nobody"],
        vec!["check-mail", "--config", CONFIG, "--state-dir", STATE],
    ] {
        assert!(
            parse_args(wrong.iter().map(OsString::from)).is_err(),
            "{wrong:?}"
        );
    }
}

#[test]
fn check_mail_signs_in_and_sends_the_check_to_one_address() {
    // The stub mail server lives on its own runtime; `manage` builds one of its own.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stub = runtime.block_on(notify::mail::stub::start("hub"));
    let dir = tempfile::tempdir().unwrap();
    let password = dir.path().join("smtp-password");
    std::fs::write(&password, "mail-pass\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&password, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let config_path = dir.path().join("hub.json");
    let config = |email: serde_json::Value| {
        std::fs::write(
            &config_path,
            serde_json::json!({
                "listen": "127.0.0.1:9",
                "public_url": "https://hub.example.com",
                "auth": { "mode": "hub", "rp_id": "hub.example.com", "email": email },
                "brokers": [],
                "grants": []
            })
            .to_string(),
        )
        .unwrap();
    };
    config(serde_json::json!({
        "smtp_url": stub.url,
        "password_file": password.to_string_lossy(),
        "from": "Devagotchi <sign-in@hub.example.com>"
    }));
    manage(Command::CheckMail {
        config: config_path.clone(),
        to: None,
    })
    .unwrap();
    assert!(
        stub.received.lock().unwrap().is_empty(),
        "check sends nothing"
    );
    manage(Command::CheckMail {
        config: config_path.clone(),
        to: Some("you@example.com".into()),
    })
    .unwrap();
    let received = stub.received.lock().unwrap().clone();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].to, "you@example.com");
    assert!(
        received[0].data.contains("mail check"),
        "{}",
        received[0].data
    );
    assert!(
        !received[0].data.contains("mail-pass"),
        "{}",
        received[0].data
    );
    // A variable the config names that is not set is reported first, by name.
    config(serde_json::json!({
        "smtp_url": stub.url,
        "password_env": "DEVAGOTCHI_TEST_SMTP_PASSWORD_THAT_IS_NOT_SET",
        "from": "Devagotchi <sign-in@hub.example.com>"
    }));
    let error = manage(Command::CheckMail {
        config: config_path.clone(),
        to: None,
    })
    .unwrap_err();
    assert!(
        error
            .message
            .contains("DEVAGOTCHI_TEST_SMTP_PASSWORD_THAT_IS_NOT_SET is not set"),
        "{error}"
    );
    // A config without mail is told so.
    config(serde_json::Value::Null);
    let error = manage(Command::CheckMail {
        config: config_path,
        to: None,
    })
    .unwrap_err();
    assert!(error.message.contains("no mail server"), "{error}");
}

#[test]
fn device_commands_take_a_state_dir_a_device_and_maybe_the_hubs_config() {
    let enroll =
        parse_args(["enroll", "--state-dir", STATE, "--device", "gus"].map(OsString::from))
            .unwrap();
    assert_eq!(
        enroll,
        Command::Enroll {
            state: PathBuf::from(STATE),
            config: None,
            device: "gus".into()
        }
    );
    // The hub's config names the Postgres the machines are kept in.
    assert_eq!(
        parse_args(
            [
                "revoke",
                "--state-dir",
                STATE,
                "--config",
                CONFIG,
                "--device",
                "gus"
            ]
            .map(OsString::from)
        )
        .unwrap(),
        Command::Revoke {
            state: PathBuf::from(STATE),
            config: Some(PathBuf::from(CONFIG)),
            device: "gus".into()
        }
    );
    assert!(matches!(
        parse_args(["devices", "--state-dir", STATE].map(OsString::from)).unwrap(),
        Command::Devices { .. }
    ));
    assert!(parse_args(["enroll", "--state-dir", STATE].map(OsString::from)).is_err());
    assert!(
        parse_args(["revoke", "--state-dir", STATE, "--config", CONFIG].map(OsString::from))
            .is_err()
    );
}

#[test]
fn owner_takes_a_state_dir_a_config_and_maybe_a_name() {
    let owner = |extra: &[&str]| {
        let mut args = vec!["owner", "--state-dir", STATE, "--config", CONFIG];
        args.extend_from_slice(extra);
        parse_args(args.into_iter().map(OsString::from))
    };
    assert_eq!(
        owner(&[]).unwrap(),
        Command::Owner {
            state: STATE.into(),
            config: CONFIG.into(),
            name: None,
            replace: false
        }
    );
    assert!(matches!(
        owner(&["--name", "Jesse"]).unwrap(),
        Command::Owner { name: Some(n), .. } if n == "Jesse"
    ));
    assert!(matches!(
        owner(&["--replace"]).unwrap(),
        Command::Owner { replace: true, .. }
    ));
    assert!(owner(&["--replace", "--replace"]).is_err());
    assert!(owner(&["--device", "x"]).is_err());
    assert!(
        parse_args(["owner", "--state-dir", STATE].map(OsString::from)).is_err(),
        "the config says where the link points"
    );
}

#[test]
fn the_owner_command_makes_the_owner_once_and_only_in_hub_mode() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let Some(url) = runtime.block_on(store::testing::empty_postgres()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let config = dir.path().join("hub.json");
    let write = |auth: serde_json::Value, owner: Option<&str>| {
        let mut value = serde_json::json!({
            "listen": "127.0.0.1:1",
            "public_url": "http://127.0.0.1:8787",
            "auth": auth,
            "brokers": [],
            "grants": [],
            "storage": { "url": url }
        });
        if let Some(owner) = owner {
            value["owner"] = owner.into();
        }
        std::fs::write(&config, value.to_string()).unwrap();
    };
    let run = |name: Option<&str>| {
        manage(Command::Owner {
            state: state.clone(),
            config: config.clone(),
            name: name.map(str::to_string),
            replace: false,
        })
    };
    let tokens = dir.path().join("tokens.json");
    std::fs::write(&tokens, "{}").unwrap();
    write(
        serde_json::json!({ "mode": "dev", "tokens_file": tokens }),
        None,
    );
    assert_eq!(run(None).unwrap_err().code, "invalid_cli");

    // The Texas case: the config's owner id becomes the owner's account.
    write(
        serde_json::json!({ "mode": "hub" }),
        Some("395290028818825218"),
    );
    run(Some("Jesse")).unwrap();
    run(None).unwrap();
    let (owner, chain) = runtime.block_on(async {
        let store = store::open(&url).await.unwrap();
        let accounts = identity::accounts::Accounts::open(store.clone())
            .await
            .unwrap();
        let chain = Audit::new(store).verify().await.unwrap();
        (accounts.owner().await.unwrap().unwrap(), chain)
    });
    assert_eq!(
        (owner.id.as_str(), owner.name.as_str()),
        ("395290028818825218", "Jesse")
    );
    // Each run is on the audit chain, without the link.
    assert!(chain.ok && chain.entries == 2, "{chain:?}");

    // A config naming someone else than the hub's owner is refused.
    write(
        serde_json::json!({ "mode": "hub" }),
        Some("acct_aaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    assert_eq!(run(None).unwrap_err().code, "owner_mismatch");
}

#[test]
fn connect_nomen_needs_both_ids_and_absolute_paths() {
    let connect = |extra: &[&str]| {
        let mut args = vec![
            "connect-nomen",
            "--state-dir",
            STATE,
            "--config",
            CONFIG,
            "--client-id",
            "7",
        ];
        args.extend_from_slice(extra);
        parse_args(args.into_iter().map(OsString::from))
    };
    assert!(
        connect(&["--project", "5"]).is_err(),
        "no issuer: nothing is assumed"
    );
    assert!(matches!(
        connect(&["--project", "5", "--issuer", "https://id.example.com"]).unwrap(),
        Command::ConnectNomen(c) if c.issuer == "https://id.example.com"
    ));
    assert!(connect(&[]).is_err(), "no project");
    assert!(connect(&["--project", "5", "--device", "x"]).is_err());
    assert!(
        parse_args(
            [
                "connect-nomen",
                "--state-dir",
                "rel",
                "--config",
                CONFIG,
                "--client-id",
                "7",
                "--project",
                "5"
            ]
            .map(OsString::from)
        )
        .is_err()
    );
}
