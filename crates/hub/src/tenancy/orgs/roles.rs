//! Reading roles from the store: a stored role, a subject's role in an organisation, and the admin check every change starts with.

use sqlx::AnyConnection;

use super::{Org, Role, checks::refused};
use crate::store::Failed;

pub(super) fn role_of(text: String) -> Result<Role, Failed> {
    Role::parse(&text)
        .ok_or_else(|| refused("board_store", format!("unknown role {text:?} in the store")))
}

pub(super) async fn org_role(
    connection: &mut AnyConnection,
    tenant: &str,
    subject: &str,
) -> Result<Option<Role>, Failed> {
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM org_members WHERE tenant_id = $1 AND subject = $2")
            .bind(tenant)
            .bind(subject)
            .fetch_optional(&mut *connection)
            .await?;
    role.map(role_of).transpose()
}

/// The caller's role when it is an admin or an owner of `org`; refused otherwise.
pub(super) async fn admin_of(
    connection: &mut AnyConnection,
    org: &Org,
    by: &str,
) -> Result<Role, Failed> {
    match org_role(connection, &org.id, by).await? {
        Some(role) if role.may_configure() => Ok(role),
        Some(role) => Err(refused(
            "not_permitted",
            format!(
                "you are {} of {}; an admin or an owner does this",
                role.as_str(),
                org.slug
            ),
        )),
        None => Err(refused(
            "not_found",
            format!("you are not a member of {}", org.slug),
        )),
    }
}
