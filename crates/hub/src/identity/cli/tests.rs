use super::*;

#[test]
fn a_password_keeps_its_spaces_and_one_password_is_all_that_is_taken() {
    assert_eq!(
        read_password(&b"  password  \r\n"[..]).unwrap(),
        "  password  "
    );
    for input in [
        b"one\ntwo".to_vec(),
        vec![b'x'; 4099],
        vec![0xff],
        b"password\0".to_vec(),
    ] {
        assert_eq!(
            read_password(input.as_slice()).unwrap_err().code,
            "password_input"
        );
    }
}

#[test]
fn set_password_takes_the_password_on_stdin_never_as_an_argument() {
    let dir = std::env::temp_dir().to_string_lossy().into_owned();
    let base = [
        "--state-dir",
        &dir,
        "--config",
        &dir,
        "--account",
        "recovery-owner",
        "--email",
        "owner@example.test",
    ];
    let parse_with =
        |extra: &[&str]| parse_set_password(base.iter().chain(extra).map(OsString::from).collect());
    assert!(matches!(
        parse_with(&["--password-stdin"]).unwrap(),
        Command::SetPassword(_)
    ));
    for extra in [
        &[][..],
        &["--password", "secret"],
        &["--password-stdin", "--password-stdin"],
        &["--password-stdin", "--account", "another"],
    ] {
        assert!(parse_with(extra).is_err(), "{extra:?}");
    }
    assert!(matches!(
        parse_set_password(vec!["--help".into()]).unwrap(),
        Command::Help
    ));
    assert!(matches!(
        parse_owner(vec!["--help".into()]).unwrap(),
        Command::Help
    ));
    assert!(crate::USAGE.contains("hub set-password"));
}

#[test]
fn set_password_sets_an_existing_accounts_password_ends_its_sessions_and_is_audited() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let Some(url) = runtime.block_on(crate::store::testing::empty_postgres()) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let config_path = dir.path().join("hub.json");
    std::fs::write(
        &config_path,
        serde_json::json!({
            "listen": "127.0.0.1:1", "public_url": "http://127.0.0.1:8787",
            "auth": { "mode": "hub" }, "owner": "recovery-owner",
            "brokers": [], "grants": [], "storage": { "url": url }
        })
        .to_string(),
    )
    .unwrap();
    let config = read_config(&config_path).unwrap();
    let session = runtime.block_on(async {
        let accounts = accounts_at(&state, &config).await.unwrap();
        let owner = accounts
            .ensure_owner(Some("recovery-owner"), "Owner", [7; 16])
            .await
            .unwrap();
        accounts.open_session(&owner.id, None).await.unwrap()
    });
    let options = |account: &str| SetPassword {
        state: state.clone(),
        config: config_path.clone(),
        account: account.into(),
        email: "Owner@Example.test".into(),
    };
    assert_eq!(
        set_password_from(
            options("missing-account"),
            &config,
            &b"a good password\n"[..]
        )
        .unwrap_err()
        .code,
        "no_account"
    );
    set_password_from(
        options("recovery-owner"),
        &config,
        &b"a good password\n"[..],
    )
    .unwrap();
    runtime.block_on(async {
        let accounts = accounts_at(&state, &config).await.unwrap();
        let signed = accounts
            .sign_in_password("owner@example.test", "a good password")
            .await
            .unwrap();
        assert_eq!(signed.id, "recovery-owner");
        assert!(signed.owner, "still the owner");
        assert!(
            accounts.session(&session).await.unwrap().is_none(),
            "its browser session ended"
        );
        let chain = Audit::new(accounts.store().clone()).verify().await.unwrap();
        assert!(chain.ok && chain.entries == 1, "{chain:?}");
    });
}
