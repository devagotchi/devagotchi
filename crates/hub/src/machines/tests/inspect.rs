//! `machine_inspect` over HTTP on a hosted hub, on the test Postgres: a machine `online` in
//! `machines` has its link up; whether its broker answers is the broker's to say, within 3
//! seconds, and every way it does not is named and audited as it happened.

use std::{sync::Arc, time::Duration};

use devagotchi_contracts::{
    API_VERSION, BrokerCapabilities, RequestBody, ResponseBody, WireRequest, WireResponse,
};
use serde_json::{Value, json};
use tokio::{sync::mpsc::Receiver, task::JoinHandle};

use crate::{
    machines::link::Links,
    tenancy::tests::tenants::{Hosted, Person, hosted_with, refused},
};

type Calls = Receiver<(u64, WireRequest)>;

fn capabilities() -> BrokerCapabilities {
    BrokerCapabilities {
        api_version: API_VERSION,
        version: "test-1".into(),
        os: "linux".into(),
        arch: "aarch64".into(),
        exec_policy: "granted".into(),
        shell_commands: false,
        workspace_scope: "workspace".into(),
        supported_requests: vec!["broker_capabilities".into(), "operation_start".into()],
    }
}

async fn inspect(hub: &Hosted, person: &mut Person, machine: &str) -> Value {
    hub.call(person, "machine_inspect", json!({ "machine": machine }))
        .await
}

fn code(answer: &Value) -> &str {
    assert!(refused(answer), "{answer}");
    answer["structuredContent"]["code"]
        .as_str()
        .unwrap_or_default()
}

/// The machine's broker answering the next call it is sent with `answer`; the calls back after.
fn broker_answers(
    links: Arc<Links>,
    device: String,
    mut calls: Calls,
    answer: BrokerCapabilities,
) -> JoinHandle<Calls> {
    tokio::spawn(async move {
        let (id, asked) = calls.recv().await.unwrap();
        assert!(matches!(asked.request, RequestBody::BrokerCapabilities(_)));
        let response = WireResponse::Ok(ResponseBody::Capabilities(answer));
        links.answer_for_test(&device, id, response);
        calls
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inspection_asks_the_broker_and_says_why_not_without_guessing() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut person = hub.first_connect("Claude").await;

    // A name the caller has no machine by.
    let answer = inspect(&hub, &mut person, "nowhere").await;
    assert_eq!(code(&answer), "unknown_broker");

    // Its own machine, joined, with no link running.
    let added = hub
        .call(&mut person, "machine_add", json!({ "name": "desk" }))
        .await;
    let added = &added["structuredContent"];
    let stored = added["manual"].as_str().unwrap();
    let stored = stored.split("--device ").nth(1).unwrap();
    let stored = stored.split(' ').next().unwrap().to_string();
    let enrolled = hub
        .http
        .post(format!("{}/enroll", hub.url))
        .json(&json!({ "name": stored, "code": added["code"], "public_key": "desk-key" }))
        .send()
        .await
        .unwrap();
    assert_eq!(enrolled.status(), 201);
    let answer = inspect(&hub, &mut person, "desk").await;
    assert_eq!(code(&answer), "broker_offline");

    // Its link up and its broker silent: `machines` says online, the inspection says unknown
    // after its 3 seconds, and the link stays.
    let mut calls = hub.hub.links.connect_for_test(&stored);
    let listed = hub.call(&mut person, "machines", json!({})).await;
    assert_eq!(
        listed["structuredContent"]["machines"][0]["status"], "online",
        "{listed}"
    );
    let began = std::time::Instant::now();
    let answer = inspect(&hub, &mut person, "desk").await;
    assert_eq!(code(&answer), "inspection_timeout");
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_secs(3) && waited < Duration::from_secs(10),
        "{waited:?}"
    );
    let (_, asked) = calls.recv().await.unwrap();
    assert!(matches!(asked.request, RequestBody::BrokerCapabilities(_)));
    assert!(hub.hub.links.connected(&stored));

    // Its broker answers: that answer, as it came, held to the tool's output schema.
    let links = hub.hub.links.clone();
    let broker = broker_answers(links.clone(), stored.clone(), calls, capabilities());
    let answer = inspect(&hub, &mut person, "desk").await;
    assert!(!refused(&answer), "{answer}");
    assert_eq!(
        answer["structuredContent"],
        serde_json::to_value(capabilities()).unwrap()
    );
    let calls = broker.await.unwrap();

    // A broker on another API version is not a ready one.
    let mut other = capabilities();
    other.api_version += 1;
    let broker = broker_answers(links, stored.clone(), calls, other);
    let answer = inspect(&hub, &mut person, "desk").await;
    assert_eq!(code(&answer), "api_version");
    broker.await.unwrap();

    // Every one is in the audit, as it happened, newest first for the caller; the chain holds.
    let recent = hub
        .call(
            &mut person,
            "get_recent_tool_calls",
            json!({ "tool": "machine_inspect", "max_results": 10 }),
        )
        .await;
    let outcomes: Vec<&str> = recent["structuredContent"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(
        outcomes,
        [
            "api_version",
            "ok",
            "inspection_timeout",
            "broker_offline",
            "unknown_broker"
        ],
        "{recent}"
    );
    let recorded = hub.hub.audit.after(0, 1000).await.unwrap();
    let inspections: Vec<(&str, &str)> = recorded
        .iter()
        .filter(|entry| entry.tool == "machine_inspect")
        .map(|entry| (entry.broker.as_str(), entry.outcome.as_str()))
        .collect();
    assert_eq!(
        inspections,
        [
            ("nowhere", "unknown_broker"),
            ("desk", "broker_offline"),
            ("desk", "inspection_timeout"),
            ("desk", "ok"),
            ("desk", "api_version"),
        ]
    );
    let chain = hub.hub.audit.verify().await.unwrap();
    assert!(chain.ok && chain.entries >= 5, "{chain:?}");
}
