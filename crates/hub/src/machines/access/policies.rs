//! Acknowledged, versioned unsandboxed policy. Disabling ends uncovered grants atomically.
use super::*;
use crate::{
    catalog::Row as CatalogRow,
    machines::devices::{Devices, MachineBinding},
};
use devagotchi_store::receipts::{self, Outcome, Request};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcknowledgedRisk {
    pub id: String,
    pub version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolicyEdit {
    pub version: i64,
    pub allowed: bool,
    pub acknowledged: Vec<AcknowledgedRisk>,
    pub expires_at: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SandboxPolicy {
    pub scope: String,
    pub version: i64,
    pub allowed: bool,
    pub acknowledged: Vec<CatalogRow>,
    pub expires_at: Option<i64>,
    pub ended_grants: u64,
}
impl Access {
    /// Catalog must already hold its environment lock before composing this check.
    pub(crate) async fn require_environment_policy_in(
        c: &mut AnyConnection,
        tenant: &str,
    ) -> DbResult<()> {
        Self::lock_in(c, tenant).await?;
        let now = Self::now_in(c).await?;
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM machine_sandbox_policies WHERE tenant_id=$1 AND scope='environment' AND allowed AND (expires_at IS NULL OR expires_at>$2))")
            .bind(tenant).bind(now).fetch_one(c).await?;
        if !live {
            return Err(refused(
                "policy_denied",
                "enable the environment-wide unsandboxed policy first",
            ));
        }
        Ok(())
    }
    pub(super) async fn policy_live_in(
        c: &mut AnyConnection,
        tenant: &str,
        machine: &MachineBinding,
        now: i64,
    ) -> DbResult<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM machine_sandbox_policies WHERE tenant_id=$1
            AND allowed AND (expires_at IS NULL OR expires_at>$4) AND (scope='environment' OR (machine=$2 AND generation=$3)))")
            .bind(tenant).bind(&machine.name).bind(&machine.generation).bind(now).fetch_one(c).await?)
    }
    /// None enables/disables the environment policy; Some binds one owned enrollment.
    pub(crate) async fn set_policy(
        &self,
        person: &str,
        machine: Option<&MachineBinding>,
        edit: &PolicyEdit,
        key: &str,
    ) -> BridgeResult<Changed<SandboxPolicy>> {
        if edit.version < 0
            || edit.acknowledged.len() > 64
            || edit.acknowledged.iter().any(|r| r.version < 1)
        {
            return Err(invalid("invalid policy version or acknowledgments"));
        }
        let ids: Vec<String> = edit.acknowledged.iter().map(|r| r.id.clone()).collect();
        if ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len() {
            return Err(invalid("acknowledgments must be distinct"));
        }
        let scope = machine.map_or_else(|| "environment".into(), |m| format!("machine:{}", m.name));
        let action = if edit.allowed {
            "policy.unsandboxed.enable"
        } else {
            "policy.unsandboxed.disable"
        };
        let original = json!({"machine":machine,"edit":edit});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.owner_in(c, person, action).await?;
            self.catalog.access_scope_in(c, person).await?;
            if let Some(machine) = machine {
                // Machine ownership comes from the owner's live account in this environment.
                let account = machine.owner.as_deref().ok_or_else(|| {
                    refused(
                        "not_granted",
                        "configured machine policy needs its configured authority resolver",
                    )
                })?;
                let found = self.accounts.catalog_actor_in(c, account, false).await?;
                if found.tenant != tenant {
                    return Err(refused(
                        "not_granted",
                        "machine belongs to another environment",
                    ));
                }
                Devices::new(self.accounts.store.clone())
                    .check_access_in(c, Some(account), machine)
                    .await?;
            }
            Self::lock_in(c, &tenant).await?;
            let request = Request::new(&tenant, person, key, "sandbox_policy", &original)?;
            let reservation = match receipts::begin_in(c, &request).await? {
                Outcome::Replay(value) => {
                    return Ok(Changed {
                        result: serde_json::from_value(value)
                            .map_err(|_| refused("receipt_damaged", "invalid policy receipt"))?,
                        replayed: true,
                    });
                }
                Outcome::Fresh(r) => r,
            };
            let current: Option<i64> = sqlx::query_scalar(
                "SELECT version FROM machine_sandbox_policies WHERE tenant_id=$1 AND scope=$2",
            )
            .bind(&tenant)
            .bind(&scope)
            .fetch_optional(&mut *c)
            .await?;
            if current.unwrap_or(0) != edit.version {
                return Err(refused(
                    "version_conflict",
                    "policy changed; reload before saving",
                ));
            }
            let now = Self::now_in(c).await?;
            if edit.allowed && edit.expires_at.is_some_and(|at| at <= now) {
                return Err(invalid("policy expiry must be in the future").into());
            }
            let settings = Self::settings_in(c, &tenant).await?;
            if edit.allowed
                && !settings
                    .unsandboxed_risk_ids
                    .iter()
                    .all(|id| ids.contains(id))
            {
                return Err(refused(
                    "risks_not_acknowledged",
                    "acknowledge every configured risk before enabling unsandboxed access",
                ));
            }
            let snapshots = if ids.is_empty() {
                vec![]
            } else {
                self.catalog.access_risks_in(c, person, &ids).await?
            };
            if snapshots
                .iter()
                .zip(&edit.acknowledged)
                .any(|(row, ack)| row.version != ack.version)
            {
                return Err(refused(
                    "version_conflict",
                    "risk explanations changed; review them before acknowledging",
                ));
            }
            let version = edit
                .version
                .checked_add(1)
                .ok_or_else(|| invalid("policy version overflow"))?;
            sqlx::query("INSERT INTO machine_sandbox_policies(tenant_id,scope,machine,generation,allowed,acknowledged,enabled_by,enabled_at,expires_at,version)
                VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9,$10) ON CONFLICT(tenant_id,scope) DO UPDATE SET machine=EXCLUDED.machine,
                generation=EXCLUDED.generation,allowed=EXCLUDED.allowed,acknowledged=EXCLUDED.acknowledged,enabled_by=EXCLUDED.enabled_by,
                enabled_at=EXCLUDED.enabled_at,expires_at=EXCLUDED.expires_at,version=EXCLUDED.version")
                .bind(&tenant).bind(&scope).bind(machine.map(|m|m.name.as_str())).bind(machine.map(|m|m.generation.as_str()))
                .bind(edit.allowed).bind(encoded(&snapshots)?).bind(person).bind(now).bind(edit.expires_at).bind(version).execute(&mut *c).await?;
            let ended_grants = Self::end_uncovered_in(c, &tenant, person, now).await?;
            let result = SandboxPolicy {
                scope: scope.clone(),
                version,
                allowed: edit.allowed,
                acknowledged: snapshots,
                expires_at: edit.expires_at,
                ended_grants,
            };
            Self::audit_in(
                c,
                &tenant,
                person,
                if edit.allowed {
                    "policy.unsandboxed.enabled"
                } else {
                    "policy.unsandboxed.disabled"
                },
                &json!({"scope":scope,"version":version,"risks":ids,"ended_grants":ended_grants}),
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
    pub(super) async fn end_uncovered_in(
        c: &mut AnyConnection,
        tenant: &str,
        person: &str,
        now: i64,
    ) -> DbResult<u64> {
        let mut ended = 0;
        let mut cursor = String::new();
        loop {
            // Broad GUI input and policy-dependent presets need live policy too.
            let ids:Vec<String>=sqlx::query_scalar("SELECT g.id FROM machine_grants g JOIN machine_access_requests r
                ON r.tenant_id=g.tenant_id AND r.id=g.request_id WHERE g.tenant_id=$1 AND g.ended_at IS NULL AND g.id>$2
                AND (g.full_access OR g.item->>'sandbox_level'='unsandboxed'
                    OR (g.capability='gui.input' AND (g.item->'selector'->'apps' IS NULL OR g.item->'selector'->>'apps'='any'))
                    OR (r.preset_snapshot->'requires_policy' IS NOT NULL AND r.preset_snapshot->'requires_policy'<>'null'::jsonb))
                AND NOT EXISTS(SELECT 1 FROM machine_sandbox_policies p WHERE p.tenant_id=g.tenant_id AND p.allowed
                    AND (p.expires_at IS NULL OR p.expires_at>$3) AND (p.scope='environment' OR (p.machine=g.machine AND p.generation=g.machine_binding->>'generation')))
                ORDER BY g.id LIMIT 100")
                .bind(tenant).bind(&cursor).bind(now).fetch_all(&mut *c).await?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                if Self::end_grant_in(c, tenant, &id, "policy_disabled", person, now)
                    .await?
                    .is_some()
                {
                    ended += 1;
                }
                cursor = id;
            }
        }
        Ok(ended)
    }
}
