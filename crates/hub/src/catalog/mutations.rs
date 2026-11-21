use super::*;
use devagotchi_store::receipts::{self, Outcome, Request, Reservation};
use serde::{Serialize, de::DeserializeOwned};

enum Receipt<T> {
    Fresh(Option<Reservation>),
    Replay(T),
}
impl<T: Serialize + DeserializeOwned> Receipt<T> {
    async fn begin(
        c: &mut AnyConnection,
        tenant: &str,
        actor: &str,
        key: Option<&str>,
        operation: &str,
        input: &Value,
    ) -> DbResult<Self> {
        let Some(key) = key else {
            return Ok(Self::Fresh(None));
        };
        let request = Request::new(tenant, actor, key, operation, input)?;
        match receipts::begin_in(c, &request).await? {
            Outcome::Fresh(reservation) => Ok(Self::Fresh(Some(reservation))),
            Outcome::Replay(value) => Ok(Self::Replay(
                serde_json::from_value(value)
                    .map_err(|_| refused("receipt_damaged", "invalid catalog receipt"))?,
            )),
        }
    }
    async fn complete(self, c: &mut AnyConnection, result: T) -> DbResult<Changed<T>> {
        if let Self::Fresh(Some(reservation)) = self {
            reservation.complete_in(c, &result).await?;
        }
        Ok(Changed {
            result,
            replayed: false,
        })
    }
}
async fn audit(
    c: &mut AnyConnection,
    tenant: &str,
    actor: &str,
    kind: Kind,
    id: &str,
    action: &str,
) -> DbResult<()> {
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp()))::bigint")
            .fetch_one(&mut *c)
            .await?;
    crate::identity::accounts::tenants::record_in(
        c,
        tenant,
        actor,
        &format!("catalog.{}.{}", kind.name(), action),
        &json!({"id":id}).to_string(),
        now.max(0) as u64,
    )
    .await
}
impl Catalog {
    pub async fn put(
        &self,
        actor: &str,
        kind: Kind,
        id: &str,
        version: i64,
        patch: &Value,
        key: Option<&str>,
    ) -> BridgeResult<Changed<Row>> {
        if !valid_id(id) || version < 0 || !patch.is_object() {
            return Err(invalid("invalid catalog id, version or fields"));
        }
        schema::bounded(patch)?;
        if patch.as_object().unwrap().is_empty()
            || patch
                .as_object()
                .unwrap()
                .keys()
                .any(|key| !kind.fields().iter().any(|field| field.name == key))
        {
            return Err(invalid("supply known editable catalog fields"));
        }
        let input = json!({"kind":kind.name(),"id":id,"version":version,"fields":patch});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.enter(c, actor, kind, Some("put")).await?;
            let receipt = Receipt::begin(c, &tenant, actor, key, "catalog_put", &input).await?;
            if let Receipt::Replay(result) = receipt {
                return Ok(Changed {
                    result,
                    replayed: true,
                });
            }
            let current = stored::get(c, &tenant, kind, id).await?;
            match &current {
                Some(row) if version != row.version => {
                    return Err(refused(
                        "version_conflict",
                        "catalog record changed; reload before saving",
                    ));
                }
                None if version != 0 => {
                    return Err(refused("not_found", "catalog record does not exist"));
                }
                _ => {}
            }
            if current.is_none() && stored::list(c, &tenant, kind, None, 1001).await?.len() >= 1000
            {
                return Err(refused("in_use", "catalog kind is limited to 1000 records"));
            }
            let mut merged = current
                .as_ref()
                .map(|row| row.data.clone())
                .unwrap_or_default();
            merged.extend(patch.as_object().unwrap().clone());
            let value = validate::record(c, &tenant, kind, Value::Object(merged)).await?;
            if current
                .as_ref()
                .is_some_and(|row| row.is_default == Some(true))
            {
                references::default_selection(&value)?;
            }
            if kind == Kind::Capabilities && current.is_some() {
                references::capability_change(c, &tenant, id, &value).await?;
            }
            let next = version
                .checked_add(1)
                .ok_or_else(|| invalid("catalog version exhausted"))?;
            let row = Row {
                id: id.into(),
                version: next,
                is_default: current
                    .as_ref()
                    .and_then(|row| row.is_default)
                    .or_else(|| (kind == Kind::GrantPresets).then_some(false)),
                data: value.as_object().unwrap().clone(),
            };
            stored::save(c, &tenant, kind, &row, current.is_none()).await?;
            audit(c, &tenant, actor, kind, id, "saved").await?;
            receipt.complete(c, row).await
        })
    }
    pub async fn remove(
        &self,
        actor: &str,
        kind: Kind,
        id: &str,
        version: i64,
        key: Option<&str>,
    ) -> BridgeResult<Changed<Value>> {
        if !valid_id(id) || version <= 0 {
            return Err(invalid("removal needs a catalog id and positive version"));
        }
        let input = json!({"kind":kind.name(),"id":id,"version":version});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.enter(c, actor, kind, Some("delete")).await?;
            let receipt = Receipt::begin(c, &tenant, actor, key, "catalog_delete", &input).await?;
            if let Receipt::Replay(result) = receipt {
                return Ok(Changed {
                    result,
                    replayed: true,
                });
            }
            let row = stored::get(c, &tenant, kind, id)
                .await?
                .ok_or_else(|| refused("not_found", "catalog record does not exist"))?;
            if row.version != version {
                return Err(refused(
                    "version_conflict",
                    "catalog record changed; reload before removing",
                ));
            }
            references::deletion(c, &tenant, kind, &row).await?;
            stored::delete(c, &tenant, kind, id).await?;
            audit(c, &tenant, actor, kind, id, "deleted").await?;
            receipt.complete(c, json!({"deleted":id})).await
        })
    }
    pub async fn choose_default(
        &self,
        actor: &str,
        id: &str,
        version: i64,
        key: Option<&str>,
    ) -> BridgeResult<Changed<Row>> {
        if !valid_id(id) || version <= 0 {
            return Err(invalid(
                "default selection needs an id and positive version",
            ));
        }
        let kind = Kind::GrantPresets;
        let input = json!({"id":id,"version":version});
        transaction!(&self.accounts.store, |c| {
            let tenant = self.enter(c, actor, kind, Some("default")).await?;
            let receipt = Receipt::begin(c, &tenant, actor, key, "catalog_default", &input).await?;
            if let Receipt::Replay(result) = receipt {
                return Ok(Changed {
                    result,
                    replayed: true,
                });
            }
            let mut row = stored::get(c, &tenant, kind, id)
                .await?
                .ok_or_else(|| refused("not_found", "grant preset does not exist"))?;
            if row.version != version {
                return Err(refused(
                    "version_conflict",
                    "grant preset changed; reload before choosing",
                ));
            }
            if row.is_default != Some(true) {
                references::default_selection(&Value::Object(row.data.clone()))?;
                sqlx::query("UPDATE catalog_grant_presets SET is_default=false,version=version+1 WHERE tenant_id=$1 AND is_default").bind(&tenant).execute(&mut *c).await?;
                row.is_default = Some(true);
                row.version = version
                    .checked_add(1)
                    .ok_or_else(|| invalid("catalog version exhausted"))?;
                stored::save(c, &tenant, kind, &row, false).await?;
                audit(c, &tenant, actor, kind, id, "default_selected").await?;
            }
            receipt.complete(c, row).await
        })
    }
}
