//! Sign-ins Nomen confirmed are kept in the hub's Postgres and honoured across a restart
//! only while Nomen cannot be reached (`nomen/kept.rs`).

use super::*;

#[tokio::test]
async fn confirmed_sign_ins_survive_a_restart_during_an_outage_and_a_sign_out_ends_them() {
    let key = test_keys::rsa();
    let fake = fake_nomen(&key, |iss| {
        let owner = active(iss, OWNER, json!(HUB_PROJECT));
        HashMap::from([
            ("tok-owner".into(), owner.clone()),
            ("tok-signed-out".into(), owner.clone()),
            (
                "tok-brief".into(),
                with(owner, "exp", json!(unix_now() + 2)),
            ),
        ])
    })
    .await;
    let store = or_skip!(crate::store::testing::postgres()).store;
    let first = verifier(&fake.issuer, &key)
        .remembering(store.clone())
        .await;
    for token in ["tok-owner", "tok-signed-out", "tok-brief"] {
        first.verify(token).await.unwrap();
    }
    drop(first);
    // The store holds hashes of tokens, never a token.
    let subjects: Vec<String> =
        sqlx::query_scalar("SELECT encode(digest, 'escape') FROM devagotchi.auth_nomen_sign_ins")
            .fetch_all(store.admin())
            .await
            .unwrap();
    assert!(
        subjects.iter().all(|digest| !digest.contains("tok-owner")),
        "a token was kept in the clear"
    );

    // Nomen goes down and the hub restarts in the middle of it.
    fake.status.store(503, SeqCst);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let mut nomen = verifier(&fake.issuer, &key)
        .remembering(store.clone())
        .await;
    nomen.retry = (Duration::from_millis(200), Duration::from_millis(400));
    let calls = fake.calls.load(SeqCst);
    let owner = nomen.verify("tok-owner").await.unwrap();
    assert_eq!(
        (owner.id.as_str(), owner.org.as_deref()),
        (OWNER, Some(OWNER_ORG))
    );
    assert_eq!(owner.name.as_deref(), Some("jessay@example.test"));
    assert_eq!(
        fake.calls.load(SeqCst),
        calls + 1,
        "Nomen is asked first, then the kept sign-in honoured"
    );
    assert!(nomen.health().honoured_during_outage >= 1);
    // Nothing is honoured past its token's own expiry, and nothing never confirmed is.
    assert!(matches!(
        nomen.verify("tok-brief").await,
        Err(AuthError::Unavailable(_))
    ));
    assert!(matches!(
        nomen.verify("tok-never-seen").await,
        Err(AuthError::Unavailable(_))
    ));

    // Nomen returns, having signed one of them out: that one ends, on disk too.
    fake.answers.lock().unwrap().remove("tok-signed-out");
    fake.status.store(200, SeqCst);
    tokio::time::sleep(nomen.retry.1).await;
    assert!(matches!(
        nomen.verify("tok-signed-out").await,
        Err(AuthError::Invalid(_))
    ));
    drop(nomen);
    fake.status.store(503, SeqCst);
    let after = verifier(&fake.issuer, &key)
        .remembering(store.clone())
        .await;
    assert!(matches!(
        after.verify("tok-signed-out").await,
        Err(AuthError::Unavailable(_))
    ));
    assert_eq!(after.verify("tok-owner").await.unwrap().id, OWNER);
}
