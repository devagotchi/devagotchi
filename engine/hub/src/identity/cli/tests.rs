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
    assert!(crate::cli::USAGE.contains("hub set-password"));
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

#[test]
fn test_accounts_takes_two_addresses_and_never_a_password() {
    let dir = std::env::temp_dir().to_string_lossy().into_owned();
    let parse_with = |extra: &[&str]| {
        let base = ["--state-dir", &dir, "--config", &dir];
        test_accounts::parse(base.iter().chain(extra).map(OsString::from).collect())
    };
    match parse_with(&[]).unwrap() {
        Command::TestAccounts(options) => {
            assert_eq!(options.admin_email, test_accounts::ADMIN_EMAIL);
            assert_eq!(options.user_email, test_accounts::USER_EMAIL);
        }
        other => panic!("{other:?}"),
    }
    match parse_with(&["--user-email", "Tester@Example.test"]).unwrap() {
        Command::TestAccounts(options) => assert_eq!(options.user_email, "tester@example.test"),
        other => panic!("{other:?}"),
    }
    for extra in [
        &["--password", "secret"][..],
        &["--user-email", "not an address"],
        &["--user-email", test_accounts::ADMIN_EMAIL],
        &["--admin-email"],
    ] {
        assert!(parse_with(extra).is_err(), "{extra:?}");
    }
    assert!(matches!(
        test_accounts::parse(vec!["--help".into()]).unwrap(),
        Command::Help
    ));
    assert!(crate::cli::USAGE.contains("hub test-accounts"));
}

#[test]
fn a_test_password_is_four_groups_of_unmistakable_characters() {
    let password =
        test_accounts::password([0, 31, 32, 255, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]);
    assert_eq!(password.len(), 19);
    assert_eq!(password.matches('-').count(), 3);
    assert!(
        password
            .chars()
            .all(|c| c == '-' || (c.is_ascii_alphanumeric() && !"lo01".contains(c))),
        "{password}"
    );
    assert_ne!(
        test_accounts::password(crate::machines::random::<16>()),
        test_accounts::password(crate::machines::random::<16>())
    );
}

#[test]
fn test_accounts_make_an_admin_and_a_user_and_rotate_their_passwords() {
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
            "auth": { "mode": "hub" },
            "brokers": [], "grants": [], "storage": { "url": url }
        })
        .to_string(),
    )
    .unwrap();
    let config = read_config(&config_path).unwrap();
    let options = test_accounts::TestAccounts {
        state: state.clone(),
        config: config_path.clone(),
        admin_email: test_accounts::ADMIN_EMAIL.into(),
        user_email: test_accounts::USER_EMAIL.into(),
    };
    runtime.block_on(async {
        let accounts = accounts_at(&state, &config).await.unwrap();
        let first = test_accounts::make(
            &accounts,
            &config,
            &options,
            "admin pass one",
            "user pass one",
        )
        .await
        .unwrap();
        assert!(first.admin.owner, "admin is the hub's owner");
        assert!(!first.user.owner, "user is an ordinary account");
        assert_ne!(first.admin.id, first.user.id);
        assert_eq!(
            first.admin.tenant, first.user.tenant,
            "single mode gives the owner and local users one environment"
        );
        let admin = accounts
            .sign_in_password(test_accounts::ADMIN_EMAIL, "admin pass one")
            .await
            .unwrap();
        assert_eq!(admin.id, first.admin.id);
        let user = accounts
            .sign_in_password(test_accounts::USER_EMAIL, "user pass one")
            .await
            .unwrap();
        assert_eq!(user.id, first.user.id);
        let (admin_session, user_session) = (
            accounts.open_session(&admin.id, None).await.unwrap(),
            accounts.open_session(&user.id, None).await.unwrap(),
        );

        // Again: the same two accounts, new passwords, the old ones refused.
        let second = test_accounts::make(
            &accounts,
            &config,
            &options,
            "admin pass two",
            "user pass two",
        )
        .await
        .unwrap();
        assert_eq!(
            (second.admin.id.as_str(), second.user.id.as_str()),
            (first.admin.id.as_str(), first.user.id.as_str())
        );
        for (email, old, new) in [
            (
                test_accounts::ADMIN_EMAIL,
                "admin pass one",
                "admin pass two",
            ),
            (test_accounts::USER_EMAIL, "user pass one", "user pass two"),
        ] {
            assert_eq!(
                accounts
                    .sign_in_password(email, old)
                    .await
                    .unwrap_err()
                    .code,
                "password_wrong"
            );
            accounts.sign_in_password(email, new).await.unwrap();
        }
        // The owner keeps their session; the test user's ended with its password.
        assert!(accounts.session(&admin_session).await.unwrap().is_some());
        assert!(accounts.session(&user_session).await.unwrap().is_none());
        // The user's address may not be one that signs in to the owner.
        let clash = test_accounts::TestAccounts {
            user_email: test_accounts::ADMIN_EMAIL.into(),
            ..options
        };
        assert_eq!(
            test_accounts::make(&accounts, &config, &clash, "x pass one", "x pass two")
                .await
                .err()
                .unwrap()
                .code,
            "invalid_cli"
        );
        let chain = Audit::new(accounts.store().clone()).verify().await.unwrap();
        assert!(chain.ok && chain.entries == 2, "{chain:?}");
    });
}
