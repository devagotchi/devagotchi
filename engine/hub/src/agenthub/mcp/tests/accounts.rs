//! Org boards and account boards: who works them, how they are bound and counted.

use super::*;

fn hosted_config(dir: &std::path::Path, unlimited: &[&str]) -> HubConfig {
    HubConfig::parse(
        &serde_json::json!({
            "listen": "127.0.0.1:1",
            "public_url": "http://127.0.0.1:1",
            "auth": { "mode": "dev", "tokens_file": dir.join("tokens.json") },
            "owner": "300000000000000001",
            "brokers": [],
            "boards": [{ "project": "shared" }],
            "grants": [{ "subject": "300000000000000002", "boards": ["shared"] }],
            "account_boards": { "unlimited": unlimited },
        })
        .to_string(),
    )
    .expect("config")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_boards_belong_to_the_org_and_every_member_works_them() {
    use crate::identity::Subject;
    let member = |id: &str, org: Option<&str>| Subject {
        id: id.into(),
        via: "nomen",
        tenant: None,
        name: None,
        org: org.map(str::to_string),
        provisional_until: None,
        agent: None,
        claim_code: None,
        client: None,
        provisional_warning: false,
        access: None,
    };
    let (alice, bob) = (
        member("300000000000000011", Some("395263680805142530")),
        member("300000000000000012", Some("395263680805142530")),
    );
    let carol = member("300000000000000013", Some("400000000000000001"));
    let solo = member("300000000000000014", None);
    assert_eq!(Boards::owner_of(&alice), "org-395263680805142530");
    assert_eq!(Boards::owner_of(&bob), Boards::owner_of(&alice));
    assert_eq!(
        Boards::owner_of(&solo),
        solo.id,
        "no org: the account's own"
    );

    let test = or_skip!(crate::store::testing::postgres());
    let dir = tempfile::tempdir().unwrap();
    // The paid org is unlimited by its org id; Carol's org is free.
    let boards = Boards::new(
        &hosted_config(dir.path(), &["395263680805142530"]),
        test.store,
    )
    .unwrap();
    let bind = |who: &Subject, project: &str| {
        let ids: Vec<&str> = std::iter::once(who.id.as_str())
            .chain(who.org.as_deref())
            .collect();
        boards.bind_for(&Boards::owner_of(who), &ids, project, &[])
    };
    assert!(bind(&alice, "site").unwrap().created);
    assert!(
        !bind(&bob, "site").unwrap().created,
        "Bob finds the org's board"
    );
    let a = boards
        .open_account(&Boards::owner_of(&alice), "site")
        .unwrap()
        .unwrap();
    let b = boards
        .open_account(&Boards::owner_of(&bob), "site")
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&a, &b), "one board, not two");
    assert!(
        bind(&bob, "api").unwrap().created,
        "an unlimited org binds more"
    );
    assert_eq!(
        boards.account_boards(&Boards::owner_of(&alice)).unwrap(),
        vec!["api", "site"]
    );

    assert!(
        boards
            .open_account(&Boards::owner_of(&carol), "site")
            .unwrap()
            .is_none(),
        "another org never reaches it"
    );
    assert!(
        bind(&carol, "site").unwrap().created,
        "its own board of the same name"
    );
    assert_eq!(
        bind(&carol, "more").unwrap_err().code,
        "plan_limit",
        "a free org holds one"
    );
    assert!(
        boards.open_account(&solo.id, "site").unwrap().is_none(),
        "an account with no org has only its own"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_boards_are_bound_per_account_counted_and_never_named_like_a_shared_one() {
    let test = or_skip!(crate::store::testing::postgres());
    let dir = tempfile::tempdir().unwrap();
    let only_configured =
        Boards::new(&hub_config("devagotchi", dir.path()), test.store.clone()).unwrap();
    assert_eq!(
        only_configured
            .bind("someone", "theirs", &[])
            .expect_err("refused")
            .code,
        "not_offered"
    );
    assert!(!only_configured.hosts_accounts());

    let boards = Boards::new(
        &hosted_config(dir.path(), &["300000000000000003"]),
        test.store,
    )
    .unwrap();
    assert!(boards.hosts_accounts());
    let free = "300000000000000002";
    assert!(boards.account_boards(free).unwrap().is_empty());
    assert!(
        boards.open_account(free, "mine").unwrap().is_none(),
        "nothing until bound"
    );

    let bound = boards.bind(free, "mine", &["shared".into()]).unwrap();
    assert!(bound.created);
    assert_eq!(bound.boards, vec!["mine".to_string(), "shared".to_string()]);
    assert_eq!(bound.limit, Some(1));
    let again = boards.bind(free, "mine", &["shared".into()]).unwrap();
    assert!(!again.created, "binding is idempotent");
    assert_eq!(
        boards.account_boards(free).unwrap(),
        vec!["mine".to_string()]
    );
    let opened = boards.open_account(free, "mine").unwrap().expect("bound");
    assert_eq!(opened.prefix(), "mine");

    assert_eq!(
        boards.bind(free, "another", &[]).expect_err("limit").code,
        "plan_limit"
    );
    assert_eq!(
        boards
            .bind(free, "shared", &["shared".into()])
            .expect_err("taken")
            .code,
        "name_taken"
    );
    assert_eq!(
        boards
            .bind(free, "Bad Name", &[])
            .expect_err("invalid")
            .code,
        "invalid"
    );
    assert!(
        boards.open_account(free, "../etc").unwrap().is_none(),
        "a bad name opens nothing"
    );

    // The owner and the unlimited list bind as many as they like.
    for subject in ["300000000000000001", "300000000000000003"] {
        for project in ["a", "b", "c"] {
            let bound = boards.bind(subject, project, &[]).unwrap();
            assert!(
                bound.created && bound.limit.is_none(),
                "{subject} {project}"
            );
        }
        assert_eq!(boards.account_boards(subject).unwrap(), vec!["a", "b", "c"]);
    }
    // Another account's board is not this account's, whatever the name.
    assert!(boards.open_account(free, "a").unwrap().is_none());
    assert!(!boards.is_empty());
}
