//! The schemas the hub's own tools advertise, held to what they write.

use crate::mcp::{Hub, schema::WhoAmI};

/// What `whoami` writes for an account with nothing optional set must satisfy the output
/// schema it advertises: every `required` property present (tests/clients_oauth.py found
/// one missing through the MCP TypeScript SDK, which checks).
#[test]
fn whoami_output_has_every_required_property() {
    let tool = Hub::router()
        .list_all()
        .into_iter()
        .find(|tool| tool.name == "whoami")
        .expect("whoami is served");
    let schema = tool.output_schema.expect("whoami has an output schema");
    let written = serde_json::to_value(WhoAmI {
        subject: "300000000000000001".into(),
        via: "hub".into(),
        name: None,
        org: None,
        provisional_until: None,
        agent: None,
        claim_code: None,
        provisional_warning: false,
        claim_url: None,
        tenant: None,
        access: None,
        waiting_since: None,
        access_note: None,
        plan: None,
        secure_url: None,
        owner: false,
        human: false,
        brokers: Vec::new(),
        boards: Vec::new(),
        orgs: Vec::new(),
    })
    .unwrap();
    let required = schema["required"].as_array().expect("required properties");
    for property in required {
        let property = property.as_str().unwrap();
        assert!(
            written.get(property).is_some(),
            "whoami's schema requires {property}, which it leaves out"
        );
    }
    assert!(!required.iter().any(|p| p == "provisional_warning"));
    assert!(!required.iter().any(|p| p == "orgs"));
}
