use crate::identity::web::agents::tests::setup;
use crate::{
    identity::accounts::{agents::AgentEdit, oauth::s256},
    tenancy::tests::tenants::Hosted,
};

fn profile(id: &str) -> AgentEdit {
    AgentEdit {
        id: id.into(),
        name: id.into(),
        kind: "codex".into(),
        roles: vec!["worker".into()],
        enabled: true,
        wip_limit: Some(0),
        version: 0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_binds_stable_oauth_clients_and_rejects_stale_or_disabled_connections() {
    let (hub, cookie, owner) = or_skip!(setup());
    let agent = hub
        .accounts
        .put_agent(&owner, &owner, &profile("worker"))
        .await
        .unwrap();
    hub.accounts
        .put_agent(&owner, &owner, &profile("reviewer"))
        .await
        .unwrap();
    let redirect = "http://127.0.0.1/callback";
    let resource = format!("{}/mcp", hub.url);
    let verifier = "v".repeat(43);
    let mut clients = Vec::new();
    let mut tokens = Vec::new();
    for _ in 0..2 {
        let client = hub
            .accounts
            .register_client("Same <client>", &[redirect.into()])
            .await
            .unwrap();
        let code = hub
            .accounts
            .issue_code(
                &client.id,
                &owner,
                redirect,
                &s256(&verifier),
                &resource,
                "mcp offline_access",
            )
            .await
            .unwrap();
        tokens.push(
            hub.accounts
                .exchange_code(&code, &client.id, redirect, &verifier, Some(&resource))
                .await
                .unwrap(),
        );
        clients.push(client);
    }
    let (status, html) = Hosted::text(hub.get("/app/agents/worker", Some(&cookie)).await).await;
    assert_eq!(status, 200);
    assert!(html.contains("Same &lt;client&gt;"));
    for client in &clients {
        assert!(html.contains(&client.id));
    }
    let path = "/app/agents/worker/oauth";
    let fields = [
        ("account", owner.as_str()),
        ("client", clients[0].id.as_str()),
        ("request_id", "bind-client"),
        ("version", "0"),
        ("enabled", "true"),
    ];
    assert_eq!(hub.form(path, Some(&cookie), &fields).await.status(), 303);
    assert_eq!(
        hub.accounts
            .verify_access(&tokens[0].access)
            .await
            .unwrap()
            .unwrap()
            .agent,
        Some(agent.principal())
    );
    assert_eq!(
        hub.accounts
            .verify_access(&tokens[1].access)
            .await
            .unwrap()
            .unwrap()
            .agent,
        None
    );
    // An old form on a different agent cannot silently transfer this client's sessions.
    assert_eq!(
        hub.form("/app/agents/reviewer/oauth", Some(&cookie), &fields)
            .await
            .status(),
        409
    );
    let (_, html) = Hosted::text(hub.get("/app/agents/reviewer", Some(&cookie)).await).await;
    assert!(
        html.contains("Currently assigned to: worker (Enabled)"),
        "{html}"
    );
    let fresh = hub
        .accounts
        .refresh(&tokens[0].refresh, &clients[0].id, Some(&resource))
        .await
        .unwrap();
    let disabled = [
        ("account", owner.as_str()),
        ("client", clients[0].id.as_str()),
        ("request_id", "disable-client"),
        ("version", "1"),
    ];
    assert_eq!(hub.form(path, Some(&cookie), &disabled).await.status(), 303);
    assert!(
        hub.accounts
            .verify_access(&fresh.access)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        hub.accounts
            .refresh(&fresh.refresh, &clients[0].id, Some(&resource))
            .await
            .unwrap_err()
            .code,
        "invalid_grant"
    );
    assert_eq!(hub.form(path, Some(&cookie), &disabled).await.status(), 303);
    // Replaying the old enabled binding returns its receipt without re-enabling it.
    assert_eq!(hub.form(path, Some(&cookie), &fields).await.status(), 303);
    assert!(
        !hub.accounts
            .agent_oauth_bindings(&owner, &owner)
            .await
            .unwrap()[0]
            .enabled
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn roster_reports_authorized_total_without_hidden_task_or_board_details() {
    let (hub, cookie, owner) = or_skip!(setup());
    let agent = hub
        .accounts
        .put_agent(&owner, &owner, &profile("worker"))
        .await
        .unwrap();
    let user = hub
        .accounts
        .create_local_user(&owner, "User", "hidden@example.test", "a good password")
        .await
        .unwrap();
    let viewer = hub
        .accounts
        .subject(&hub.accounts.owner().await.unwrap().unwrap(), "passkey");
    let other = hub.accounts.subject(&user.account, "passkey");
    hub.hub.bind_project(&viewer, "visible").await.unwrap();
    hub.hub
        .bind_project(&other, "hidden-project")
        .await
        .unwrap();
    let principal = agent.principal();
    let actor = crate::agenthub::Actor {
        id: &principal,
        human: false,
        wip_limit: Some(0),
        policy: Some(devagotchi_agenthub::policy::Identity {
            kind: agent.kind,
            roles: agent.roles,
        }),
    };
    for (subject, project, title) in [
        (&viewer, "visible", "Visible task"),
        (&other, "hidden-project", "Private task title"),
    ] {
        let (board, _, _) = hub.hub.board_access(subject, project).unwrap();
        let ticket = board
            .create(
                &crate::agenthub::Actor::human(&subject.id),
                crate::agenthub::NewTicket {
                    title: title.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        // A live lease may outlast the viewer's permission to read its board.
        board.claim(&actor, &ticket.id, None).unwrap();
    }
    let (board, _, _) = hub.hub.board_access(&viewer, "visible").unwrap();
    let task = board
        .create(
            &crate::agenthub::Actor::human(&owner),
            crate::agenthub::NewTicket {
                title: "Delegated task".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let path = format!("/app/tickets/visible/{}", task.id);
    let (status, html) = Hosted::text(hub.get(&path, Some(&cookie)).await).await;
    assert_eq!(status, 200);
    assert!(
        html.contains(&format!("value=\"{principal}\" label=\"worker\"")),
        "{html}"
    );
    let version = task.version.to_string();
    assert_eq!(
        hub.form(
            &format!("{path}/delegate"),
            Some(&cookie),
            &[("version", version.as_str()), ("to", principal.as_str())]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        board.get(&task.id).unwrap().ticket.assignee.as_deref(),
        Some(principal.as_str())
    );
    assert!(hub.hub.board_access(&viewer, "hidden-project").is_err());
    let (status, html) = Hosted::text(hub.get("/app/agents/worker", Some(&cookie)).await).await;
    assert_eq!(status, 200);
    assert!(html.contains("Total active tasks: 2"));
    assert!(html.contains("Visible task"));
    assert!(!html.contains("Private task title"));
    assert!(!html.contains("hidden-project"));
}
