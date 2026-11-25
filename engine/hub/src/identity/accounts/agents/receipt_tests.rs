use super::*;
use devagotchi_agenthub::policy::{Effect, Rule, Selector};

#[tokio::test]
async fn role_and_policy_retries_preserve_original_results_and_current_authority() {
    let (accounts, owner) = or_skip!(super::tests::setup());
    let role = catalog::Role {
        id: "receipt-reviewer".into(),
        description: "Original".into(),
        version: 0,
    };
    let (one, two) = tokio::join!(
        accounts.put_principal_role_requested(&owner.id, &role, Some("role-create")),
        accounts.put_principal_role_requested(&owner.id, &role, Some("role-create")),
    );
    let (one, two) = (one.unwrap(), two.unwrap());
    assert_ne!(one.replayed, two.replayed);
    assert_eq!(one.value.version, 1);
    let later = catalog::Role {
        description: "Later".into(),
        version: 1,
        ..role.clone()
    };
    accounts
        .put_principal_role(&owner.id, &later)
        .await
        .unwrap();
    let replay = accounts
        .put_principal_role_requested(&owner.id, &role, Some("role-create"))
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.value.description, "Original");
    assert_eq!(
        accounts
            .put_principal_role_requested(&owner.id, &later, Some("role-create"))
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    accounts
        .delete_principal_role_requested(&owner.id, "receipt-reviewer", 2, Some("role-delete"))
        .await
        .unwrap();
    assert!(
        accounts
            .delete_principal_role_requested(&owner.id, "receipt-reviewer", 2, Some("role-delete"))
            .await
            .unwrap()
            .replayed
    );

    let rule = Rule {
        id: "receipt-test".into(),
        name: "Test".into(),
        subject: "role:admin".into(),
        action: "task.read".into(),
        selector: Selector::default(),
        effect: Effect::Allow,
        priority: 50,
        enabled: true,
        version: 0,
    };
    accounts
        .put_principal_rule_requested(&owner.id, &rule, Some("rule-create"))
        .await
        .unwrap();
    accounts
        .delete_principal_rule_requested(&owner.id, &rule.id, 1, Some("rule-delete"))
        .await
        .unwrap();
    assert!(
        accounts
            .delete_principal_rule_requested(&owner.id, &rule.id, 1, Some("rule-delete"))
            .await
            .unwrap()
            .replayed
    );
    assert!(
        accounts
            .put_principal_rule_requested(&owner.id, &rule, Some("rule-create"))
            .await
            .unwrap()
            .replayed
    );
    assert!(
        !accounts
            .principal_configuration(&owner.id)
            .await
            .unwrap()
            .rules
            .iter()
            .any(|r| r.id == rule.id)
    );
    let deny = Rule {
        id: "deny-role-put".into(),
        action: "admin.roles.put".into(),
        effect: Effect::Deny,
        priority: 10000,
        ..rule
    };
    accounts.put_principal_rule(&owner.id, &deny).await.unwrap();
    assert_eq!(
        accounts
            .put_principal_role_requested(&owner.id, &role, Some("role-create"))
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
}

#[tokio::test]
async fn profile_retry_does_not_restore_old_authority_or_cross_accounts() {
    let (accounts, owner) = or_skip!(super::tests::setup());
    let user = accounts
        .create_local_user(
            &owner.id,
            "User",
            "receipt-user@example.test",
            "a good password",
        )
        .await
        .unwrap()
        .account;
    let edit = AgentEdit {
        id: "codex".into(),
        name: "Original".into(),
        kind: "codex".into(),
        roles: vec!["worker".into()],
        enabled: true,
        wip_limit: None,
        version: 0,
    };
    accounts
        .put_agent_requested(&owner.id, &user.id, &edit, Some("profile-create"))
        .await
        .unwrap();
    let rename = AgentEdit {
        name: "Renamed".into(),
        version: 1,
        ..edit.clone()
    };
    accounts
        .put_agent_requested(&user.id, &user.id, &rename, Some("rename"))
        .await
        .unwrap();
    let disabled = AgentEdit {
        name: "Disabled later".into(),
        enabled: false,
        version: 2,
        ..edit.clone()
    };
    accounts
        .put_agent(&owner.id, &user.id, &disabled)
        .await
        .unwrap();
    let replay = accounts
        .put_agent_requested(&user.id, &user.id, &rename, Some("rename"))
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.value.name, "Renamed");
    assert!(replay.value.enabled);
    assert!(
        !accounts
            .agent(&owner.id, &user.id, &edit.id)
            .await
            .unwrap()
            .enabled
    );
    assert_eq!(
        accounts
            .put_agent_requested(&owner.id, &owner.id, &edit, Some("profile-create"))
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    assert_eq!(
        accounts
            .put_agent_requested(&user.id, &owner.id, &rename, Some("rename"))
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    // Only the two keyed writes reserved receipts; retries created no additional rows.
    let count: i64 = transaction!(&accounts.store, |connection| {
        accounts.store.scope(connection, &owner.tenant).await?;
        Ok(sqlx::query_scalar("SELECT count(*) FROM store_request_receipts WHERE tenant_id=$1 AND operation='put_agent'").bind(&owner.tenant).fetch_one(connection).await?)
    }).unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn settings_retry_preserves_later_configuration() {
    let (accounts, owner) = or_skip!(super::tests::setup());
    let mut settings = accounts
        .principal_configuration(&owner.id)
        .await
        .unwrap()
        .settings;
    settings.wip_limit_default = 7;
    let original = accounts
        .put_principal_settings_requested(&owner.id, &settings, Some("settings"))
        .await
        .unwrap();
    let mut later = original.value.clone();
    later.wip_limit_default = 9;
    accounts
        .put_principal_settings(&owner.id, &later)
        .await
        .unwrap();
    let replay = accounts
        .put_principal_settings_requested(&owner.id, &settings, Some("settings"))
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.value.wip_limit_default, 7);
    assert_eq!(
        accounts
            .principal_configuration(&owner.id)
            .await
            .unwrap()
            .settings
            .wip_limit_default,
        9
    );
}
