//! Environment roster and role membership share the registered profile authority.
use super::*;

impl Accounts {
    /// Owners see this environment; other people see only their own agents. The
    /// composite cursor is stable even when two accounts use the same profile id.
    pub async fn principal_roster(
        &self,
        actor: &str,
        account: Option<&str>,
        after: Option<&str>,
        role: Option<&str>,
    ) -> BridgeResult<Vec<Agent>> {
        if after.is_some_and(|cursor| {
            cursor.split_once('/').is_none_or(|(account, id)| {
                !valid_account_id(account) || !devagotchi_agenthub::valid_name(id)
            })
        }) || role.is_some_and(|role| !devagotchi_agenthub::valid_name(role))
        {
            return Err(BridgeError::new("invalid", "invalid roster cursor or role"));
        }
        let (after_account, after_id) = after
            .and_then(|cursor| cursor.split_once('/'))
            .unwrap_or(("", ""));
        transaction!(&self.store, |connection| {
            let manager = self
                .agent_manager_in(connection, actor, account.unwrap_or(actor))
                .await?;
            let account = if manager.owner { account } else { Some(actor) };
            let sql = format!(
                "SELECT {COLUMNS} FROM auth_agents WHERE tenant_id=$1
                 AND ($2::text IS NULL OR account=$2)
                 AND (account COLLATE \"C\",id COLLATE \"C\") > ($3,$4)
                 AND ($5::text IS NULL OR roles::jsonb @> jsonb_build_array($5::text))
                 ORDER BY account COLLATE \"C\",id COLLATE \"C\" LIMIT 101"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&manager.tenant)
                .bind(account)
                .bind(after_account)
                .bind(after_id)
                .bind(role)
                .fetch_all(&mut *connection)
                .await?;
            rows.iter().map(stored).collect()
        })
    }
}
