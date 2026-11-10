//! Organisations on both store backends (SQLite always, Postgres when
//! `DEVAGOTCHI_TEST_POSTGRES_URL` names a server, as CI's `hub (postgres)` job does).

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use super::*;
use crate::store::testing;

fn clocked(store: Store) -> (Orgs, Arc<AtomicI64>) {
    let now = Arc::new(AtomicI64::new(1_800_000_000));
    let clock = now.clone();
    (
        Orgs::with_clock(store, Arc::new(move || clock.load(Ordering::SeqCst))),
        now,
    )
}

fn code<T: std::fmt::Debug>(result: BridgeResult<T>) -> String {
    result.expect_err("refused").code
}

#[tokio::test]
async fn an_organisation_has_its_maker_as_owner_and_a_name_nobody_else_takes() {
    for test in testing::stores().await {
        let (orgs, _) = clocked(test.store.clone());
        let acme = orgs.create("alice", "acme", "Acme Corp").await.unwrap();
        assert!(valid_tenant_id(&acme.id));
        assert_eq!(acme.slug, "acme");
        assert_eq!(orgs.by_slug("acme").await.unwrap(), Some(acme.clone()));
        assert_eq!(
            orgs.role_in(&acme, "alice").await.unwrap(),
            Some(Role::Owner)
        );
        assert_eq!(orgs.role_in(&acme, "bob").await.unwrap(), None);
        assert_eq!(
            code(orgs.create("bob", "acme", "Another").await),
            "name_taken"
        );
        assert_eq!(code(orgs.create("bob", "Acme", "Bad").await), "invalid");
        assert_eq!(code(orgs.create("bob", "ok", " ").await), "invalid");
        assert_eq!(
            orgs.of_subject("alice").await.unwrap(),
            vec![(acme.clone(), Role::Owner)]
        );
        assert!(orgs.of_subject("bob").await.unwrap().is_empty());
        // An account's own tenant is no organisation: its slug never answers.
        assert_eq!(orgs.by_slug("nobody").await.unwrap(), None);
    }
}

#[tokio::test]
async fn admins_manage_members_below_them_and_the_last_owner_stays() {
    for test in testing::stores().await {
        let (orgs, _) = clocked(test.store.clone());
        let acme = orgs.create("alice", "acme", "Acme").await.unwrap();
        orgs.set_member(&acme, "alice", "bob", Role::Admin)
            .await
            .unwrap();
        orgs.set_member(&acme, "bob", "carol", Role::Member)
            .await
            .unwrap();
        orgs.set_member(&acme, "bob", "robot", Role::Agent)
            .await
            .unwrap();

        // An admin never makes an owner, never touches one, never grants above itself.
        assert_eq!(
            code(orgs.set_member(&acme, "bob", "carol", Role::Owner).await),
            "not_permitted"
        );
        assert_eq!(
            code(orgs.set_member(&acme, "bob", "alice", Role::Member).await),
            "not_permitted"
        );
        assert_eq!(
            code(orgs.remove_member(&acme, "bob", "alice").await),
            "not_permitted"
        );
        // A member manages nobody; a stranger is not a member at all.
        assert_eq!(
            code(orgs.set_member(&acme, "carol", "dave", Role::Viewer).await),
            "not_permitted"
        );
        assert_eq!(
            code(
                orgs.set_member(&acme, "mallory", "dave", Role::Viewer)
                    .await
            ),
            "not_found"
        );

        // The last owner neither leaves nor steps down.
        assert_eq!(
            code(orgs.set_member(&acme, "alice", "alice", Role::Admin).await),
            "last_owner"
        );
        assert_eq!(
            code(orgs.remove_member(&acme, "alice", "alice").await),
            "last_owner"
        );
        orgs.set_member(&acme, "alice", "bob", Role::Owner)
            .await
            .unwrap();
        orgs.set_member(&acme, "alice", "alice", Role::Admin)
            .await
            .unwrap();
        assert_eq!(
            orgs.role_in(&acme, "alice").await.unwrap(),
            Some(Role::Admin)
        );

        // A member may leave; leaving takes its teams and project rows with it.
        orgs.create_team(&acme, "bob", "web", "Web").await.unwrap();
        orgs.set_team_member(&acme, "bob", "web", "carol", true)
            .await
            .unwrap();
        orgs.remove_member(&acme, "carol", "carol").await.unwrap();
        assert_eq!(orgs.role_in(&acme, "carol").await.unwrap(), None);
        assert!(orgs.teams(&acme).await.unwrap()[0].members.is_empty());

        let members: Vec<(String, Role)> = orgs
            .members(&acme)
            .await
            .unwrap()
            .into_iter()
            .map(|m| (m.subject, m.role))
            .collect();
        assert_eq!(
            members,
            vec![
                ("alice".into(), Role::Admin),
                ("bob".into(), Role::Owner),
                ("robot".into(), Role::Agent),
            ]
        );
    }
}

