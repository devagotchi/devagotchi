use super::*;
fn authority() -> GrantAuthority {
    GrantAuthority {
        binding: GrantBinding {
            origin: GrantOrigin {
                hub: "a".repeat(64),
                machine: "machine".into(),
                generation: "enrollment".into(),
            },
            account: "owner".into(),
            principal: "@agent:owner:builder".into(),
            admission_id: "admission".into(),
            grant_id: "grant".into(),
            grant_version: 1,
            operation_key: "key".into(),
            input_sha256: "b".repeat(64),
            effective: GrantItem {
                capability: GrantCapability::Exec,
                sandbox_level: GrantLevel::Unsandboxed,
                selector: GrantSelector::default(),
            },
            lease_sha256: None,
        },
        issued_at: 100,
        expires_at: 200,
    }
}
#[test]
fn remote_authority_time_is_bounded_and_never_overflows() {
    let mut auth = authority();
    assert_eq!(auth.remaining_ms(150).unwrap(), 50);
    for now in [-1, 99, 200, i64::MAX] {
        assert!(auth.remaining_ms(now).is_err());
    }
    for (issued, expires) in [
        (0, 30_001),
        (1, 1),
        (-1, 100),
        (i64::MIN, i64::MAX),
        (100, 99),
    ] {
        auth.issued_at = issued;
        auth.expires_at = expires;
        assert!(auth.remaining_ms(100).is_err());
    }
}
#[test]
fn remote_authority_requires_canonical_principal_and_immutable_closed_binding() {
    let auth = authority();
    auth.binding.validate().unwrap();
    for principal in [
        "other",
        "@agent:other:builder",
        "@agent:owner:",
        "@agent:owner:UPPER",
    ] {
        let mut changed = auth.clone();
        changed.binding.principal = principal.into();
        assert!(changed.binding.validate().is_err());
    }
    let mut json = serde_json::to_value(&auth).unwrap();
    json["binding"]["bypass"] = true.into();
    assert!(serde_json::from_value::<GrantAuthority>(json).is_err());
}

#[test]
fn remote_observation_lifetime_and_wire_fields_are_closed() {
    let auth = authority();
    let mut observation = GrantObservation {
        binding: auth.binding,
        issued_at: 100,
        expires_at: 200,
    };
    assert_eq!(observation.remaining_ms(150).unwrap(), 50);
    for now in [-1, 99, 200, i64::MAX] {
        assert!(observation.remaining_ms(now).is_err());
    }
    for (issued, expires) in [(0, 30_001), (1, 1), (-1, 100), (i64::MIN, i64::MAX)] {
        observation.issued_at = issued;
        observation.expires_at = expires;
        assert!(observation.remaining_ms(100).is_err());
    }
    let mut value = serde_json::to_value(observation).unwrap();
    value["renew_execution"] = true.into();
    assert!(serde_json::from_value::<GrantObservation>(value).is_err());
}
