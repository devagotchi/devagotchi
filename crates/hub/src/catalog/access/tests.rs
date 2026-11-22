use super::*;
use crate::store;

fn facts() -> MachineFacts<'static> {
    MachineFacts {
        os: "linux",
        display_server: "x11",
        kind: "wsl",
    }
}
fn selection(value: Value) -> Selection {
    serde_json::from_value(value).unwrap()
}
async fn prepare(
    catalog: &Catalog,
    account: &str,
    selection: &Selection,
    duration: Option<GrantDuration>,
) -> BridgeResult<Prepared> {
    transaction!(&catalog.accounts.store, |c| {
        catalog
            .prepare_access_in(c, account, selection, duration, &facts())
            .await
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_access_snapshots_all_seed_presets_and_refuses_stale_versions() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store.clone()).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [81; 16])
        .await
        .unwrap();
    let catalog = Catalog::new(accounts);
    let presets = catalog
        .list(&owner.id, Kind::GrantPresets, None)
        .await
        .unwrap();
    assert!(!presets.is_empty());
    for preset in presets {
        let selection = Selection::Preset {
            id: preset.id.clone(),
            version: preset.version,
        };
        let prepared = prepare(&catalog, &owner.id, &selection, None)
            .await
            .unwrap();
        assert_eq!(prepared.preset.as_ref().unwrap(), &preset);
        assert!(!prepared.items.is_empty());
        for item in &prepared.items {
            item.item.validate().unwrap();
            assert!(!item.risks.is_empty(), "{}", item.capability.id);
        }
        if preset.id == "experimental-full-access" {
            assert!(prepared.needs_unsandboxed_policy);
            assert_eq!(prepared.duration, GrantDuration::UntilRevoked);
        } else if prepared.duration != GrantDuration::UntilRevoked {
            assert_eq!(
                prepare(
                    &catalog,
                    &owner.id,
                    &selection,
                    Some(GrantDuration::UntilRevoked)
                )
                .await
                .unwrap_err()
                .code,
                "scope_wider_than_preset"
            );
            assert_eq!(
                prepare(&catalog, &owner.id, &selection, Some(GrantDuration::Once))
                    .await
                    .unwrap()
                    .duration,
                GrantDuration::Once
            );
        }
        let stale = Selection::Preset {
            id: preset.id,
            version: preset.version + 1,
        };
        assert_eq!(
            prepare(&catalog, &owner.id, &stale, None)
                .await
                .unwrap_err()
                .code,
            "version_conflict"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_access_risk_snapshot_survives_edits_and_deduplicates_explanations() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store.clone()).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [82; 16])
        .await
        .unwrap();
    let catalog = Catalog::new(accounts);
    let input = selection(json!({"kind":"items","items":[{"capability":"exec",
        "sandbox_level":"workspace-write","selector":{"paths":["/"],"argv_prefix":[["docker"]],"hosts":["*.docker.io"]}}]}));
    let before = prepare(&catalog, &owner.id, &input, Some(GrantDuration::Hours(1)))
        .await
        .unwrap();
    let risks = &before.items[0].risks;
    assert!(risks.iter().any(|risk| risk.id == "wsl-windows-drive"));
    assert!(risks.iter().any(|risk| risk.id == "docker-runtime-exec"));
    assert!(risks.iter().any(|risk| risk.id == "net-egress-broad"));
    let old = risks
        .iter()
        .find(|r| r.id == "exec-runs-code")
        .unwrap()
        .clone();
    catalog
        .put(
            &owner.id,
            Kind::Risks,
            &old.id,
            old.version,
            &json!({"plain_text":"Updated explanation for future requests"}),
            crate::catalog::WriteOptions {
                request_id: None,
                confirm_default: None,
            },
        )
        .await
        .unwrap();
    catalog
        .put(
            &owner.id,
            Kind::CapabilityRisks,
            "exec:duplicate",
            0,
            &json!({"capability_id":"exec","risk_id":"exec-runs-code","when_selector":{}}),
            crate::catalog::WriteOptions {
                request_id: None,
                confirm_default: None,
            },
        )
        .await
        .unwrap();
    let after = prepare(&catalog, &owner.id, &input, Some(GrantDuration::Hours(1)))
        .await
        .unwrap();
    let new = after.items[0]
        .risks
        .iter()
        .find(|r| r.id == old.id)
        .unwrap();
    assert_eq!(new.version, old.version + 1);
    assert_ne!(new.plain_text, old.plain_text);
    assert_eq!(old.conditions.len(), 1);
    assert_eq!(new.conditions.len(), 2);
    assert_eq!(
        after.items[0]
            .risks
            .iter()
            .filter(|r| r.id == old.id)
            .count(),
        1
    );
    assert_eq!(
        before.items[0]
            .risks
            .iter()
            .find(|r| r.id == old.id)
            .unwrap()
            .plain_text,
        old.plain_text
    );
    let wire = serde_json::to_value(&before).unwrap();
    let recovered: Prepared = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(recovered).unwrap(), wire);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_access_validates_schema_shell_os_durations_and_live_account() {
    let test = or_skip!(store::testing::postgres());
    let accounts = Arc::new(Accounts::open(test.store.clone()).await.unwrap());
    let owner = accounts
        .ensure_owner(None, "Owner", [83; 16])
        .await
        .unwrap();
    let catalog = Catalog::new(accounts.clone());
    let empty = Selection::Items { items: vec![] };
    assert_eq!(
        prepare(&catalog, &owner.id, &empty, None)
            .await
            .unwrap_err()
            .code,
        "invalid"
    );
    // A typed field still has to belong to this capability's catalog schema.
    let wrong = selection(json!({"kind":"items","items":[{"capability":"fs.read",
        "sandbox_level":"read-only","selector":{"hosts":["example.com"]}}]}));
    assert_eq!(
        prepare(&catalog, &owner.id, &wrong, None)
            .await
            .unwrap_err()
            .code,
        "invalid"
    );
    catalog
        .put(
            &owner.id,
            Kind::ShellProfiles,
            "test.windows",
            0,
            &json!({"os":"windows","label":"Windows test","argv_prefix":["cmd.exe"]}),
            crate::catalog::WriteOptions {
                request_id: None,
                confirm_default: None,
            },
        )
        .await
        .unwrap();
    let shell = selection(json!({"kind":"items","items":[{"capability":"exec",
        "sandbox_level":"read-only","selector":{"shell_profile":"test.windows"}}]}));
    assert_eq!(
        prepare(&catalog, &owner.id, &shell, None)
            .await
            .unwrap_err()
            .code,
        "invalid"
    );
    let mixed = selection(json!({"kind":"items","items":[{"capability":"exec",
        "sandbox_level":"read-only"},{"capability":"fs.read","sandbox_level":"read-only"}]}));
    let exec = catalog
        .get(&owner.id, Kind::Capabilities, "exec")
        .await
        .unwrap();
    catalog
        .put(
            &owner.id,
            Kind::Capabilities,
            "exec",
            exec.version,
            &json!({"default_duration":"1h"}),
            crate::catalog::WriteOptions {
                request_id: None,
                confirm_default: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        prepare(&catalog, &owner.id, &mixed, None)
            .await
            .unwrap_err()
            .code,
        "invalid"
    );
    let explicit = prepare(&catalog, &owner.id, &mixed, Some(GrantDuration::Once))
        .await
        .unwrap();
    assert_eq!(explicit.duration, GrantDuration::Once);
    // Account scoping cannot turn an unknown caller into an environment member.
    assert!(
        prepare(&catalog, "acct_unknown", &mixed, Some(GrantDuration::Once))
            .await
            .is_err()
    );
    transaction!(&accounts.store, |c| {
        accounts.store.scope(c, &owner.tenant).await?;
        sqlx::query("UPDATE auth_accounts SET disabled_at=1 WHERE id=$1")
            .bind(&owner.id)
            .execute(c)
            .await?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        prepare(&catalog, &owner.id, &mixed, Some(GrantDuration::Once))
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
}
