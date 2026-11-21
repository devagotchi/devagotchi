use super::*;

#[test]
fn risk_paths_include_parent_scopes_and_items_only_narrow() {
    let condition: GrantRiskCondition =
        serde_json::from_value(serde_json::json!({"path_prefix":["/mnt/"]})).unwrap();
    for (paths, expected) in [
        (serde_json::json!({"paths":["/"]}), true),
        (serde_json::json!({"paths":["/mnt/c"]}), true),
        (serde_json::json!({"paths":["/mnt-other"]}), false),
        (serde_json::json!({"paths":[]}), false),
        (serde_json::json!({}), true),
    ] {
        let selector: GrantSelector = serde_json::from_value(paths).unwrap();
        assert_eq!(
            condition
                .matches(&GrantRiskContext {
                    selector: &selector,
                    level: GrantLevel::ReadOnly,
                    duration: GrantDuration::Task,
                    display_server: "",
                    machine_kind: "wsl"
                })
                .unwrap(),
            expected
        );
    }
    let request: GrantItem = serde_json::from_value(serde_json::json!({"capability":"exec",
        "sandbox_level":"workspace-write","selector":{"argv_prefix":[["echo"]]}}))
    .unwrap();
    let mut decision = request.clone();
    decision.sandbox_level = GrantLevel::ReadOnly;
    decision.selector.argv_prefix = Some(vec![vec!["echo".into(), "hello".into()]]);
    decision.within(&request).unwrap();
    decision.sandbox_level = GrantLevel::Unsandboxed;
    assert!(decision.within(&request).is_err());
    decision = request.clone();
    decision.capability = GrantCapability::Process;
    assert!(decision.within(&request).is_err());
    let mut input: GrantItem = serde_json::from_value(serde_json::json!({"capability":"gui.input",
        "sandbox_level":"desktop","selector":{"apps":"any"}}))
    .unwrap();
    assert!(input.needs_unsandboxed_policy());
    input.selector.apps = Some(AllowedNames::Names(vec![]));
    assert!(!input.needs_unsandboxed_policy());
}
use serde_json::{Value, json};
fn selector(value: Value) -> GrantSelector {
    let s: GrantSelector = serde_json::from_value(value).unwrap();
    s.validate().unwrap();
    s
}
fn command() -> GrantCall {
    let mut call = GrantCall::new(GrantCapability::Exec, GrantLevel::WorkspaceWrite);
    call.argv = vec!["podman".into(), "run".into()];
    call.paths = vec!["/w/x".into()];
    call.hosts = vec!["cdn01.quay.io".into()];
    call
}

#[test]
fn source_narrowing_fixtures_preserve_paths_arguments_hosts_and_limits() {
    let requested = selector(
        json!({"paths":["/home/a/code"],"argv_prefix":[["npm"],["git"]],"hosts":["*.npmjs.org"],"max_runtime_s":600}),
    );
    assert!(requested.within(&requested).is_ok());
    for value in [
        json!({"paths":["/home/a/code/shop"],"argv_prefix":[["npm","test"]],"hosts":["registry.npmjs.org"],"max_runtime_s":60}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["git"]],"hosts":[],"max_runtime_s":600}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["git"]],"max_runtime_s":600}),
    ] {
        assert!(selector(value).within(&requested).is_ok());
    }
    for value in [
        json!({"paths":["/home/a"],"argv_prefix":[["git"]],"max_runtime_s":600}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["bash"]],"max_runtime_s":600}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["git"]],"hosts":["*"],"max_runtime_s":600}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["git"]],"max_runtime_s":601}),
        json!({"argv_prefix":[["git"]],"max_runtime_s":600}),
        json!({"paths":["/home/a/code"],"argv_prefix":[["git"]]}),
        json!({"paths":["/home/a/codex"],"argv_prefix":[["git"]],"max_runtime_s":600}),
    ] {
        let s = selector(value);
        assert!(s.within(&requested).is_err(), "{s:?}");
    }
    assert!(
        selector(json!({"env_allow":["HOME"]}))
            .within(&GrantSelector::default())
            .is_err()
    );
    assert!(
        selector(json!({"apps":"any"}))
            .within(&selector(json!({"apps":["gedit"]})))
            .is_err()
    );
    assert!(
        selector(json!({"deny_keys":[]}))
            .within(&selector(json!({"deny_keys":["super"]})))
            .is_err()
    );
}

