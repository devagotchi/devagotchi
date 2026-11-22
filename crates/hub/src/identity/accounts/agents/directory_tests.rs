use super::*;
use crate::tenancy::orgs::{self, Orgs, ProjectRef, Role, Visibility};
use devagotchi_agenthub::{Actor, hosted::PrincipalDirectory};

async fn authorized(accounts: &Accounts, at: &ProjectRef, principal: &str) -> BridgeResult<()> {
    let directory = Directory(Default::default(), Some(at.clone()));
    transaction!(&accounts.store, |connection| {
        accounts.store.scope(connection, &at.tenant).await?;
        // The restricted board connection cannot read the member's home account.
        assert!(account_in(connection, principal).await?.is_none());
        directory
            .authorize_request(connection, &at.tenant, principal, &Actor::human(principal))
            .await?;
        let scope: String = sqlx::query_scalar("SELECT current_setting('devagotchi.tenant')")
            .fetch_one(connection)
            .await?;
        assert_eq!(scope, at.tenant);
        Ok(())
    })
}

#[tokio::test]
async fn organization_requests_recheck_live_accounts_and_exact_project_roles_without_rescoping() {
    let (accounts, owner) = or_skip!(super::tests::setup());
    let user = accounts
        .create_local_user(
            &owner.id,
            "Member",
            "org-member@example.test",
            "a good password",
        )
        .await
        .unwrap();
    let orgs = Orgs::new(accounts.store.clone());
    let org = orgs.create(&owner.id, "acme", "Acme").await.unwrap();
    orgs.set_member(&org, &owner.id, &user.account.id, Role::Member)
        .await
        .unwrap();
    orgs.register_project(&org, &owner.id, "site", Visibility::Org)
        .await
        .unwrap();
    orgs.register_project(&org, &owner.id, "private", Visibility::Members)
        .await
        .unwrap();
    let at = ProjectRef {
        tenant: org.id.clone(),
        owner: orgs::owner_of(&org.id),
        project: "site".into(),
    };
    authorized(&accounts, &at, &user.account.id).await.unwrap();
    let private = ProjectRef {
        project: "private".into(),
        ..at.clone()
    };
    assert_eq!(
        authorized(&accounts, &private, &user.account.id)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    orgs.set_member(&org, &owner.id, &user.account.id, Role::Viewer)
        .await
        .unwrap();
    assert_eq!(
        authorized(&accounts, &at, &user.account.id)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
    orgs.set_member(&org, &owner.id, &user.account.id, Role::Member)
        .await
        .unwrap();
    authorized(&accounts, &at, &user.account.id).await.unwrap();
    accounts
        .update_local_user(&owner.id, &user.account.id, user.version, "Member", false)
        .await
        .unwrap();
    assert_eq!(
        authorized(&accounts, &at, &user.account.id)
            .await
            .unwrap_err()
            .code,
        "not_granted"
    );
}
