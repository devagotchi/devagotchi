//! The MCP and HTTP access surfaces share requests, decisions and current credentials.
use super::{
    access_reads::{requested, token},
    access_requests::{answer, connect, input, receive},
    principals::Admin,
    *,
};
use crate::{
    identity::accounts::{
        agents::AgentEdit,
        api_keys::{NewKey, Scope},
    },
    tenancy::tests::tenants::Person,
};

fn person(token: String) -> Person {
    Person {
        token,
        cookie: String::new(),
        session: None,
    }
}
fn success(value: &Value) -> &Value {
    assert_ne!(value["isError"], true, "{value}");
    assert!(value.get("error").is_none(), "{value}");
    &value["structuredContent"]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_mcp_request_http_decision_and_mcp_grants_share_one_durable_result() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut agent = person(bearer);
    let catalog = admin
        .hub
        .call(&mut agent, "capabilities_list", json!({}))
        .await;
    let catalog = success(&catalog);
    assert!(catalog["capabilities"].as_array().unwrap().len() >= 9);
    assert!(!catalog["presets"].as_array().unwrap().is_empty());
    assert!(!catalog["risks"].as_array().unwrap().is_empty());
    assert!(
        catalog["durations"]
            .as_array()
            .unwrap()
            .contains(&json!("task"))
    );
    let mut calls = connect(&admin).await;
    let mut draft = input();
    draft["request_id"] = "cross-surface-request".into();
    let (result, _) = tokio::join!(
        admin.hub.call(&mut agent, "request_access", draft.clone()),
        async {
            let id = receive(&mut calls).await;
            answer(&admin, id, "linux");
        }
    );
    let request = success(&result)["result"].clone();
    let id = request["request_ids"][0].as_str().unwrap();
    let (status, lookup, _) =
        response(admin.request(Method::GET, &format!("/api/v1/access-requests/{id}"))).await;
    assert_eq!(status, StatusCode::OK, "{lookup}");
    assert_eq!(lookup["status"], "pending");
    let (result, _) = tokio::join!(admin.hub.call(&mut agent, "request_access", draft), async {
        let id = receive(&mut calls).await;
        answer(&admin, id, "linux");
    });
    assert_eq!(success(&result)["result"], request);
    assert_eq!(success(&result)["replayed"], true);
    let (status, decision, _) = response(
        admin
            .request(
                Method::POST,
                &format!("/api/v1/access-requests/{id}/approve"),
            )
            .header("idempotency-key", "human-decision")
            .json(&json!({"version":1,"duration":"once"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    let wait = admin
        .hub
        .call(
            &mut agent,
            "wait_for_access",
            json!({"request_id":request["group_id"]}),
        )
        .await;
    assert_eq!(
        success(&wait)["items"][0]["grant_id"],
        decision["result"]["grant_id"]
    );
    let grants = admin
        .hub
        .call(&mut agent, "my_grants", json!({"machine":"lab"}))
        .await;
    assert_eq!(success(&grants)["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        success(&grants)["rows"][0]["id"],
        decision["result"]["grant_id"]
    );
    let machines = admin.hub.call(&mut agent, "machines_list", json!({})).await;
    assert_eq!(success(&machines)["machines"][0]["name"], "lab");
    assert_eq!(success(&machines)["machines"][0]["connected"], true);
    let entries = admin.hub.hub.audit.after(0, 1000).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.tool == "request_access" && entry.outcome == "ok")
            .count(),
        2
    );
    for name in [
        "capabilities_list",
        "wait_for_access",
        "my_grants",
        "machines_list",
    ] {
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.tool == name && entry.outcome == "ok")
                .count(),
            1,
            "{name}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_mcp_wait_rechecks_revoked_key_within_the_existing_session() {
    let admin = or_skip!(Admin::new());
    let request = requested(&admin).await;
    let (key, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut agent = person(bearer);
    success(&admin.hub.call(&mut agent, "my_grants", json!({})).await);
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (result, _) = tokio::join!(
        admin.hub.call(
            &mut agent,
            "wait_for_access",
            json!({"request_id":request.group_id,"timeout_s":3})
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            admin
                .hub
                .accounts
                .revoke_key(&owner.tenant, &owner.id, &key)
                .await
                .unwrap();
        }
    );
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["code"], "unauthenticated");
    let entries = admin.hub.hub.audit.after(0, 1000).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.tool == "wait_for_access" && entry.outcome == "unauthenticated")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_mcp_scopes_closed_arguments_and_other_principal_history_are_refused() {
    let admin = or_skip!(Admin::new());
    let request = requested(&admin).await;
    let mut calls = connect(&admin).await;
    let (_, bearer) = token(&admin, vec![Scope::MachinesRead], Some("lab"), None, true).await;
    let mut agent = person(bearer);
    let denied = admin.hub.call(&mut agent, "my_grants", json!({})).await;
    assert_eq!(denied["isError"], true, "{denied}");
    assert_eq!(denied["structuredContent"]["code"], "key_denied");
    let denied = admin
        .hub
        .call(&mut agent, "capabilities_list", json!({}))
        .await;
    assert_eq!(denied["isError"], true, "{denied}");
    let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, Some("delivery"), true).await;
    let mut agent = person(bearer);
    let mut draft = input();
    draft["request_id"] = "not-project-bound".into();
    let denied = admin.hub.call(&mut agent, "request_access", draft).await;
    assert_eq!(
        denied["structuredContent"]["code"], "key_denied",
        "{denied}"
    );
    let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut agent = person(bearer);
    let mut draft = input();
    draft["request_id"] = "spoof".into();
    draft["principal"] = admin.owner.clone().into();
    let denied = admin.hub.call(&mut agent, "request_access", draft).await;
    assert_eq!(denied["isError"], true, "{denied}");
    assert!(calls.try_recv().is_err());
    admin
        .hub
        .accounts
        .put_agent(
            &admin.owner,
            &admin.owner,
            &AgentEdit {
                id: "other".into(),
                name: "Other".into(),
                kind: "codex".into(),
                roles: vec!["worker".into()],
                enabled: true,
                wip_limit: None,
                version: 0,
            },
        )
        .await
        .unwrap();
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (_, bearer) = admin
        .hub
        .accounts
        .create_key(
            &owner.tenant,
            &owner.id,
            &NewKey {
                name: "Other agent".into(),
                scopes: Scope::ALL.to_vec(),
                machines: vec![],
                projects: vec![],
                agent_id: Some("other".into()),
                hours: 24,
            },
        )
        .await
        .unwrap();
    let mut other = person(bearer);
    let denied = admin
        .hub
        .call(
            &mut other,
            "wait_for_access",
            json!({"request_id":request.group_id}),
        )
        .await;
    assert_eq!(denied["isError"], true, "{denied}");
    assert_eq!(denied["structuredContent"]["code"], "not_found");
    let grants = admin.hub.call(&mut other, "my_grants", json!({})).await;
    assert_eq!(success(&grants)["rows"], json!([]));
}

#[test]
fn access_mcp_schema_matches_nested_typed_requests_and_refuses_null_selectors() {
    let tools = crate::mcp::tools();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "request_access")
        .unwrap();
    let schema = Value::Object(tool.input_schema.as_ref().clone());
    let mut value = input();
    value["request_id"] = "schema".into();
    assert!(
        crate::mcp::output_schema::errors(&schema, &value).is_empty(),
        "{schema}"
    );
    value["selection"]["items"][0]["selector"]["paths"] = Value::Null;
    assert!(!crate::mcp::output_schema::errors(&schema, &value).is_empty());
    let mut value = input();
    value["request_id"] = "schema".into();
    value["principal"] = "spoofed".into();
    assert!(!crate::mcp::output_schema::errors(&schema, &value).is_empty());
}