#[test]
fn explicit_empty_allowlists_never_turn_a_narrowed_grant_into_any() {
    let requested = selector(
        json!({"paths":["/w"],"argv_prefix":[["podman"]],"hosts":["quay.io","*.quay.io"]}),
    );
    let call = command();
    assert!(
        requested
            .permits(GrantCapability::Exec, GrantLevel::WorkspaceWrite, &call)
            .is_ok()
    );
    for field in ["paths", "argv_prefix", "hosts"] {
        let mut value = serde_json::to_value(&requested).unwrap();
        value[field] = json!([]);
        let narrowed = selector(value);
        assert!(narrowed.within(&requested).is_ok());
        assert!(
            narrowed
                .permits(GrantCapability::Exec, GrantLevel::WorkspaceWrite, &call)
                .is_err(),
            "{field}"
        );
    }
    let mut gui = GrantCall::new(GrantCapability::Input, GrantLevel::Desktop);
    gui.apps = vec!["gedit".into()];
    gui.actions = vec!["click".into()];
    for value in [json!({"apps":[]}), json!({"actions":[]})] {
        let s = selector(value);
        assert!(s.within(&GrantSelector::default()).is_ok());
        assert!(
            s.permits(GrantCapability::Input, GrantLevel::Desktop, &gui)
                .is_err()
        );
    }
}

#[test]
fn source_call_fixtures_refuse_wider_operations_and_keep_argv_elements_literal() {
    let scope = selector(
        json!({"paths":["/w"],"argv_prefix":[["podman"]],"hosts":["quay.io","*.quay.io"]}),
    );
    let original = command();
    for n in 0..7 {
        let mut c = original.clone();
        match n {
            0 => c.argv = vec!["podmanx".into()],
            1 => c.argv = vec!["sh".into(), "-c".into(), "podman run".into()],
            2 => c.paths = vec!["/etc".into()],
            3 => c.hosts = vec!["evil.io".into()],
            4 => c.level = GrantLevel::Unsandboxed,
            5 => c.capability = GrantCapability::FileWrite,
            _ => c.env_names = vec!["SECRET".into()],
        };
        assert!(
            scope
                .permits(GrantCapability::Exec, GrantLevel::WorkspaceWrite, &c)
                .is_err(),
            "{n}"
        );
    }
    let mut c = GrantCall::new(GrantCapability::FileRead, GrantLevel::ReadOnly);
    c.paths = vec!["/x".into()];
    assert!(
        GrantSelector::default()
            .permits(GrantCapability::FileWrite, GrantLevel::WorkspaceWrite, &c)
            .is_ok()
    );
    let mut c = command();
    c.hosts.clear();
    c.argv = vec![
        "python".into(),
        "-c".into(),
        "print('one')\nprint('two')".into(),
        String::new(),
    ];
    assert!(
        selector(json!({"argv_prefix":[["python","-c"]]}))
            .permits(GrantCapability::Exec, GrantLevel::WorkspaceWrite, &c)
            .is_ok()
    );
}

#[test]
fn malformed_types_unknown_keys_and_nulls_cannot_remove_scope_constraints() {
    for value in [
        json!({"paths":"/w"}),
        json!({"hosts":[1]}),
        json!({"apps":"all"}),
        json!({"argv_prefix":["sh"]}),
        json!({"max_runtime_s":"60"}),
        json!({"max_edge":-1}),
        json!({"extra":[]}),
        json!({"paths":null}),
        json!({"max_runtime_s":null}),
    ] {
        assert!(
            serde_json::from_value::<GrantSelector>(value.clone()).is_err(),
            "{value}"
        );
    }
    for value in [
        json!({"paths":["../private"]}),
        json!({"argv_prefix":[[]]}),
        json!({"max_edge":0}),
        json!({"hosts":["https://example.com"]}),
        json!({"hosts":["*.com:443"]}),
    ] {
        let scope: GrantSelector = serde_json::from_value(value).unwrap();
        assert!(scope.validate().is_err());
    }
}

