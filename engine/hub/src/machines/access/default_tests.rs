use super::request_tests::Fixture;
use crate::{
    catalog::{Kind, WriteOptions},
    store,
};
use serde_json::json;

fn options<'a>(key: &'a str, confirm: Option<&'a str>) -> WriteOptions<'a> {
    WriteOptions {
        request_id: Some(key),
        confirm_default: confirm,
    }
}
const FULL: &str = "experimental-full-access";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_default_requires_exact_confirmation_and_environment_policy_even_on_replay() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let catalog = &f.access.catalog;
    for confirm in [
        None,
        Some("Experimental-full-access"),
        Some("experimental-full-access "),
    ] {
        assert_eq!(
            catalog
                .choose_default(&f.owner.id, FULL, 1, options("choose", confirm))
                .await
                .unwrap_err()
                .code,
            "confirm_required"
        );
    }
    assert_eq!(
        catalog
            .choose_default(&f.owner.id, FULL, 1, options("choose", Some(FULL)))
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    let mut edit = f.policy_edit().await;
    f.access
        .set_policy(&f.owner.id, Some(&f.machine), &edit, "machine-policy")
        .await
        .unwrap();
    assert_eq!(
        catalog
            .choose_default(&f.owner.id, FULL, 1, options("choose", Some(FULL)))
            .await
            .unwrap_err()
            .code,
        "policy_denied",
        "machine-only policy cannot set an environment default"
    );
    f.access
        .set_policy(&f.owner.id, None, &edit, "environment-policy")
        .await
        .unwrap();
    let selected = catalog
        .choose_default(&f.owner.id, FULL, 1, options("choose", Some(FULL)))
        .await
        .unwrap();
    assert_eq!(selected.result.is_default, Some(true));
    assert_eq!(selected.result.version, 2);
    assert!(
        catalog
            .choose_default(&f.owner.id, FULL, 1, options("choose", Some(FULL)))
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(
        catalog
            .choose_default(&f.owner.id, FULL, 1, options("choose", None))
            .await
            .unwrap_err()
            .code,
        "request_conflict"
    );
    edit.allowed = false;
    edit.version = 1;
    f.access
        .set_policy(&f.owner.id, None, &edit, "disable-environment")
        .await
        .unwrap();
    assert_eq!(
        catalog
            .choose_default(&f.owner.id, FULL, 1, options("choose", Some(FULL)))
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    assert_eq!(
        catalog
            .choose_default(
                &f.owner.id,
                FULL,
                2,
                options("already-selected", Some(FULL))
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    catalog
        .choose_default(&f.owner.id, "read-only-look", 1, WriteOptions::default())
        .await
        .unwrap();
    assert_eq!(
        f.lifecycle_counts().await,
        (0, 0, 0),
        "a preset default never grants access"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_default_edit_cannot_hide_unsafe_items_behind_safe_metadata() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let catalog = &f.access.catalog;
    let patch =
        json!({"items":[{"capability":"exec","sandbox_level":"unsandboxed","selector":{}}]});
    assert_eq!(
        catalog
            .put(
                &f.owner.id,
                Kind::GrantPresets,
                "task-scoped",
                1,
                &patch,
                options("edit", None)
            )
            .await
            .unwrap_err()
            .code,
        "confirm_required"
    );
    assert_eq!(
        catalog
            .put(
                &f.owner.id,
                Kind::GrantPresets,
                "task-scoped",
                1,
                &patch,
                options("edit", Some("task-scoped"))
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    let edit = f.policy_edit().await;
    f.access
        .set_policy(&f.owner.id, None, &edit, "environment-policy")
        .await
        .unwrap();
    let made = catalog
        .put(
            &f.owner.id,
            Kind::GrantPresets,
            "task-scoped",
            1,
            &patch,
            options("edit", Some("task-scoped")),
        )
        .await
        .unwrap();
    assert_eq!(made.result.data["full_access"], false);
    assert!(
        catalog
            .put(
                &f.owner.id,
                Kind::GrantPresets,
                "task-scoped",
                1,
                &patch,
                options("edit", Some("task-scoped"))
            )
            .await
            .unwrap()
            .replayed
    );
    sqlx::query("UPDATE machine_sandbox_policies SET expires_at=enabled_at+1 WHERE tenant_id=$1 AND scope='environment'")
        .bind(&f.owner.tenant).execute(f.access.accounts.store.admin()).await.unwrap();
    assert_eq!(
        catalog
            .put(
                &f.owner.id,
                Kind::GrantPresets,
                "task-scoped",
                1,
                &patch,
                options("edit", Some("task-scoped"))
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
    assert_eq!(
        catalog
            .choose_default(
                &f.owner.id,
                "task-scoped",
                2,
                options("selected", Some("task-scoped"))
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn access_default_broad_gui_input_requires_policy_and_confirmation() {
    let test = or_skip!(store::testing::postgres());
    let f = Fixture::new(test.store).await;
    let catalog = &f.access.catalog;
    let patch = json!({"label":"Desktop", "items":[{"capability":"gui.input","sandbox_level":"desktop","selector":{"apps":"any"}}],"duration":"1h"});
    catalog
        .put(
            &f.owner.id,
            Kind::GrantPresets,
            "custom.desktop",
            0,
            &patch,
            WriteOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        catalog
            .choose_default(&f.owner.id, "custom.desktop", 1, WriteOptions::default())
            .await
            .unwrap_err()
            .code,
        "confirm_required"
    );
    assert_eq!(
        catalog
            .choose_default(
                &f.owner.id,
                "custom.desktop",
                1,
                options("choose", Some("custom.desktop"))
            )
            .await
            .unwrap_err()
            .code,
        "policy_denied"
    );
}
