use super::*;
use crate::{ListInput, RequestBody, StartInput};

fn smuggled() -> serde_json::Value {
    serde_json::json!({"account": LOCAL_ACCOUNT, "client": "forged"})
}

#[test]
fn a_caller_in_a_tool_call_is_not_offered_and_a_transport_replaces_it() {
    // An AI is never shown the field...
    for schema in [
        schemars::schema_for!(StartInput),
        schemars::schema_for!(ListInput),
        schemars::schema_for!(SendInput),
        schemars::schema_for!(ResizeInput),
    ] {
        let properties = &schema.as_value()["properties"];
        assert!(properties.get("caller").is_none(), "{properties}");
    }
    // ...but one could still send it, so every transport writes its own over it.
    let hub = Caller::hub("acct-1", "session-9");
    assert_eq!(hub.account, "hub:acct-1");
    let requests = [
        (
            "operation_start",
            serde_json::json!({"idempotency_key": "k", "executable": "x",
            "args": [], "deadline_ms": 1000, "caller": smuggled()}),
        ),
        (
            "operation_list",
            serde_json::json!({"offset": 0, "limit": 1, "caller": smuggled()}),
        ),
        (
            "operation_input",
            serde_json::json!({"operation_id": "o", "caller": smuggled()}),
        ),
        (
            "operation_resize",
            serde_json::json!({"operation_id": "o", "cols": 80, "rows": 24,
            "caller": smuggled()}),
        ),
        // Observing and stopping an operation is its account's alone too.
        (
            "operation_get",
            serde_json::json!({"operation_id": "o", "caller": smuggled()}),
        ),
        (
            "operation_cancel",
            serde_json::json!({"operation_id": "o", "caller": smuggled()}),
        ),
        (
            "operation_read",
            serde_json::json!({"operation_id": "o", "caller": smuggled()}),
        ),
        (
            "operation_wait",
            serde_json::json!({"operation_id": "o", "timeout_ms": 1, "caller": smuggled()}),
        ),
    ];
    for (method, params) in requests {
        let request = serde_json::json!({"method": method, "params": params});
        let mut body: RequestBody = serde_json::from_value(request).unwrap();
        assert_eq!(body.caller().unwrap().client, "forged");
        body.set_caller(&hub);
        assert_eq!(body.caller(), Some(&hub));
    }
    let file = serde_json::json!({"method": "file_info", "params": {"path": "/x"}});
    let mut read: RequestBody = serde_json::from_value(file).unwrap();
    read.set_caller(&hub);
    assert_eq!(
        read.caller(),
        None,
        "a request with no caller field stays as it is"
    );
}