#[test]
fn lexical_scope_refuses_parent_sibling_and_root_confusion() {
    for (path, root, expected) in [
        ("/w/a", "/w", true),
        ("/w/../etc", "/w", false),
        ("/w2", "/w", false),
        ("C:\\work\\a", "C:/work", true),
        ("D:/work/a", "C:/work", false),
        ("C:work", "C:/", false),
        ("~/work/a", "~/work", true),
        ("/home/a", "~", false),
        ("/../../private", "/", false),
        ("//server/share", "/", false),
    ] {
        assert_eq!(selector::path_within(path, root), expected, "{path} {root}");
    }
}

#[test]
fn host_pattern_narrowing_cannot_broaden_an_exact_or_wildcard_domain() {
    for (host, pattern, expected) in [
        ("a.example.com", "*.example.com", true),
        ("example.com", "*.example.com", false),
        ("badexample.com", "*.example.com", false),
        ("*.child.example.com", "*.example.com", true),
        ("*.example.com", "a.example.com", false),
        ("*", "*.example.com", false),
        ("A.EXAMPLE.COM", "a.example.com", true),
        ("*.example.com", "*", true),
        ("::1", "0:0:0:0:0:0:0:1", true),
        ("2001:db8::1", "::1", false),
        ("EXAMPLE.com.", "example.com", true),
        ("127.0.0.1", "*.0.0.1", false),
    ] {
        assert_eq!(
            selector::host_within(host, pattern),
            expected,
            "{host} {pattern}"
        );
    }
}

#[test]
fn levels_and_durations_preserve_task_and_wall_clock_boundaries() {
    for (narrow, wide) in [
        (GrantLevel::ReadOnly, GrantLevel::WorkspaceWrite),
        (GrantLevel::WorkspaceWrite, GrantLevel::Unsandboxed),
        (GrantLevel::Desktop, GrantLevel::Unsandboxed),
    ] {
        assert!(narrow.within(wide));
        assert!(!wide.within(narrow));
    }
    assert!(!GrantLevel::Desktop.within(GrantLevel::WorkspaceWrite));
    assert!(!GrantLevel::ReadOnly.within(GrantLevel::Desktop));
    assert!(serde_json::from_value::<GrantLevel>(json!("unknown")).is_err());
    for (narrow, wide, expected) in [
        ("1h", "8h", true),
        ("8h", "1h", false),
        ("8h", "task", false),
        ("once", "task", true),
        ("task", "until_revoked", true),
    ] {
        let a = GrantDuration::try_from(narrow.to_owned()).unwrap();
        let b = GrantDuration::try_from(wide.to_owned()).unwrap();
        assert_eq!(a.within(b), expected);
        assert_eq!(serde_json::to_value(a).unwrap(), json!(narrow));
    }
    for invalid in ["0h", "-1h", "+1h", " 1h", "unknown", "4294967296h"] {
        assert!(serde_json::from_value::<GrantDuration>(json!(invalid)).is_err());
    }
    assert!(!GrantDuration::Hours(0).within(GrantDuration::UntilRevoked));
}

