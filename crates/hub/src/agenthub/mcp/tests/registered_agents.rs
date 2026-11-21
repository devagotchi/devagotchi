use crate::{
    identity::accounts::{
        agents::AgentEdit,
        api_keys::{NewKey, Scope},
    },
    tenancy::tests::tenants::{Person, hosted_with, refused},
};
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_credentials_share_fences_capacity_and_live_revocation_on_existing_mcp_sessions() {
    let hub = or_skip!(hosted_with(
        None,
        |_, _| json!({"tenancy":{"mode":"single"},"boards":[{"project":"shared"}]})
    ));
    let owner = hub
        .accounts
        .ensure_owner(None, "Owner", [77; 16])
        .await
        .unwrap();
    hub.config.learn_owner(&owner.id).unwrap();
    let mut edit = AgentEdit {
        id: "worker".into(),
        name: "Worker".into(),
        kind: "codex".into(),
        roles: vec!["worker".into()],
        enabled: true,
        wip_limit: Some(1),
        version: 0,
    };
    let agent = hub
        .accounts
        .put_agent(&owner.id, &owner.id, &edit)
        .await
        .unwrap();
    let new = NewKey {
        name: "Setup".into(),
        scopes: Scope::ALL.to_vec(),
        hours: 24,
        ..Default::default()
    };
    let (_, setup_secret) = hub
        .accounts
        .create_key(&owner.tenant, &owner.id, &new)
        .await
        .unwrap();
    let mut setup = Person {
        cookie: String::new(),
        token: setup_secret,
        session: None,
    };
    for project in ["alpha", "beta"] {
        let result = hub
            .call(&mut setup, "board_bind", json!({"project":project}))
            .await;
        assert!(!refused(&result), "{result}");
    }
    let bound = NewKey {
        agent_id: Some(agent.id.clone()),
        ..new
    };
    let (_, first) = hub
        .accounts
        .create_key(&owner.tenant, &owner.id, &bound)
        .await
        .unwrap();
    let (_, second) = hub
        .accounts
        .create_key(&owner.tenant, &owner.id, &bound)
        .await
        .unwrap();
    let mut a = Person {
        cookie: String::new(),
        token: first,
        session: None,
    };
    let mut b = Person {
        cookie: String::new(),
        token: second,
        session: None,
    };
    let mut ids = Vec::new();
    for project in ["alpha", "shared"] {
        let made = hub
            .call(
                &mut a,
                "board_create",
                json!({"project":project,"title":"Work"}),
            )
            .await;
        assert!(!refused(&made), "{made}");
        ids.push(
            made["structuredContent"]["id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    let claim = hub
        .call(
            &mut a,
            "board_claim",
            json!({"project":"alpha","id":ids[0]}),
        )
        .await;
    assert!(!refused(&claim), "{claim}");
    assert_eq!(
        claim["structuredContent"]["lease"]["ticket"]["holder"],
        agent.principal()
    );
    let denied = hub
        .call(
            &mut b,
            "board_claim",
            json!({"project":"shared","id":ids[1]}),
        )
        .await;
    assert!(refused(&denied), "{denied}");
    assert_eq!(denied["structuredContent"]["code"], "wip_limit");
    // A different valid seat header and another credential still use the same fence memory.
    let response = hub
        .http
        .post(format!("{}/mcp", hub.url))
        .bearer_auth(&b.token)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", b.session.as_deref().unwrap())
        .header("x-devagotchi-seat", "changed-seat")
        .json(
            &json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{
                "name":"board_heartbeat","arguments":{"project":"alpha","id":ids[0]}
            }}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .find(|line| line.starts_with('{'))
        .unwrap_or(&text)
        .trim();
    let heartbeat: Value = serde_json::from_str(data).unwrap();
    assert!(!refused(&heartbeat["result"]), "{heartbeat}");
    let released = hub
        .call(
            &mut b,
            "board_release",
            json!({"project":"alpha","id":ids[0],"reason":"Other project"}),
        )
        .await;
    assert!(!refused(&released), "{released}");
    let claimed = hub
        .call(
            &mut b,
            "board_claim",
            json!({"project":"shared","id":ids[1]}),
        )
        .await;
    assert!(!refused(&claimed), "{claimed}");
    let mut recipient = edit.clone();
    recipient.id = "recipient".into();
    recipient.enabled = false;
    let target = hub
        .accounts
        .put_agent(&owner.id, &owner.id, &recipient)
        .await
        .unwrap();
    let before = claimed["structuredContent"]["lease"]["ticket"]["version"].clone();
    for to in [
        target.principal(),
        format!("@agent:{}:missing", owner.id),
        "invented-seat".into(),
    ] {
        let denied = hub
            .call(
                &mut b,
                "board_handoff",
                json!({"project":"shared","id":ids[1],"note":"Ready for another worker","to":to}),
            )
            .await;
        assert!(refused(&denied), "{denied}");
        assert_eq!(denied["structuredContent"]["code"], "invalid");
    }
    let unchanged = hub
        .call(&mut b, "board_get", json!({"project":"shared","id":ids[1]}))
        .await;
    assert_eq!(unchanged["structuredContent"]["ticket"]["version"], before);
    recipient.version = target.version;
    recipient.enabled = true;
    hub.accounts
        .put_agent(&owner.id, &owner.id, &recipient)
        .await
        .unwrap();
    let handed = hub.call(&mut b, "board_handoff", json!({"project":"shared","id":ids[1],"note":"Ready for another worker","to":target.principal()})).await;
    assert!(!refused(&handed), "{handed}");
    assert_eq!(handed["structuredContent"]["assignee"], target.principal());
    edit.version = 1;
    edit.enabled = false;
    hub.accounts
        .put_agent(&owner.id, &owner.id, &edit)
        .await
        .unwrap();
    for person in [&a, &b] {
        let response = hub.http.post(format!("{}/mcp", hub.url)).bearer_auth(&person.token)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", person.session.as_deref().unwrap())
            .json(&json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"whoami","arguments":{}}}))
            .send().await.unwrap();
        assert_eq!(
            response.status(),
            401,
            "disabled agent must not retain session authority"
        );
    }
}
