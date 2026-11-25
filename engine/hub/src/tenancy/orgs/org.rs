//! An organisation itself: making one, finding it by slug, id or member, a subject's role in it, and the home tenant.

use devagotchi_contracts::BridgeResult;
use sqlx::Row;

use super::{
    Org, Orgs, Role,
    checks::{check_name, check_subject, check_title, refused},
    roles::org_role,
};
use crate::{
    config::valid_name,
    identity::accounts::tenants::{new_tenant_id, record_in},
    store::transaction,
};

impl Orgs {
    /// Make an organisation named `slug`, with `by` as its owner.
    pub async fn create(&self, by: &str, slug: &str, name: &str) -> BridgeResult<Org> {
        check_subject(by)?;
        check_name("organisation", slug)?;
        let name = check_title(name)?;
        let id = new_tenant_id(crate::machines::random::<16>());
        let now = self.now();
        transaction!(&self.store, |connection| {
            let taken = sqlx::query("SELECT 1 FROM tenancy_tenants WHERE slug = $1")
                .bind(slug)
                .fetch_optional(&mut *connection)
                .await?
                .is_some();
            if taken {
                return Err(refused(
                    "name_taken",
                    format!("{slug:?} is taken on this hub; pick another name"),
                ));
            }
            sqlx::query(
                "INSERT INTO tenancy_tenants (id, home, slug, name, kind, created_at)
                 VALUES ($1, 0, $2, $3, 'organisation', $4)",
            )
            .bind(&id)
            .bind(slug)
            .bind(&name)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            self.store.scope(connection, &id).await?;
            sqlx::query(
                "INSERT INTO tenancy_access (tenant_id, state, admitted_at, admitted_by)
                 VALUES ($1, 'admitted', $2, $3)",
            )
            .bind(&id)
            .bind(now)
            .bind(format!("organisation made by {by}"))
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO org_members (tenant_id, subject, role, added_by, added_at)
                 VALUES ($1, $2, 'owner', $2, $3)",
            )
            .bind(&id)
            .bind(by)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            record_in(
                connection,
                &id,
                by,
                "org.created",
                &format!("{slug} ({name}), owner {by}"),
                now as u64,
            )
            .await?;
            Ok(Org {
                id: id.clone(),
                slug: slug.to_string(),
                name: name.clone(),
                created_at: now,
            })
        })
    }

    /// The organisation named `slug`, if there is one.
    pub async fn by_slug(&self, slug: &str) -> BridgeResult<Option<Org>> {
        if !valid_name(slug) {
            return Ok(None);
        }
        let mut connection = self.store.read().await?;
        let row = sqlx::query(
            "SELECT id, slug, name, created_at FROM tenancy_tenants
             WHERE slug = $1 AND kind = 'organisation'",
        )
        .bind(slug)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::store::store_error)?;
        row.map(|row| org_from(&row))
            .transpose()
            .map_err(crate::store::store_error)
    }

    /// The organisation whose tenant is `id`, if it is one.
    pub async fn by_id(&self, id: &str) -> BridgeResult<Option<Org>> {
        let mut connection = self.store.read().await?;
        let row = sqlx::query(
            "SELECT id, slug, name, created_at FROM tenancy_tenants
             WHERE id = $1 AND kind = 'organisation'",
        )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::store::store_error)?;
        row.map(|row| org_from(&row))
            .transpose()
            .map_err(crate::store::store_error)
    }

    /// The organisations `subject` belongs to, with its role in each, by slug.
    pub async fn of_subject(&self, subject: &str) -> BridgeResult<Vec<(Org, Role)>> {
        let mut connection = self.store.read().await?;
        let sql = "SELECT tenant_id, role FROM devagotchi.orgs_of($1)";
        let rows: Vec<(String, String)> = sqlx::query_as(sql)
            .bind(subject)
            .fetch_all(&mut *connection)
            .await
            .map_err(crate::store::store_error)?;
        drop(connection);
        let mut out = Vec::with_capacity(rows.len());
        for (tenant, role) in rows {
            let Some(role) = Role::parse(&role) else {
                continue;
            };
            if let Some(org) = self.by_id(&tenant).await? {
                out.push((org, role));
            }
        }
        out.sort_by(|a, b| a.0.slug.cmp(&b.0.slug));
        Ok(out)
    }

    /// `subject`'s role in `org`, if it is a member.
    pub async fn role_in(&self, org: &Org, subject: &str) -> BridgeResult<Option<Role>> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, &org.id).await?;
            org_role(connection, &org.id, subject).await
        })
    }

    /// The home tenant's id: where configured boards (`@shared`) keep their members.
    pub async fn home_tenant(&self) -> BridgeResult<Option<String>> {
        let mut connection = self.store.read().await?;
        sqlx::query_scalar("SELECT id FROM tenancy_tenants WHERE home = 1")
            .fetch_optional(&mut *connection)
            .await
            .map_err(crate::store::store_error)
    }
}

pub(super) fn org_from(row: &sqlx::any::AnyRow) -> Result<Org, sqlx::Error> {
    Ok(Org {
        id: row.try_get("id")?,
        slug: row
            .try_get::<Option<String>, _>("slug")?
            .unwrap_or_default(),
        name: row
            .try_get::<Option<String>, _>("name")?
            .unwrap_or_default(),
        created_at: row.try_get("created_at")?,
    })
}
