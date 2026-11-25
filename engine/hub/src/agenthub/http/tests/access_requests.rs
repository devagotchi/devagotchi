//! Real HTTP and PostgreSQL, with controlled authenticated broker responses/races.
use super::{
    access_reads::{requested, token},
    principals::Admin,
    *,
};
use crate::{
    identity::accounts::api_keys::Scope,
    machines::{access::Access, devices::Devices},
};
use devagotchi_contracts::{
    GrantLevel, REMOTE_ACCESS_VERSION, RemoteReply, RemoteRequest, RemoteWireRequest,
    RemoteWireResponse,
};
use tokio::sync::mpsc;
type Calls = mpsc::Receiver<(u64, RemoteWireRequest)>;

pub(super) async fn connect(admin: &Admin) -> Calls {
    let binding = Access::new(admin.hub.accounts.clone())
        .machine_administration(&admin.owner, "lab", None)
        .await
        .unwrap()
        .binding;
    admin
        .hub
        .hub
        .links
        .connect_remote_for_test(binding, "machine-key")
}
pub(super) async fn receive(calls: &mut Calls) -> u64 {
    let (id, wire) = tokio::time::timeout(std::time::Duration::from_secs(5), calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(wire.remote, RemoteRequest::Capabilities {}));
    id
}
pub(super) fn answer(admin: &Admin, id: u64, os: &str) {
    admin.hub.hub.links.answer_remote_for_test(
        "lab",
        id,
        RemoteWireResponse::Ok(RemoteReply::Capabilities {
            version: REMOTE_ACCESS_VERSION,
            os: os.into(),
            sandbox_levels: vec![GrantLevel::Unsandboxed],
        }),
    );
}
pub(super) fn input() -> Value {
    json!({"machine":"lab", "selection":{"kind":"items","items":[{
        "capability":"exec", "sandbox_level":"read-only", "selector":{"paths":["/work"]}
    }]}, "duration":"1h", "reason":"Inspect this repository for the requested change."})
}
fn post(admin: &Admin, token: &str, key: &str, body: &Value) -> reqwest::RequestBuilder {
    admin
        .hub
        .http
        .post(format!("{}/api/v1/access-requests", admin.hub.url))
        .bearer_auth(token)
        .header("idempotency-key", key)
        .json(body)
}
async fn submit(
    admin: &Admin,
    calls: &mut Calls,
    token: &str,
    key: &str,
    body: &Value,
) -> (StatusCode, Value, bool) {
    let response = response(post(admin, token, key, body));
    tokio::pin!(response);
    tokio::select! {
        result = &mut response => panic!("request completed before asking the broker: {result:?}"),
        id = receive(calls) => answer(admin, id, "linux"),
    }
    response.await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_http_replays_pending_request_and_reports_actual_available_levels() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let (_, token) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let mut calls = connect(&admin).await;
    let (status, first, replayed) =
        submit(&admin, &mut calls, &token, "request-http", &input()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(!replayed);
    assert_eq!(first["result"]["status"], "pending");
    assert_eq!(
        first["machine_capabilities"]["sandbox_levels"],
        json!(["unsandboxed"])
    );
    assert!(first["machine_capabilities"]["display_server"].is_null());
    // Requesting a future confinement level does not authorize it or claim the broker supports it.
    let (status, again, replayed) =
        submit(&admin, &mut calls, &token, "request-http", &input()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(replayed);
    assert_eq!(first["result"], again["result"]);
    let mut changed = input();
    changed["reason"] = "A different operation needs a different original request key.".into();
    assert_eq!(
        submit(&admin, &mut calls, &token, "request-http", &changed)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let discovery = admin
        .hub
        .http
        .get(format!(
            "{}/api/v1/machines/lab/access-capabilities",
            admin.hub.url
        ))
        .bearer_auth(&token);
    let (result, _) = tokio::join!(response(discovery), async {
        let id = receive(&mut calls).await;
        answer(&admin, id, "windows");
    });
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["capabilities"]["os"], "windows");
    let (_, grants, _) = response(admin.request(Method::GET, "/api/v1/grants")).await;
    assert_eq!(grants["rows"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_http_checks_scopes_identity_and_closed_fields_before_contacting_broker() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let mut calls = connect(&admin).await;
    for (scopes, machine, project, bound) in [
        (vec![Scope::MachinesRead], None, None, true),
        (Scope::ALL.to_vec(), Some("other"), None, true),
        (Scope::ALL.to_vec(), None, Some("delivery"), true),
        (Scope::ALL.to_vec(), None, None, false),
    ] {
        let (_, key) = token(&admin, scopes, machine, project, bound).await;
        let result = response(post(&admin, &key, "refuse", &input())).await;
        assert_eq!(result.0, StatusCode::FORBIDDEN, "{}", result.1);
    }
    let (_, token) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    for field in ["os", "display_server", "owner", "generation", "principal"] {
        let mut body = input();
        body[field] = "spoofed".into();
        assert_eq!(
            response(post(&admin, &token, "spoof", &body)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        response(
            post(&admin, &token, "seat", &input())
                .header(crate::agenthub::mcp::SEAT_HEADER, "worker")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut foreign = input();
    foreign["machine"] = "another-account".into();
    assert_eq!(
        response(post(&admin, &token, "foreign", &foreign)).await.0,
        StatusCode::NOT_FOUND
    );
    assert!(calls.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_http_rechecks_credential_and_enrollment_after_broker_wait() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let mut calls = connect(&admin).await;
    let (key, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let owner = admin.hub.accounts.get(&admin.owner).await.unwrap().unwrap();
    let (result, _) = tokio::join!(
        response(post(&admin, &bearer, "revoked-while-waiting", &input())),
        async {
            let id = receive(&mut calls).await;
            admin
                .hub
                .accounts
                .revoke_key(&owner.tenant, &owner.id, &key)
                .await
                .unwrap();
            answer(&admin, id, "linux");
        }
    );
    assert_eq!(result.0, StatusCode::UNAUTHORIZED, "{}", result.1);
    let (_, bearer) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let (result, _) = tokio::join!(
        response(post(&admin, &bearer, "reenrolled-while-waiting", &input())),
        async {
            let id = receive(&mut calls).await;
            let devices = Devices::new(admin.hub.accounts.store().clone());
            let code = devices
                .issue_owned_code(&admin.owner, "lab", [133; 16], 16)
                .await
                .unwrap();
            devices
                .enroll("lab", &code, "machine-key", false)
                .await
                .unwrap();
            answer(&admin, id, "linux");
        }
    );
    assert_ne!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["error"]["code"], "machine_reenrolled");
    let (_, page, _) = response(admin.request(Method::GET, "/api/v1/access-requests")).await;
    assert_eq!(page["rows"].as_array().unwrap().len(), 1, "{page}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_http_refuses_offline_and_invalid_broker_facts_without_creating_a_request() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    let (_, token) = token(&admin, Scope::ALL.to_vec(), None, None, true).await;
    let result = response(post(&admin, &token, "offline", &input())).await;
    assert_eq!(result.0, StatusCode::SERVICE_UNAVAILABLE, "{}", result.1);
    let mut calls = connect(&admin).await;
    let (result, _) = tokio::join!(
        response(post(&admin, &token, "invalid-facts", &input())),
        async {
            let id = receive(&mut calls).await;
            answer(&admin, id, "made-up-os");
        }
    );
    assert_eq!(result.0, StatusCode::SERVICE_UNAVAILABLE, "{}", result.1);
    assert_eq!(result.1["error"]["code"], "invalid_response");
    let mut short = input();
    short["reason"] = "short".into();
    assert_eq!(
        submit(&admin, &mut calls, &token, "short", &short).await.0,
        StatusCode::BAD_REQUEST
    );
    let (_, page, _) = response(admin.request(Method::GET, "/api/v1/access-requests")).await;
    assert_eq!(page["rows"].as_array().unwrap().len(), 1, "{page}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_request_http_binds_the_allowed_project_and_rechecks_task_fence() {
    let admin = or_skip!(Admin::new());
    requested(&admin).await;
    super::projects::create(&admin, "delivery").await;
    let (status, task, _) = response(
        admin
            .request(Method::POST, "/api/v1/tasks")
            .json(&json!({"project":"delivery","title":"Inspect the source"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{task}");
    let (_, token) = token(
        &admin,
        Scope::ALL.to_vec(),
        Some("lab"),
        Some("delivery"),
        true,
    )
    .await;
    let (status, claim, _) = response(
        admin
            .hub
            .http
            .post(format!("{}/api/v1/claim-next", admin.hub.url))
            .bearer_auth(&token)
            .json(&json!({"project":"delivery","request_id":"claim"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claim}");
    let mut body = input();
    body["duration"] = "task".into();
    body["task"] = json!({"project":"delivery","ticket":claim["lease"]["ticket"]["id"],"fence":claim["lease"]["fence"]});
    let mut calls = connect(&admin).await;
    let (status, first, _) = submit(&admin, &mut calls, &token, "task-request", &body).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let mut foreign = body.clone();
    foreign["task"]["project"] = "other".into();
    assert_eq!(
        response(post(&admin, &token, "wrong-project", &foreign))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(calls.try_recv().is_err());
    let mut stale = body.clone();
    stale["task"]["fence"] = (body["task"]["fence"].as_i64().unwrap() + 1).into();
    let result = submit(&admin, &mut calls, &token, "wrong-fence", &stale).await;
    assert_eq!(result.0, StatusCode::CONFLICT, "{}", result.1);
    assert_eq!(result.1["error"]["code"], "stale_fence");
    let (_, page, _) = response(admin.request(Method::GET, "/api/v1/access-requests")).await;
    assert_eq!(page["rows"].as_array().unwrap().len(), 2, "{page}");
}
