//! Replay only revocation's empty result, never an issued or rotated key secret.
use super::super::{
    Lookup, account_in,
    receipts::{MutationResult, Receipt},
};
use super::*;

impl Accounts {
    #[cfg(test)]
    pub async fn revoke_key(&self, tenant: &str, account: &str, id: &str) -> BridgeResult<()> {
        self.revoke_key_requested(tenant, account, id, None)
            .await
            .map(|result| result.value)
    }

    pub async fn revoke_key_requested(
        &self,
        tenant: &str,
        account: &str,
        id: &str,
        request_key: Option<&str>,
    ) -> BridgeResult<MutationResult<()>> {
        transaction!(&self.store, |connection| {
            let missing = || BridgeError::new("not_found", "no such live key on this account");
            if self
                .enter(connection, Lookup::Account(account))
                .await?
                .is_none()
            {
                return Err(missing().into());
            }
            let live = account_in(connection, account)
                .await?
                .filter(|live| {
                    live.tenant == tenant
                        && live.disabled_at.is_none()
                        && !self.lapsed(live, now_secs())
                })
                .ok_or_else(missing)?;
            let receipt = Receipt::begin(
                connection,
                &live.tenant,
                account,
                request_key,
                "revoke_key",
                &id,
            )
            .await?;
            if let Receipt::Replay(saved) = receipt {
                return Ok(MutationResult::replay(saved));
            }
            self.end_key(connection, tenant, account, id, now_secs())
                .await?;
            receipt.complete(connection, ()).await
        })
    }
}
