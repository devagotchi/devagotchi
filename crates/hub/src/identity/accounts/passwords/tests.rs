use crate::config::{TenancyConfig, TenancyMode};
use crate::identity::accounts::Accounts;
use crate::store::testing;

#[tokio::test]
async fn an_operator_reset_keeps_the_account_and_changes_nothing_when_it_is_refused() {
    for store in testing::stores().await {
        // A hosted hub, so a second person can sign up and hold an address.
        let hosted = TenancyConfig {
            mode: TenancyMode::Multi,
            ..TenancyConfig::default()
        };
        let accounts = Accounts::open(store.store.clone())
            .await
            .unwrap()
            .with_tenancy(hosted, Vec::new());
        let owner = accounts
            .ensure_owner(None, "Owner", [91; 16])
            .await
            .unwrap();
        let other = accounts
            .sign_up_password("taken@example.com", "other password")
            .await
            .unwrap();
        let session = accounts.open_session(&owner.id, None).await.unwrap();
        let other_session = accounts.open_session(&other.id, None).await.unwrap();
        // Another account's address, an unknown account, a short password: refused, and the
        // owner's session is still there.
        let refused = [
            (
                owner.id.as_str(),
                "taken@example.com",
                "new password",
                "email_taken",
            ),
            (
                "missing-account",
                "new@example.com",
                "new password",
                "no_account",
            ),
            (
                owner.id.as_str(),
                "owner@example.com",
                "short",
                "password_short",
            ),
        ];
        for (account, email, password, code) in refused {
            let error = accounts
                .reset_password_for_email(account, email, password)
                .await
                .unwrap_err();
            assert_eq!(error.code, code, "{account} {email}");
            assert!(accounts.session(&session).await.unwrap().is_some());
        }
        assert!(accounts.get("missing-account").await.unwrap().is_none());
        accounts
            .reset_password_for_email(&owner.id, "Owner@Example.com", "first password")
            .await
            .unwrap();
        assert!(
            accounts.session(&session).await.unwrap().is_none(),
            "its sessions end"
        );
        assert!(
            accounts.session(&other_session).await.unwrap().is_some(),
            "no one else's"
        );
        let signed = accounts
            .sign_in_password("owner@example.com", "first password")
            .await
            .unwrap();
        assert_eq!(
            (signed.id.as_str(), signed.owner),
            (owner.id.as_str(), true)
        );
        assert_eq!(signed.tenant, owner.tenant);
        accounts
            .reset_password_for_email(&owner.id, "owner@example.com", "second password")
            .await
            .unwrap();
        let old = accounts
            .sign_in_password("owner@example.com", "first password")
            .await;
        assert_eq!(old.unwrap_err().code, "password_wrong");
        let new = accounts
            .sign_in_password("owner@example.com", "second password")
            .await;
        assert_eq!(new.unwrap().id, owner.id);
    }
}
