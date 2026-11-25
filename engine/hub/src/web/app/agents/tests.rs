use crate::{
    tenancy::tests::tenants::{Hosted, hosted_with},
    web::operator_tests_support::operator_cookie,
};
use serde_json::json;

pub(super) async fn setup() -> Option<(Hosted, String, String)> {
    let hub = hosted_with(
        None,
        |_, _| json!({"tenancy":{"mode":"single"},"operators":["github:42"]}),
    )
    .await?;
    let cookie = operator_cookie(&hub).await;
    let id = hub.accounts.owner().await.unwrap().unwrap().id;
    Some((hub, cookie, id))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_registers_issues_and_disables_agents_with_visible_live_work_over_http() {
    let (hub, cookie, owner) = or_skip!(setup());
    let fields = [
        ("account", owner.as_str()),
        ("id", "worker"),
        ("name", "<Coding agent>"),
        ("kind", "codex"),
        ("role", "worker"),
        ("wip", "2"),
        ("version", "0"),
        ("enabled", "true"),
    ];
    assert_eq!(
        hub.form("/app/agents", Some(&cookie), &fields)
            .await
            .status(),
        303
    );
    let (_, html) = Hosted::text(hub.get("/app/agents", Some(&cookie)).await).await;
    assert!(html.contains("&lt;Coding agent&gt;"));
    assert!(!html.contains("<Coding agent>"));
    let (_, editor) = Hosted::text(hub.get("/app/agents/worker", Some(&cookie)).await).await;
    assert!(editor.contains("<select name=\"kind\">"));
    assert!(editor.contains("name=\"role\" value=\"worker\" checked"));
    let (_, html) = Hosted::text(hub.get("/app/keys?agent=worker", Some(&cookie)).await).await;
    assert!(html.contains("value=\"worker\" selected"));
    let response = hub
        .form(
            "/app/keys",
            Some(&cookie),
            &[
                ("name", "Worker key"),
                ("hours", "24"),
                ("scope", "boards.read"),
                ("scope", "boards.write"),
                ("agent_id", "worker"),
            ],
        )
        .await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let (status, html) = Hosted::text(response).await;
    assert_eq!(status, 200, "{html}");
    let secret = html
        .split_once("id=\"key-secret\">")
        .unwrap()
        .1
        .split_once("</code>")
        .unwrap()
        .0
        .to_string();
    let (subject, _) = hub.accounts.verify_key(&secret).await.unwrap().unwrap();
    let agent = hub.accounts.agent(&owner, &owner, "worker").await.unwrap();
    assert_eq!(subject.agent, Some(agent.principal()));
    assert!(html.contains("Registered agent: <a href=\"/app/agents/worker\">worker</a>"));
    let human = hub
        .accounts
        .subject(&hub.accounts.owner().await.unwrap().unwrap(), "passkey");
    hub.hub.bind_project(&human, "alpha").await.unwrap();
    let (board, _, _) = hub.hub.board_access(&human, "alpha").unwrap();
    let task = board
        .create(
            &crate::agenthub::Actor::human(&owner),
            crate::agenthub::NewTicket {
                title: "Visible assigned work".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let principal = agent.principal();
    let actor = crate::agenthub::Actor {
        id: &principal,
        human: false,
        wip_limit: Some(2),
        policy: Some(devagotchi_agenthub::policy::Identity {
            kind: agent.kind.clone(),
            roles: agent.roles.clone(),
        }),
    };
    board.claim(&actor, &task.id, None).unwrap();
    let (_, html) = Hosted::text(hub.get("/app/agents/worker", Some(&cookie)).await).await;
    assert!(html.contains("Visible assigned work"), "{html}");
    assert!(html.contains("Total active tasks: 1"));
    assert!(!html.contains(&secret));
    let mut disable = fields.to_vec();
    disable.retain(|(key, _)| *key != "enabled");
    disable
        .iter_mut()
        .find(|(key, _)| *key == "version")
        .unwrap()
        .1 = "1";
    assert_eq!(
        hub.form("/app/agents", Some(&cookie), &disable)
            .await
            .status(),
        303
    );
    assert!(hub.accounts.verify_key(&secret).await.unwrap().is_none());
    let (status, html) = Hosted::text(hub.form("/app/agents", Some(&cookie), &disable).await).await;
    assert_eq!(status, 409);
    assert!(html.contains("&lt;Coding agent&gt;"));
    assert!(html.contains("name=\"version\" value=\"1\""));
    let user = hub
        .accounts
        .create_local_user(&owner, "User", "user@example.test", "a good password")
        .await
        .unwrap();
    let user_cookie = format!(
        "dg_session={}",
        hub.accounts
            .open_session(&user.account.id, None)
            .await
            .unwrap()
    );
    assert_eq!(
        hub.get("/app/agents/new", Some(&user_cookie))
            .await
            .status(),
        404
    );
    assert_eq!(
        hub.get("/app/agents/configuration", Some(&user_cookie))
            .await
            .status(),
        404
    );
    assert_eq!(
        hub.get(&format!("/app/agents?account={owner}"), Some(&user_cookie))
            .await
            .status(),
        404
    );
    assert_eq!(
        hub.form("/app/agents", Some(&user_cookie), &disable)
            .await
            .status(),
        404
    );
    let rejected = hub
        .http
        .post(format!("{}/app/agents", hub.url))
        .header("cookie", &cookie)
        .header("origin", "https://elsewhere.test")
        .form(&disable)
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_manages_roles_rules_and_kinds_without_losing_versions_or_locking_out() {
    let (hub, cookie, owner) = or_skip!(setup());
    let path = "/app/agents/configuration";
    let (status, html) = Hosted::text(hub.get(path, Some(&cookie)).await).await;
    assert_eq!(status, 200, "{html}");
    assert!(html.contains("Agent configuration"));
    let role = [
        ("operation", "role-save"),
        ("id", "qa"),
        ("description", "<Review work>"),
        ("version", "0"),
    ];
    assert_eq!(hub.form(path, Some(&cookie), &role).await.status(), 303);
    let (status, html) = Hosted::text(hub.form(path, Some(&cookie), &role).await).await;
    assert_eq!(status, 409);
    assert!(html.contains("&lt;Review work&gt;"));
    assert_eq!(
        hub.form(
            path,
            Some(&cookie),
            &[("operation", "role-delete"), ("id", "qa"), ("version", "1")]
        )
        .await
        .status(),
        303
    );
    let rule = [
        ("operation", "rule-save"),
        ("id", "blocked-label"),
        ("name", "Block protected work"),
        ("subject", "agent"),
        ("action", "task.claim:*"),
        ("projects", "alpha"),
        ("labels", "protected"),
        ("effect", "deny"),
        ("priority", "200"),
        ("enabled", "true"),
        ("version", "0"),
    ];
    assert_eq!(hub.form(path, Some(&cookie), &rule).await.status(), 303);
    let config = hub.accounts.principal_configuration(&owner).await.unwrap();
    let stored = config
        .rules
        .iter()
        .find(|r| r.id == "blocked-label")
        .unwrap();
    assert_eq!(stored.selector.labels, ["protected"]);
    assert_eq!(stored.selector.projects, ["alpha"]);
    assert_eq!(
        hub.form(
            path,
            Some(&cookie),
            &[
                ("operation", "rule-delete"),
                ("id", "blocked-label"),
                ("version", "1")
            ]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        hub.form(
            path,
            Some(&cookie),
            &[
                ("operation", "rule-delete"),
                ("id", "configure"),
                ("version", "1")
            ]
        )
        .await
        .status(),
        400
    );
    let kinds = config.settings.kinds.join("\n");
    assert_eq!(
        hub.form(
            path,
            Some(&cookie),
            &[
                ("operation", "settings"),
                ("kinds", &kinds),
                ("wip", "3"),
                ("version", "1")
            ]
        )
        .await
        .status(),
        303
    );
    assert_eq!(
        hub.accounts
            .principal_configuration(&owner)
            .await
            .unwrap()
            .settings
            .wip_limit_default,
        3
    );
}
