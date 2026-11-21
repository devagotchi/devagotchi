//! Internal bounded environment enumeration; callers still scope each domain transaction.
use super::*;
impl Accounts {
    pub(crate) async fn maintenance_environments(
        &self,
        after: Option<&str>,
    ) -> BridgeResult<Vec<String>> {
        if after.is_some_and(|id| id.len() > 128 || id.chars().any(char::is_control)) {
            return Err(BridgeError::new("invalid", "invalid environment cursor"));
        }
        transaction!(&self.store, |c| {
            Ok(sqlx::query_scalar(
                "SELECT id FROM tenancy_tenants WHERE id>$1 ORDER BY id LIMIT 32",
            )
            .bind(after.unwrap_or(""))
            .fetch_all(c)
            .await?)
        })
    }
}
