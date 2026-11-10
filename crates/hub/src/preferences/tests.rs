//! A person's preferences on SQLite, and on Postgres under row-level security when the test
//! server is set (CI).

use serde_json::json;

use super::*;
use crate::store::testing;

async fn tenant(store: &Store) -> String {
    let id = crate::accounts::tenants::new_tenant_id(crate::link::random::<16>());
    transaction!(store, |connection| {
        store.scope(connection, &id).await?;
        sqlx::query("INSERT INTO tenancy_tenants (id, created_at) VALUES ($1, 1)")
            .bind(&id)
            .execute(&mut *connection)
            .await?;
        Ok(())
    })
    .unwrap();
    id
}

async fn chain(store: &Store, tenant: &str) -> Vec<(String, String, String)> {
    transaction!(store, |connection| {
        store.scope(connection, tenant).await?;
        let rows = sqlx::query(
            "SELECT actor, event, detail FROM tenancy_events WHERE tenant_id = $1 ORDER BY seq",
        )
        .bind(tenant)
        .fetch_all(&mut *connection)
        .await?;
        let mut all = Vec::new();
        for row in rows {
            all.push((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?));
        }
        Ok(all)
    })
    .unwrap()
}

async fn holds(store: Store) {
    let preferences = PreferenceStore::new(store.clone());
    let (a, b) = (tenant(&store).await, tenant(&store).await);

    // Nobody set anything: the defaults, and no row or event.
    let fresh = preferences.get(&a, "alice").await.unwrap();
    assert_eq!(fresh, Preferences::default());
    assert!(chain(&store, &a).await.is_empty());
    assert!(preferences.machine_pushes("alice").await.unwrap());

    // Saved: read back; the chain names what changed, and a name only by that fact.
    let mine = Preferences {
        name: Some("Alice Doe".into()),
        theme: Theme::Dark,
        allow_days: 7,
        offer_ahead: false,
        always_ask: vec!["deploy".into()],
        machine_pushes: false,
        ..Preferences::default()
    };
    let saved = preferences.save(&a, "alice", &mine).await.unwrap();
    assert_eq!(saved.added_always_ask, ["deploy"]);
    assert_eq!(preferences.get(&a, "alice").await.unwrap(), mine);
    let events = chain(&store, &a).await;
    assert_eq!(events.len(), 1, "{events:?}");
    let (actor, event, detail) = &events[0];
    assert_eq!((actor.as_str(), event.as_str()), ("alice", EVENT));
    assert_eq!(
        detail,
        "name; theme: dark; allow_days: 7; offer_ahead: false; always_ask: deploy; machine_pushes: false"
    );
    assert!(!detail.contains("Alice"), "{detail}");

    // Saved unchanged: nothing written, nothing on the chain, nothing newly added.
    let again = preferences.save(&a, "alice", &mine).await.unwrap();
    assert!(again.added_always_ask.is_empty());
    assert_eq!(chain(&store, &a).await.len(), 1);

    // Only what was added is reported.
    let more = Preferences {
        always_ask: vec!["deploy".into(), "rotate".into()],
        ..mine.clone()
    };
    let saved = preferences.save(&a, "alice", &more).await.unwrap();
    assert_eq!(saved.added_always_ask, ["rotate"]);

    // One person's row is theirs: another tenant and another person see the defaults.
    assert_eq!(
        preferences.get(&b, "alice").await.unwrap(),
        Preferences::default()
    );
    assert_eq!(
        preferences.get(&a, "bob").await.unwrap(),
        Preferences::default()
    );

    // A machine asks without knowing the tenant.
    assert!(!preferences.machine_pushes("alice").await.unwrap());
    assert!(preferences.machine_pushes("bob").await.unwrap());
    assert_eq!(
        wanting_machine_pushes(Some(&preferences), vec!["alice".into(), "bob".into()]).await,
        ["bob"]
    );
    assert_eq!(
        wanting_machine_pushes(None, vec!["alice".into()]).await,
        ["alice"]
    );

    // What a newer hub stored stays through this hub's save, and its number with it.
    transaction!(&store, |connection| {
        store.scope(connection, &b).await?;
        sqlx::query(
            "INSERT INTO person_preferences
               (tenant_id, subject, schema_version, body, machine_pushes, updated_at)
             VALUES ($1, 'carol', 9, $2, 1, 1)",
        )
        .bind(&b)
        .bind(json!({ "schema": 9, "theme": "light", "quiet_hours": { "from": 22 } }).to_string())
        .execute(&mut *connection)
        .await?;
        Ok(())
    })
    .unwrap();
    let read = preferences.get(&b, "carol").await.unwrap();
    assert_eq!((read.schema, read.theme), (9, Theme::Light));
    let next = Preferences {
        allow_days: 90,
        ..read
    };
    let kept = preferences
        .save(&b, "carol", &next)
        .await
        .unwrap()
        .preferences;
    assert_eq!(kept.schema, 9);
    assert_eq!(kept.extra["quiet_hours"], json!({ "from": 22 }));
    let read = preferences.get(&b, "carol").await.unwrap();
    assert_eq!(read.extra["quiet_hours"], json!({ "from": 22 }));
    assert_eq!(read.allow_days, 90);

    // Refused choices write nothing.
    let bad = Preferences {
        allow_days: 45,
        ..Preferences::default()
    };
    assert_eq!(
        preferences.save(&a, "dan", &bad).await.unwrap_err().code,
        "invalid"
    );
    assert_eq!(
        preferences.get(&a, "dan").await.unwrap(),
        Preferences::default()
    );
}

#[tokio::test]
async fn preferences_hold_on_sqlite() {
    let test = testing::sqlite().await;
    holds(test.store.clone()).await;
}

#[tokio::test]
async fn preferences_hold_on_postgres_under_row_level_security() {
    if let Some(test) = testing::postgres().await {
        holds(test.store.clone()).await;
    }
}

#[test]
fn an_old_row_is_read_in_todays_shape_and_a_broken_one_as_the_defaults() {
    // Before the number existed.
    let old = upgrade(json!({ "theme": "dark", "allow_days": 7 }));
    assert_eq!(
        (old.schema, old.theme, old.allow_days),
        (SCHEMA, Theme::Dark, 7)
    );
    assert!(old.offer_ahead, "what it did not say has its default");
    assert_eq!(stored("not json"), Preferences::default());
    assert_eq!(stored("{\"theme\":\"mauve\"}"), Preferences::default());
}

#[test]
fn a_persons_choices_are_checked() {
    let ok = Preferences::default();
    assert!(ok.validate().is_ok());
    let mut bad: Vec<Preferences> = Vec::new();
    for days in [0, 45, 365] {
        bad.push(Preferences {
            allow_days: days,
            ..ok.clone()
        });
    }
    for name in ["  ".to_string(), "a\nb".to_string(), "x".repeat(41)] {
        bad.push(Preferences {
            name: Some(name),
            ..ok.clone()
        });
    }
    bad.push(Preferences {
        always_ask: vec!["Not A Kind!".into()],
        ..ok.clone()
    });
    bad.push(Preferences {
        always_ask: (0..31).map(|n| format!("k{n}")).collect(),
        ..ok.clone()
    });
    for preferences in bad {
        assert_eq!(
            preferences.validate().unwrap_err().code,
            "invalid",
            "{preferences:?}"
        );
    }
    assert_eq!(
        parse_kinds(" Deploy, rotate  deploy\nedit-file,"),
        ["deploy", "rotate", "edit-file"]
    );
}
