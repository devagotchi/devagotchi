//! Current account/profile proof composed into machine-access transactions.
use super::*;
use devagotchi_agenthub::{Actor, policy::catalog};

impl Accounts {
    /// Resolve only a middleware-authenticated subject. Never accept a tool's actor,
    /// seat, kind or roles as the registered machine principal.
    pub(crate) async fn machine_identity(
        &self,
        subject: &crate::identity::Subject,
    ) -> BridgeResult<Option<devagotchi_agenthub::policy::Identity>> {
        if subject.org.is_some() || subject.agent.is_none() && matches!(subject.via, "hub" | "key")
        {
            return Err(BridgeError::new(
                "not_granted",
                "machine access needs a person or credential-bound registered agent",
            ));
        }
        transaction!(&self.store, |c| {
            let id = subject.agent.as_deref().unwrap_or(&subject.id);
            let profile = self.machine_profile_in(c, &subject.id, id).await?;
            if subject.agent.is_some() != profile.is_some() {
                return Err(BridgeError::new(
                    "not_granted",
                    "machine principal does not match its credential",
                )
                .into());
            }
            let actor = Actor {
                id,
                human: profile.is_none(),
                wip_limit: None,
                policy: profile.clone(),
            };
            let account = self.machine_actor_in(c, &subject.id, &actor).await?;
            if subject
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &account.tenant)
            {
                return Err(
                    BridgeError::new("not_granted", "machine account changed environment").into(),
                );
            }
            Ok(profile)
        })
    }
    /// Reconstruct a stored request's principal from the current identity directory.
    /// This is not authentication for a caller-supplied principal. The access owner
    /// loads account/principal from its scoped immutable request or grant record;
    /// machine_identity uses the principal bound by credential verification.
    /// No policy lock here: task-bound decisions take their board lock next, then use
    /// machine_actor_in to validate this snapshot under the principal-policy lock.
    pub(crate) async fn machine_profile_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
        principal: &str,
    ) -> Result<Option<devagotchi_agenthub::policy::Identity>, Failed> {
        let current = self.catalog_actor_in(c, account, false).await?;
        if principal == account && !current.provisional {
            return Ok(None);
        }
        let id = principal
            .strip_prefix(&format!("@agent:{account}:"))
            .filter(|id| devagotchi_agenthub::valid_name(id))
            .ok_or_else(|| {
                BridgeError::new("not_granted", "stored access principal is unavailable")
            })?;
        let agent = agents::agent_in(c, &current.tenant, account, id).await?;
        if !agent.enabled {
            return Err(
                BridgeError::new("not_granted", "stored access principal is disabled").into(),
            );
        }
        Ok(Some(devagotchi_agenthub::policy::Identity {
            kind: agent.kind,
            roles: agent.roles,
        }))
    }
    /// The authenticated adapter supplies the account and actor. A tool-selected seat,
    /// human flag or role list must never reach this method as identity. For task-bound
    /// calls the board lock is taken before any principal-policy/catalog lock.
    pub(crate) async fn machine_actor_in(
        &self,
        c: &mut AnyConnection,
        account: &str,
        actor: &Actor<'_>,
    ) -> Result<Account, Failed> {
        let current = self.catalog_actor_in(c, account, false).await?;
        let refuse = || {
            BridgeError::new(
                "not_granted",
                "machine access needs a current authenticated person or registered agent",
            )
        };
        if actor.human {
            if actor.id != account || actor.policy.is_some() || current.provisional {
                return Err(refuse().into());
            }
        } else {
            let (owner, id) = actor
                .id
                .strip_prefix("@agent:")
                .and_then(|id| id.split_once(':'))
                .ok_or_else(refuse)?;
            if owner != account || !devagotchi_agenthub::valid_name(id) {
                return Err(refuse().into());
            }
            let agent = agents::agent_in(c, &current.tenant, account, id).await?;
            if !agent.enabled
                || !actor
                    .policy
                    .as_ref()
                    .is_some_and(|policy| policy.kind == agent.kind && policy.roles == agent.roles)
            {
                return Err(refuse().into());
            }
            catalog::check_profile_in(c, &current.tenant, &agent.kind, &agent.roles).await?;
        }
        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;
    use agents::AgentEdit;

    async fn check(accounts: &Accounts, account: &str, actor: &Actor<'_>) -> BridgeResult<Account> {
        transaction!(&accounts.store, |c| {
            accounts.machine_actor_in(c, account, actor).await
        })
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn machine_access_identity_refuses_seats_spoofed_people_and_changed_profiles() {
        let test = or_skip!(store::testing::postgres());
        let accounts = Accounts::open(test.store).await.unwrap();
        let owner = accounts
            .ensure_owner(None, "Owner", [94; 16])
            .await
            .unwrap();
        check(&accounts, &owner.id, &Actor::human(&owner.id))
            .await
            .unwrap();
        let mut edit = AgentEdit {
            id: "worker".into(),
            name: "Worker".into(),
            kind: "codex".into(),
            roles: vec!["worker".into()],
            enabled: true,
            wip_limit: None,
            version: 0,
        };
        let agent = accounts
            .put_agent(&owner.id, &owner.id, &edit)
            .await
            .unwrap();
        let principal = agent.principal();
        let actor = Actor {
            id: &principal,
            human: false,
            wip_limit: None,
            policy: Some(devagotchi_agenthub::policy::Identity {
                kind: agent.kind,
                roles: agent.roles,
            }),
        };
        check(&accounts, &owner.id, &actor).await.unwrap();
        for forged in [
            Actor::agent("worker"),
            Actor::agent(&principal),
            Actor::human(&principal),
        ] {
            assert_eq!(
                check(&accounts, &owner.id, &forged).await.unwrap_err().code,
                "not_granted"
            );
        }
        let mut forged = actor.clone();
        forged.policy.as_mut().unwrap().roles.push("admin".into());
        assert_eq!(
            check(&accounts, &owner.id, &forged).await.unwrap_err().code,
            "not_granted"
        );
        edit.version = agent.version;
        edit.roles = vec!["reviewer".into()];
        let changed = accounts
            .put_agent(&owner.id, &owner.id, &edit)
            .await
            .unwrap();
        assert_eq!(
            check(&accounts, &owner.id, &actor).await.unwrap_err().code,
            "not_granted"
        );
        let mut refreshed = actor.clone();
        refreshed.policy.as_mut().unwrap().roles = changed.roles;
        check(&accounts, &owner.id, &refreshed).await.unwrap();
        edit.version = changed.version;
        edit.enabled = false;
        accounts
            .put_agent(&owner.id, &owner.id, &edit)
            .await
            .unwrap();
        assert_eq!(
            check(&accounts, &owner.id, &refreshed)
                .await
                .unwrap_err()
                .code,
            "not_granted"
        );
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn machine_access_identity_refuses_foreign_account_and_disabled_owner() {
        let test = or_skip!(store::testing::postgres());
        let accounts = Accounts::open(test.store).await.unwrap();
        let owner = accounts
            .ensure_owner(None, "Owner", [95; 16])
            .await
            .unwrap();
        let other = accounts
            .create_local_user(&owner.id, "Other", "other@example.test", "a good password")
            .await
            .unwrap()
            .account;
        assert_eq!(
            check(&accounts, &owner.id, &Actor::human(&other.id))
                .await
                .unwrap_err()
                .code,
            "not_granted"
        );
        check(&accounts, &other.id, &Actor::human(&other.id))
            .await
            .unwrap();
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
            check(&accounts, &owner.id, &Actor::human(&owner.id))
                .await
                .unwrap_err()
                .code,
            "not_granted"
        );
    }
}