#[tokio::test]
async fn a_project_role_is_the_higher_of_the_organisations_and_its_own() {
    for test in testing::stores().await {
        let (orgs, _) = clocked(test.store.clone());
        let acme = orgs.create("alice", "acme", "Acme").await.unwrap();
        for (who, role) in [
            ("bob", Role::Member),
            ("vic", Role::Viewer),
            ("ada", Role::Admin),
        ] {
            orgs.set_member(&acme, "alice", who, role).await.unwrap();
        }
        let site = ProjectRef {
            tenant: acme.id.clone(),
            owner: owner_of(&acme.id),
            project: "site".into(),
        };
        orgs.register_project(&acme, "bob", "site", Visibility::Org)
            .await
            .unwrap();
        assert_eq!(
            orgs.project_role(&site, "bob").await.unwrap(),
            Some(Role::Member)
        );
        assert_eq!(
            orgs.project_role(&site, "vic").await.unwrap(),
            Some(Role::Viewer)
        );
        assert_eq!(orgs.project_role(&site, "stranger").await.unwrap(), None);

        // A viewer raised on one project, through a team.
        orgs.create_team(&acme, "ada", "web", "Web").await.unwrap();
        orgs.set_team_member(&acme, "ada", "web", "vic", true)
            .await
            .unwrap();
        orgs.set_project_member(&site, "ada", None, Principal::Team, "web", Role::Agent)
            .await
            .unwrap();
        assert_eq!(
            orgs.project_role(&site, "vic").await.unwrap(),
            Some(Role::Agent)
        );

        // Members-only: the organisation's member no longer reaches it, its admins do, and
        // an outside collaborator added by name does.
        orgs.set_visibility(&acme, "ada", "site", Visibility::Members)
            .await
            .unwrap();
        assert_eq!(orgs.project_role(&site, "bob").await.unwrap(), None);
        assert_eq!(
            orgs.project_role(&site, "ada").await.unwrap(),
            Some(Role::Admin)
        );
        assert_eq!(
            orgs.project_role(&site, "vic").await.unwrap(),
            Some(Role::Agent)
        );
        orgs.set_project_member(
            &site,
            "ada",
            None,
            Principal::Subject,
            "guest",
            Role::Member,
        )
        .await
        .unwrap();
        assert_eq!(
            orgs.project_role(&site, "guest").await.unwrap(),
            Some(Role::Member)
        );
        assert_eq!(
            orgs.projects_of("guest").await.unwrap(),
            vec![(site.clone(), Role::Member)]
        );
        assert_eq!(
            orgs.projects_of("vic").await.unwrap(),
            vec![(site.clone(), Role::Agent)]
        );

        // Nobody grants above themselves or names a team that does not exist; a member
        // changes nothing about who works the project.
        assert_eq!(
            code(
                orgs.set_project_member(
                    &site,
                    "guest",
                    None,
                    Principal::Subject,
                    "x",
                    Role::Viewer
                )
                .await
            ),
            "not_permitted"
        );
        assert_eq!(
            code(
                orgs.set_project_member(&site, "ada", None, Principal::Team, "nope", Role::Viewer)
                    .await
            ),
            "not_found"
        );
        assert_eq!(
            code(
                orgs.set_project_member(&site, "ada", None, Principal::Subject, "x", Role::Owner)
                    .await
            ),
            "invalid"
        );
        // A configured board's grant counts as the caller's floor.
        orgs.set_project_member(
            &site,
            "cfg",
            Some(Role::Admin),
            Principal::Subject,
            "x",
            Role::Viewer,
        )
        .await
        .unwrap();

        // People: who decides approvals there.
        assert_eq!(
            orgs.people(&site).await.unwrap(),
            vec!["ada".to_string(), "alice".into(), "guest".into()]
        );
        // A project with no access row of its own is the organisation's to see.
        let visible = orgs
            .visible(&acme, &["site".to_string(), "other".to_string()], "bob")
            .await
            .unwrap();
        assert_eq!(visible, vec![("other".to_string(), Role::Member)]);

        // Deleting the team takes what it gave.
        orgs.delete_team(&acme, "ada", "web").await.unwrap();
        assert_eq!(orgs.project_role(&site, "vic").await.unwrap(), None);
        orgs.remove_project_member(&site, "guest", None, Principal::Subject, "guest")
            .await
            .unwrap();
        assert_eq!(orgs.project_role(&site, "guest").await.unwrap(), None);
    }
}

