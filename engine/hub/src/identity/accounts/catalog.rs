//! Live identity for environment catalog operations, checked in their transaction.
use super::*;
impl Accounts {
    pub(crate) async fn catalog_actor_in(
        &self,
        c: &mut AnyConnection,
        actor: &str,
        owner: bool,
    ) -> Result<Account, Failed> {
        let account = self.agents_catalog_actor_in(c, actor).await?;
        if owner && !account.owner {
            return Err(BridgeError::new(
                "not_granted",
                "only the environment owner changes catalogs",
            )
            .into());
        }
        Ok(account)
    }
}
