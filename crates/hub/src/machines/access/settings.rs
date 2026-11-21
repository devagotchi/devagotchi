use super::*;
use devagotchi_contracts::GrantDuration;
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessSettings {
    pub version: i64,
    pub request_expiry_minutes: u32,
    pub deny_cooldown_minutes: u32,
    pub grant_duration_options: Vec<GrantDuration>,
    pub unsandboxed_risk_ids: Vec<String>,
    pub authority_ttl_ms: u32,
    pub resume_alert_idle_minutes: u32,
}
impl Default for AccessSettings {
    fn default() -> Self {
        Self {
            version: 1,
            request_expiry_minutes: 30,
            deny_cooldown_minutes: 10,
            grant_duration_options: vec![
                GrantDuration::Once,
                GrantDuration::Task,
                GrantDuration::Hours(1),
                GrantDuration::Hours(8),
                GrantDuration::UntilRevoked,
            ],
            unsandboxed_risk_ids: [
                "unsandboxed-any-file",
                "unsandboxed-network",
                "unsandboxed-persist",
            ]
            .map(str::to_owned)
            .into(),
            authority_ttl_ms: 10_000,
            resume_alert_idle_minutes: 15,
        }
    }
}
impl AccessSettings {
    pub(crate) fn validate(&self) -> BridgeResult<()> {
        if self.version < 1
            || !(1..=1440).contains(&self.request_expiry_minutes)
            || self.deny_cooldown_minutes > 1440
            || !(1000..=30_000).contains(&self.authority_ttl_ms)
            || !(1..=60).contains(&self.resume_alert_idle_minutes)
            || self.grant_duration_options.is_empty()
            || self.grant_duration_options.len() > 32
            || self.unsandboxed_risk_ids.is_empty()
            || self.unsandboxed_risk_ids.len() > 64
        {
            return Err(invalid("access settings exceed their documented bounds"));
        }
        for (i, duration) in self.grant_duration_options.iter().enumerate() {
            duration.validate().map_err(|e| invalid(e.message))?;
            if self.grant_duration_options[..i].contains(duration) {
                return Err(invalid("access durations must be distinct"));
            }
        }
        for (i, id) in self.unsandboxed_risk_ids.iter().enumerate() {
            if id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
                || self.unsandboxed_risk_ids[..i].contains(id)
            {
                return Err(invalid(
                    "acknowledged risk IDs must be distinct catalog IDs",
                ));
            }
        }
        Ok(())
    }
}
impl Access {
    pub(crate) async fn settings(&self, actor: &str) -> BridgeResult<AccessSettings> {
        transaction!(&self.accounts.store, |c| {
            let tenant = self
                .owner_in(c, actor, "admin.access.settings.read")
                .await?;
            Self::lock_in(c, &tenant).await?;
            Self::settings_in(c, &tenant).await
        })
    }
    /// Caller holds access lock after any board/principal-policy/catalog/machine locks.
    pub(super) async fn settings_in(
        c: &mut AnyConnection,
        tenant: &str,
    ) -> DbResult<AccessSettings> {
        let default = AccessSettings::default();
        sqlx::query(
            "INSERT INTO machine_access_settings(tenant_id,version,value) VALUES($1,1,$2::jsonb)
            ON CONFLICT(tenant_id) DO NOTHING",
        )
        .bind(tenant)
        .bind(encoded(&default)?)
        .execute(&mut *c)
        .await?;
        let (version, text): (i64, String) = sqlx::query_as(
            "SELECT version,value::text FROM machine_access_settings WHERE tenant_id=$1",
        )
        .bind(tenant)
        .fetch_one(c)
        .await?;
        let value: AccessSettings = serde_json::from_str(&text)
            .map_err(|_| refused("access_damaged", "invalid stored access settings"))?;
        if version != value.version || value.validate().is_err() {
            return Err(refused("access_damaged", "invalid stored access settings"));
        }
        Ok(value)
    }
    pub(crate) async fn put_settings(
        &self,
        actor: &str,
        input: &AccessSettings,
        key: &str,
    ) -> BridgeResult<Changed<AccessSettings>> {
        input.validate()?;
        let original =
            serde_json::to_value(input).map_err(|_| invalid("invalid access settings"))?;
        transaction!(&self.accounts.store, |c| {
            let tenant = self.owner_in(c, actor, "admin.access.settings.put").await?;
            self.catalog
                .access_risks_in(c, actor, &input.unsandboxed_risk_ids)
                .await?;
            Self::lock_in(c, &tenant).await?;
            let request = Request::new(&tenant, actor, key, "access_settings_put", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid access settings receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(reservation) => reservation,
            };
            let current = Self::settings_in(c, &tenant).await?;
            if current.version != input.version {
                return Err(refused(
                    "version_conflict",
                    "access settings changed; reload before saving",
                ));
            }
            let mut saved = input.clone();
            saved.version = saved
                .version
                .checked_add(1)
                .ok_or_else(|| invalid("access settings version exhausted"))?;
            sqlx::query(
                "UPDATE machine_access_settings SET version=$2,value=$3::jsonb WHERE tenant_id=$1",
            )
            .bind(&tenant)
            .bind(saved.version)
            .bind(encoded(&saved)?)
            .execute(&mut *c)
            .await?;
            let now = Self::now_in(c).await?;
            Self::audit_in(
                c,
                &tenant,
                actor,
                "access.settings_changed",
                &json!({"version":saved.version}),
                now,
            )
            .await?;
            reservation.complete_in(c, &saved).await?;
            Ok(Changed {
                result: saved,
                replayed: false,
            })
        })
    }
}