#[test]
fn gui_bounds_and_deny_keys_cannot_be_dropped_or_bypassed() {
    let scope = selector(
        json!({"apps":["editor"],"actions":["click","key"],"deny_keys":["super"],"max_actions_per_minute":20}),
    );
    let mut call = GrantCall::new(GrantCapability::Input, GrantLevel::Desktop);
    call.apps = vec!["editor".into()];
    call.actions = vec!["click".into()];
    assert!(
        scope
            .permits(GrantCapability::Input, GrantLevel::Desktop, &call)
            .is_err()
    );
    call.actions_per_minute = Some(10);
    assert!(
        scope
            .permits(GrantCapability::Input, GrantLevel::Desktop, &call)
            .is_ok()
    );
    call.keys = vec!["super".into()];
    assert!(
        scope
            .permits(GrantCapability::Input, GrantLevel::Desktop, &call)
            .is_err()
    );
    let mut narrowed = scope.clone();
    narrowed.deny_keys = None;
    assert!(narrowed.within(&scope).is_err());
    narrowed = scope.clone();
    narrowed.max_actions_per_minute = None;
    assert!(narrowed.within(&scope).is_err());
}

#[test]
fn risk_snapshots_use_the_same_scope_meanings_as_admission() {
    let scope =
        selector(json!({"argv_prefix":[["docker"]],"hosts":["*.docker.io"],"paths":["/mnt/c/x"]}));
    let mut context = GrantRiskContext {
        selector: &scope,
        level: GrantLevel::WorkspaceWrite,
        duration: GrantDuration::Task,
        display_server: "x11",
        machine_kind: "wsl",
    };
    for value in [
        json!({}),
        json!({"argv0":["docker","podman"]}),
        json!({"hosts_wildcard":true}),
        json!({"path_prefix":["/mnt/"]}),
        json!({"display_server":"x11","sandbox_level":"workspace-write"}),
        json!({"apps_any":true}),
    ] {
        let condition: GrantRiskCondition = serde_json::from_value(value).unwrap();
        assert!(condition.matches(&context).unwrap());
    }
    for value in [
        json!({"argv0":["sh"]}),
        json!({"hosts_wildcard":false}),
        json!({"duration":"until_revoked"}),
        json!({"path_prefix":["/private"]}),
    ] {
        let condition: GrantRiskCondition = serde_json::from_value(value).unwrap();
        assert!(!condition.matches(&context).unwrap());
    }
    for value in [
        json!({"unknown":true}),
        json!({"apps_any":"true"}),
        json!({"duration":null}),
        json!({"argv0":"docker"}),
    ] {
        assert!(serde_json::from_value::<GrantRiskCondition>(value).is_err());
    }
    let empty = selector(json!({"argv_prefix":[],"paths":[],"apps":[]}));
    context.selector = &empty;
    for value in [
        json!({"argv0":["docker"]}),
        json!({"path_prefix":["/mnt/"]}),
        json!({"apps_any":true}),
    ] {
        let condition: GrantRiskCondition = serde_json::from_value(value).unwrap();
        assert!(!condition.matches(&context).unwrap());
    }
}

#[test]
fn observation_and_network_calls_must_identify_their_actual_targets() {
    for cap in [GrantCapability::Observe, GrantCapability::Network] {
        let scope = GrantSelector::default();
        let call = GrantCall::new(cap, GrantLevel::Unsandboxed);
        assert!(scope.permits(cap, GrantLevel::Unsandboxed, &call).is_err());
    }
}

#[test]
fn read_only_authority_never_admits_file_writes_or_gui_input() {
    let mut file = GrantCall::new(GrantCapability::FileWrite, GrantLevel::ReadOnly);
    file.paths = vec!["/w/data".into()];
    assert!(
        GrantSelector::default()
            .permits(GrantCapability::FileWrite, GrantLevel::ReadOnly, &file)
            .is_err()
    );
    assert!(
        GrantSelector::default()
            .permits(GrantCapability::FileWrite, GrantLevel::Unsandboxed, &file)
            .is_err()
    );
    let mut gui = GrantCall::new(GrantCapability::Input, GrantLevel::ReadOnly);
    gui.apps = vec!["editor".into()];
    gui.actions = vec!["click".into()];
    assert!(
        GrantSelector::default()
            .permits(GrantCapability::Input, GrantLevel::Unsandboxed, &gui)
            .is_err()
    );
}