#[tokio::test]
async fn an_invite_works_once_before_it_runs_out_and_never_above_its_maker() {
    for test in testing::stores().await {
        let (orgs, now) = clocked(test.store.clone());
        let acme = orgs.create("alice", "acme", "Acme").await.unwrap();
        orgs.set_member(&acme, "alice", "ada", Role::Admin)
            .await
            .unwrap();
        assert_eq!(
            code(orgs.invite(&acme, "ada", Role::Owner, None).await),
            "not_permitted"
        );
        let invite = orgs
            .invite(&acme, "ada", Role::Member, Some("for Bob"))
            .await
            .unwrap();
        assert!(invite.token.starts_with("inv_"));
        assert_eq!(orgs.invites(&acme, "ada").await.unwrap().len(), 1);

        let (org, role) = orgs.accept(&invite.token, "bob").await.unwrap();
        assert_eq!((org.slug.as_str(), role), ("acme", Role::Member));
        assert_eq!(code(orgs.accept(&invite.token, "carol").await), "spent");
        assert_eq!(code(orgs.accept("inv_made-up", "carol").await), "not_found");
        assert!(orgs.invites(&acme, "ada").await.unwrap().is_empty());

        // An invite never lowers who takes it.
        let viewer = orgs.invite(&acme, "ada", Role::Viewer, None).await.unwrap();
        assert_eq!(
            orgs.accept(&viewer.token, "ada").await.unwrap().1,
            Role::Admin
        );

        let late = orgs
            .invite(&acme, "alice", Role::Agent, None)
            .await
            .unwrap();
        now.fetch_add(INVITE_TTL_SECS, Ordering::SeqCst);
        assert_eq!(code(orgs.accept(&late.token, "dave").await), "expired");

        let withdrawn = orgs
            .invite(&acme, "alice", Role::Agent, None)
            .await
            .unwrap();
        orgs.revoke_invite(&acme, "alice", &withdrawn.id)
            .await
            .unwrap();
        assert_eq!(code(orgs.accept(&withdrawn.token, "dave").await), "revoked");
        assert_eq!(
            code(orgs.revoke_invite(&acme, "alice", &withdrawn.id).await),
            "not_found"
        );
        assert_eq!(code(orgs.invites(&acme, "bob").await), "not_permitted");
    }
}

#[tokio::test]
async fn every_change_lands_on_the_organisations_own_chain() {
    for test in testing::stores().await {
        let (orgs, _) = clocked(test.store.clone());
        let acme = orgs.create("alice", "acme", "Acme").await.unwrap();
        orgs.set_member(&acme, "alice", "bob", Role::Member)
            .await
            .unwrap();
        orgs.remove_member(&acme, "alice", "bob").await.unwrap();
        let events: Vec<String> = transaction!(&test.store, |connection| {
            test.store.scope(connection, &acme.id).await?;
            Ok(sqlx::query_scalar(
                "SELECT event FROM tenancy_events WHERE tenant_id = $1 ORDER BY seq",
            )
            .bind(&acme.id)
            .fetch_all(&mut *connection)
            .await?)
        })
        .unwrap();
        assert_eq!(
            events,
            vec!["org.created", "org.member_set", "org.member_removed"]
        );
    }
}

/// Two owners each demoting the other at once: one wins, and the organisation is never
/// left without an owner. Two people taking the same invite at once: one gets in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_changes_never_leave_an_organisation_without_an_owner() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    for round in 0..6 {
        let orgs = Orgs::new(test.store.clone());
        let org = orgs
            .create("alice", &format!("race-{round}"), "Race")
            .await
            .unwrap();
        orgs.set_member(&org, "alice", "bob", Role::Owner)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            orgs.set_member(&org, "alice", "bob", Role::Member),
            orgs.set_member(&org, "bob", "alice", Role::Member),
        );
        let owners: Vec<String> = orgs
            .members(&org)
            .await
            .unwrap()
            .into_iter()
            .filter(|m| m.role == Role::Owner)
            .map(|m| m.subject)
            .collect();
        assert_eq!(owners.len(), 1, "round {round}: {a:?} {b:?}");
        assert!(a.is_ok() != b.is_ok(), "round {round}: {a:?} {b:?}");

        let invite = orgs
            .invite(&org, &owners[0], Role::Member, None)
            .await
            .unwrap();
        let (x, y) = tokio::join!(
            orgs.accept(&invite.token, "xavier"),
            orgs.accept(&invite.token, "yolanda"),
        );
        assert!(x.is_ok() != y.is_ok(), "round {round}: {x:?} {y:?}");
    }
}

/// Postgres holds an organisation's rows to its tenant: a transaction scoped to another
/// tenant sees none of them, and the cross-tenant functions answer ids and roles only.
#[tokio::test]
async fn row_level_security_keeps_each_organisations_rows_to_itself() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    let orgs = Orgs::new(test.store.clone());
    let acme = orgs.create("alice", "acme", "Acme").await.unwrap();
    let other = orgs.create("mallory", "other", "Other").await.unwrap();
    orgs.set_member(&acme, "alice", "bob", Role::Member)
        .await
        .unwrap();
    let seen: i64 = transaction!(&test.store, |connection| {
        test.store.scope(connection, &other.id).await?;
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM org_members WHERE tenant_id = $1")
                .bind(&acme.id)
                .fetch_one(&mut *connection)
                .await?,
        )
    })
    .unwrap();
    assert_eq!(seen, 0);
    assert_eq!(
        orgs.of_subject("bob").await.unwrap(),
        vec![(acme.clone(), Role::Member)]
    );
}
