//! Terminal grant changes and durable notification intent share one transaction.
use super::*;
use crate::machines::devices::MachineBinding;
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ended {
    pub grant_id: String,
    pub version: i64,
    pub machine: String,
    pub generation: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Revoked {
    pub machine: String,
    pub generation: String,
    pub count: u64,
}
impl Access {
    /// Already ended grants return None. Callers hold the environment access lock.
    /// No broker I/O occurs here; the outbox is durable even if this process crashes.
    pub(super) async fn end_grant_in(
        c: &mut AnyConnection,
        tenant: &str,
        id: &str,
        reason: &str,
        person: &str,
        now: i64,
    ) -> DbResult<Option<Ended>> {
        let row=sqlx::query("UPDATE machine_grants SET ended_at=$3,end_reason=$4,ended_by=$5,version=version+1
            WHERE tenant_id=$1 AND id=$2 AND ended_at IS NULL RETURNING version,machine_binding::text")
            .bind(tenant).bind(id).bind(now).bind(reason).bind(person).fetch_optional(&mut *c).await?;
        let Some(row) = row else { return Ok(None) };
        let binding: MachineBinding =
            serde_json::from_str(&row.try_get::<String, _>("machine_binding")?)
                .map_err(|_| refused("access_damaged", "invalid grant machine binding"))?;
        let ended = Ended {
            grant_id: id.into(),
            version: row.try_get("version")?,
            machine: binding.name,
            generation: binding.generation,
            reason: reason.into(),
        };
        sqlx::query("INSERT INTO machine_access_notifications(tenant_id,grant_id,version,machine,generation,reason,created_at,next_attempt_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$7)")
            .bind(tenant).bind(id).bind(ended.version).bind(&ended.machine).bind(&ended.generation).bind(reason).bind(now).execute(&mut *c).await?;
        Self::audit_in(
            c,
            tenant,
            person,
            "grant.ended",
            &json!({"grant":id,"reason":reason,"version":ended.version,"machine":ended.machine}),
            now,
        )
        .await?;
        Ok(Some(ended))
    }
    pub(crate) async fn revoke(
        &self,
        person: &str,
        id: &str,
        version: i64,
        key: &str,
    ) -> BridgeResult<Changed<Ended>> {
        if version < 1 {
            return Err(invalid("grant version must be positive"));
        }
        let original = json!({"id":id,"version":version});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.owner_in(c, person, "grant.revoke").await?;
            Self::lock_in(c, &tenant).await?;
            let request = Request::new(&tenant, person, key, "grant_revoke", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid revocation receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let current: Option<i64> = sqlx::query_scalar(
                "SELECT version FROM machine_grants WHERE tenant_id=$1 AND id=$2",
            )
            .bind(&tenant)
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
            match current {
                None => return Err(refused("not_found", "grant not found")),
                Some(v) if v != version => {
                    return Err(refused(
                        "version_conflict",
                        "grant changed; reload before revoking",
                    ));
                }
                _ => (),
            }
            let now = Self::now_in(c).await?;
            let ended = Self::end_grant_in(c, &tenant, id, "revoked", person, now)
                .await?
                .ok_or_else(|| refused("not_live", "grant already ended"))?;
            reservation.complete_in(c, &ended).await?;
            Ok(Changed {
                result: ended,
                replayed: false,
            })
        })
    }
    /// Account ownership is established by the authenticated machine adapter. Restrict
    /// the update to that account and enrollment so name reuse cannot affect a new device.
    pub(crate) async fn revoke_machine(
        &self,
        person: &str,
        account: &str,
        machine: &MachineBinding,
        key: &str,
    ) -> BridgeResult<Changed<Revoked>> {
        if machine.owner.as_deref() != Some(account) {
            return Err(invalid("machine owner does not match access account"));
        }
        let original = json!({"account":account,"machine":machine});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.owner_in(c, person, "grant.revoke").await?;
            Self::lock_in(c, &tenant).await?;
            let request = Request::new(&tenant, person, key, "grant_revoke_machine", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value).map_err(|_| {
                            refused("receipt_damaged", "invalid machine revocation receipt")
                        })?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let now = Self::now_in(c).await?;
            let mut count = 0;
            let mut cursor = String::new();
            loop {
                let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM machine_grants WHERE tenant_id=$1 AND account=$2 AND machine=$3
                    AND machine_binding->>'generation'=$4 AND ended_at IS NULL AND id>$5 ORDER BY id LIMIT 100")
                    .bind(&tenant).bind(account).bind(&machine.name).bind(&machine.generation).bind(&cursor).fetch_all(&mut *c).await?;
                if ids.is_empty() {
                    break;
                }
                for id in ids {
                    if Self::end_grant_in(c, &tenant, &id, "machine_revoked", person, now)
                        .await?
                        .is_some()
                    {
                        count += 1;
                    }
                    cursor = id;
                }
            }
            let result = Revoked {
                machine: machine.name.clone(),
                generation: machine.generation.clone(),
                count,
            };
            Self::audit_in(
                c,
                &tenant,
                person,
                "machine.revoke_all",
                &json!({"machine":machine.name,"count" :count}),
                now,
            )
            .await?;
            reservation.complete_in(c, &result).await?;
            Ok(Changed {
                result,
                replayed: false,
            })
        })
    }
}
