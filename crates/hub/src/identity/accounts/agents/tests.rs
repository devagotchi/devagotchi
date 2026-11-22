use super::*;
use crate::identity::accounts::api_keys::{NewKey, Scope};

pub(super) async fn setup() -> Option<(Accounts, Account)> {
    let test = crate::store::testing::postgres().await?;
    let accounts = Accounts::open(test.store).await.unwrap();
    let owner = accounts
        .ensure_owner(None, "Owner", [71; 16])
        .await
        .unwrap();
    Some((accounts, owner))
}

fn edit() -> AgentEdit {
    AgentEdit {
        id: "codex".into(),
        name: "Coding agent".into(),
        kind: "codex".into(),
        roles: vec!["worker".into()],
        enabled: true,
        wip_limit: None,
        version: 0,
    }
}

fn key(agent: &str) -> NewKey {
    NewKey {
        name: "Code".into(),
        scopes: vec![Scope::BoardsRead, Scope::BoardsWrite],
        projects: vec!["alpha".into()],
        hours: 24,
        agent_id: Some(agent.into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn registered_agents_are_versioned_account_bound_and_never_self_escalate() {
    let (accounts, owner) = or_skip!(setup());
    let a = accounts
        .create_local_user(&owner.id, "A", "a@example.test", "a good password")
        .await
        .unwrap()
        .account;
    let b = accounts
        .create_local_user(&owner.id, "B", "b@example.test", "a good password")
        .await
        .unwrap()
        .account;
    let made = accounts.put_agent(&owner.id, &a.id, &edit()).await.unwrap();
    let other = accounts.put_agent(&owner.id, &b.id, &edit()).await.unwrap();
    assert_ne!(made.principal(), other.principal());
    assert_eq!(
        accounts
            .put_agent(&a.id, &a.id, &edit())
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert_eq!(
        accounts
            .agent(&a.id, &b.id, "codex")
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    let mut renamed = edit();
    renamed.version = made.version;
    renamed.name = "My agent".into();
    let updated = accounts.put_agent(&a.id, &a.id, &renamed).await.unwrap();
    assert_eq!(updated.version, 2);
    assert_eq!(
        accounts
            .put_agent(&a.id, &a.id, &renamed)
            .await
            .unwrap_err()
            .code,
        "version_conflict"
    );
    renamed.version = 2;
    renamed.kind = "service".into();
    assert_eq!(
        accounts
            .put_agent(&a.id, &a.id, &renamed)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    renamed.kind = "codex".into();
    renamed.roles.push("admin".into());
    assert_eq!(
        accounts
            .put_agent(&a.id, &a.id, &renamed)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    assert_eq!(
        accounts.agent(&owner.id, &a.id, "codex").await.unwrap(),
        updated
    );
    assert_eq!(
        accounts
            .delete_principal_role(&owner.id, "worker", 1)
            .await
            .unwrap_err()
            .code,
        "in_use"
    );
    let mut settings = accounts
        .principal_configuration(&owner.id)
        .await
        .unwrap()
        .settings;
    settings.kinds.retain(|kind| kind != "codex");
    assert_eq!(
        accounts
            .put_principal_settings(&owner.id, &settings)
            .await
            .unwrap_err()
            .code,
        "in_use"
    );
    let (_, limit) = accounts
        .live_agent(&owner.tenant, &a.id, &made.principal())
        .await
        .unwrap();
    assert_eq!(limit, 2);
}

#[tokio::test]
async fn bound_keys_share_identity_preserve_binding_and_stop_when_agent_is_disabled() {
    let (accounts, owner) = or_skip!(setup());
    let mut profile = edit();
    let agent = accounts
        .put_agent(&owner.id, &owner.id, &profile)
        .await
        .unwrap();
    let (one, secret1) = accounts
        .create_key(&owner.tenant, &owner.id, &key(&agent.id))
        .await
        .unwrap();
    let (_, secret2) = accounts
        .create_key(&owner.tenant, &owner.id, &key(&agent.id))
        .await
        .unwrap();
    for secret in [&secret1, &secret2] {
        let (subject, limits) = accounts.verify_key(secret).await.unwrap().unwrap();
        assert_eq!(subject.agent, Some(agent.principal()));
        assert_eq!(subject.via, "key");
        assert_eq!(limits.projects, ["alpha"]);
    }
    let seen = accounts
        .agent(&owner.id, &owner.id, &agent.id)
        .await
        .unwrap();
    assert!(seen.last_seen.is_some());
    accounts.verify_key(&secret2).await.unwrap().unwrap();
    assert_eq!(
        accounts
            .agent(&owner.id, &owner.id, &agent.id)
            .await
            .unwrap()
            .last_seen,
        seen.last_seen
    );
    let (rotated, replacement) = accounts
        .rotate_key(&owner.tenant, &owner.id, &one.id)
        .await
        .unwrap();
    assert_eq!(rotated.agent_id, Some(agent.id.clone()));
    assert!(accounts.verify_key(&secret1).await.unwrap().is_none());
    assert_eq!(
        accounts
            .verify_key(&replacement)
            .await
            .unwrap()
            .unwrap()
            .0
            .agent,
        Some(agent.principal())
    );
    profile.version = 1;
    profile.enabled = false;
    accounts
        .put_agent(&owner.id, &owner.id, &profile)
        .await
        .unwrap();
    assert!(accounts.verify_key(&replacement).await.unwrap().is_none());
    assert!(accounts.verify_key(&secret2).await.unwrap().is_none());
    assert!(
        accounts
            .rotate_key(&owner.tenant, &owner.id, &rotated.id)
            .await
            .is_err()
    );
    assert!(
        accounts
            .keys(&owner.tenant, &owner.id)
            .await
            .unwrap()
            .iter()
            .find(|key| key.id == rotated.id)
            .unwrap()
            .revoked_at
            .is_none()
    );
}

#[tokio::test]
async fn oauth_binding_uses_client_id_across_refreshes_and_revocation_cannot_unbind() {
    let (accounts, owner) = or_skip!(setup());
    let agent = accounts
        .put_agent(&owner.id, &owner.id, &edit())
        .await
        .unwrap();
    let redirect = "http://127.0.0.1/callback";
    let resource = "http://127.0.0.1/mcp";
    let verifier = "a".repeat(43);
    let first = accounts
        .register_client("Same name", &[redirect.into()])
        .await
        .unwrap();
    let second = accounts
        .register_client("Same name", &[redirect.into()])
        .await
        .unwrap();
    let mut tokens = Vec::new();
    for client in [&first, &second] {
        let code = accounts
            .issue_code(
                &client.id,
                &owner.id,
                redirect,
                &oauth::s256(&verifier),
                resource,
                "mcp offline_access",
            )
            .await
            .unwrap();
        tokens.push(
            accounts
                .exchange_code(&code, &client.id, redirect, &verifier, Some(resource))
                .await
                .unwrap(),
        );
    }
    let binding = OAuthBinding {
        client: first.id.clone(),
        agent_id: agent.id.clone(),
        enabled: true,
        version: 0,
    };
    let saved = accounts
        .bind_agent_oauth(&owner.id, &owner.id, &binding)
        .await
        .unwrap();
    assert_eq!(
        accounts
            .verify_access(&tokens[0].access)
            .await
            .unwrap()
            .unwrap()
            .agent,
        Some(agent.principal())
    );
    assert_eq!(
        accounts
            .verify_access(&tokens[1].access)
            .await
            .unwrap()
            .unwrap()
            .agent,
        None
    );
    let fresh = accounts
        .refresh(&tokens[0].refresh, &first.id, Some(resource))
        .await
        .unwrap();
    assert_eq!(
        accounts
            .verify_access(&fresh.access)
            .await
            .unwrap()
            .unwrap()
            .agent,
        Some(agent.principal())
    );
    // Profile disablement stops minting, not only access-token use.
    let mut disabled = edit();
    disabled.version = agent.version;
    disabled.enabled = false;
    let disabled = accounts
        .put_agent(&owner.id, &owner.id, &disabled)
        .await
        .unwrap();
    assert_eq!(
        accounts
            .refresh(&fresh.refresh, &first.id, Some(resource))
            .await
            .unwrap_err()
            .code,
        "invalid_grant"
    );
    let mut enabled = edit();
    enabled.version = disabled.version;
    accounts
        .put_agent(&owner.id, &owner.id, &enabled)
        .await
        .unwrap();
    // A denied refresh did not consume the still-live credential.
    let fresh = accounts
        .refresh(&fresh.refresh, &first.id, Some(resource))
        .await
        .unwrap();
    let revoked = OAuthBinding {
        enabled: false,
        ..saved
    };
    accounts
        .bind_agent_oauth(&owner.id, &owner.id, &revoked)
        .await
        .unwrap();
    assert_eq!(
        accounts
            .refresh(&fresh.refresh, &first.id, Some(resource))
            .await
            .unwrap_err()
            .code,
        "invalid_grant"
    );
    let code = accounts
        .issue_code(
            &first.id,
            &owner.id,
            redirect,
            &oauth::s256(&verifier),
            resource,
            "mcp offline_access",
        )
        .await
        .unwrap();
    assert_eq!(
        accounts
            .exchange_code(&code, &first.id, redirect, &verifier, Some(resource))
            .await
            .unwrap_err()
            .code,
        "invalid_grant"
    );
    // The unrelated client with the identical display name retains its own authority.
    let second_fresh = accounts
        .refresh(&tokens[1].refresh, &second.id, Some(resource))
        .await
        .unwrap();
    assert!(
        accounts
            .verify_access(&fresh.access)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        accounts
            .verify_access(&tokens[0].access)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        accounts
            .verify_access(&second_fresh.access)
            .await
            .unwrap()
            .is_some()
    );
}
