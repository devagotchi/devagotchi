//! Corrupt restrictions refuse authentication and replacement without changing authority.

use super::*;

#[tokio::test]
async fn malformed_key_restrictions_never_become_unrestricted() {
    let test = or_skip!(crate::store::testing::postgres());
    let accounts = Accounts::open(test.store.clone())
        .await
        .unwrap()
        .with_tenancy(
            crate::config::TenancyConfig {
                mode: crate::config::TenancyMode::Multi,
                ..Default::default()
            },
            Vec::new(),
        );
    let account = accounts
        .sign_up_password("restricted@example.com", "a good password")
        .await
        .unwrap();
    let new = NewKey {
        name: "restricted".into(),
        scopes: vec![Scope::BoardsRead],
        projects: vec!["alpha".into()],
        machines: vec!["desk".into()],
        agent_id: None,
        hours: 24,
    };
    let (key, secret) = accounts
        .create_key(&account.tenant, &account.id, &new)
        .await
        .unwrap();
    let events = accounts.events(&account.tenant, 100).await.unwrap().len();
    let too_many = serde_json::to_string(&vec!["alpha"; MAX_NAMED + 1]).unwrap();
    let too_long = serde_json::to_string(&vec!["x".repeat(129)]).unwrap();
    for column in ["scopes", "projects", "machines"] {
        let mut invalid = vec!["not-json", "null", "{}", "[1]", "[\"\"]"];
        if column == "scopes" {
            invalid.extend(["[]", "[\"unknown.scope\"]"]);
        } else {
            invalid.extend(["[\"bad\\nname\"]", &too_many, &too_long]);
        }
        for raw in invalid {
            // The column is one of the three literal names above; values remain bound.
            let sql =
                format!("UPDATE auth_api_keys SET {column} = $1 WHERE tenant_id = $2 AND id = $3");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(raw)
                .bind(&account.tenant)
                .bind(&key.id)
                .execute(test.store.admin())
                .await
                .unwrap();
            for error in [
                accounts.verify_key(&secret).await.unwrap_err(),
                accounts
                    .keys(&account.tenant, &account.id)
                    .await
                    .unwrap_err(),
                accounts
                    .rotate_key(&account.tenant, &account.id, &key.id)
                    .await
                    .unwrap_err(),
            ] {
                assert!(!error.message.contains(&secret));
                assert!(!error.message.contains("unknown.scope"));
            }
            let unchanged: (i64, i64, i64) = sqlx::query_as(
                "SELECT COUNT(*), COUNT(revoked_at), COUNT(last_used_at) FROM auth_api_keys WHERE tenant_id = $1"
            ).bind(&account.tenant).fetch_one(test.store.admin()).await.unwrap();
            assert_eq!(unchanged, (1, 0, 0), "{column}: {raw}");
            assert_eq!(
                accounts.events(&account.tenant, 100).await.unwrap().len(),
                events
            );
        }
        let original = match column {
            "scopes" => serde_json::to_string(&new.scopes).unwrap(),
            "projects" => serde_json::to_string(&new.projects).unwrap(),
            _ => serde_json::to_string(&new.machines).unwrap(),
        };
        let sql =
            format!("UPDATE auth_api_keys SET {column} = $1 WHERE tenant_id = $2 AND id = $3");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(original)
            .bind(&account.tenant)
            .bind(&key.id)
            .execute(test.store.admin())
            .await
            .unwrap();
    }
    let (_, limits) = accounts.verify_key(&secret).await.unwrap().unwrap();
    assert_eq!(limits.projects, new.projects);
    assert_eq!(limits.machines, new.machines);
    let (rotated, fresh) = accounts
        .rotate_key(&account.tenant, &account.id, &key.id)
        .await
        .unwrap();
    assert_eq!(rotated.scopes, new.scopes);
    assert_eq!(rotated.projects, new.projects);
    assert_eq!(rotated.machines, new.machines);
    assert!(accounts.verify_key(&secret).await.unwrap().is_none());
    assert!(accounts.verify_key(&fresh).await.unwrap().is_some());
}
