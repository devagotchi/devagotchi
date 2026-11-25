use super::*;
use crate::RequestBody;
use serde_json::json;

#[test]
fn capabilities_request_is_read_only_and_takes_nothing() {
    let request: RequestBody =
        serde_json::from_value(json!({"method": "broker_capabilities", "params": {}})).unwrap();
    assert!(matches!(request, RequestBody::BrokerCapabilities(_)));
    assert_eq!(request.held_for(), std::time::Duration::ZERO);
    assert_eq!(request.target(), "");
    assert!(request.caller().is_none());
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({"method": "broker_capabilities", "params": {}})
    );
    assert!(
        serde_json::from_value::<RequestBody>(
            json!({"method": "broker_capabilities", "params": {"command": "ignored"}})
        )
        .is_err()
    );
}

/// `REQUEST_METHODS` is every method `RequestBody` reads, in its order: serde names them all
/// when it refuses one it does not know, so a new kind of request fails here until listed.
#[test]
fn request_methods_are_every_request_the_wire_reads() {
    let refused =
        serde_json::from_value::<RequestBody>(json!({"method": "no_such_request", "params": {}}))
            .unwrap_err()
            .to_string();
    let expected = refused
        .split("expected one of ")
        .nth(1)
        .unwrap_or_else(|| panic!("{refused}"));
    let methods: Vec<&str> = expected
        .split(", ")
        .map(|name| name.trim_matches('`'))
        .collect();
    assert_eq!(methods, REQUEST_METHODS, "{refused}");
}

#[test]
fn capabilities_carry_no_path_grant_or_account() {
    let schema = schemars::schema_for!(BrokerCapabilities);
    let mut fields: Vec<&str> = schema.as_value()["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "api_version",
            "arch",
            "exec_policy",
            "os",
            "shell_commands",
            "supported_requests",
            "version",
            "workspace_scope",
        ]
    );
}

#[test]
fn every_broker_method_but_the_desktops_own_is_a_tool() {
    let mut methods: Vec<_> = crate::describe::BROKER_TOOLS
        .iter()
        .map(|(_, m)| *m)
        .collect();
    methods.push("operations_changed");
    methods.sort();
    let mut all = super::REQUEST_METHODS.to_vec();
    all.sort();
    assert_eq!(methods, all);
    let names: std::collections::BTreeSet<_> = crate::describe::BROKER_TOOLS
        .iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names.len(), crate::describe::BROKER_TOOLS.len());
}
