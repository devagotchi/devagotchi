//! Fixed enum-selected SQL identifiers; all record values are bound parameters.
use super::*;
fn decode(text: String) -> DbResult<Row> {
    let mut value: Value = serde_json::from_str(&text)
        .map_err(|_| refused("catalog_damaged", "invalid catalog JSON"))?;
    value
        .as_object_mut()
        .ok_or_else(|| refused("catalog_damaged", "invalid catalog record"))?
        .remove("tenant_id");
    serde_json::from_value(value)
        .map_err(|_| refused("catalog_damaged", "invalid catalog metadata"))
}
pub(super) async fn get(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    id: &str,
) -> DbResult<Option<Row>> {
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM {} t WHERE tenant_id=$1 AND id=$2",
        kind.table()
    );
    let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .bind(id)
        .fetch_optional(c)
        .await?;
    raw.map(decode).transpose()
}
pub(super) async fn list(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    after: Option<&str>,
    limit: i64,
) -> DbResult<Vec<Row>> {
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM {} t WHERE tenant_id=$1 AND id COLLATE \"C\">$2 ORDER BY id COLLATE \"C\" LIMIT $3",
        kind.table()
    );
    let raw: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .bind(after.unwrap_or(""))
        .bind(limit)
        .fetch_all(c)
        .await?;
    raw.into_iter().map(decode).collect()
}
pub(super) async fn save(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    row: &Row,
    create: bool,
) -> DbResult<()> {
    let mut value = serde_json::to_value(row).map_err(|_| invalid("invalid catalog row"))?;
    value["tenant_id"] = tenant.into();
    let mut columns = vec![
        "tenant_id".to_owned(),
        "id".to_owned(),
        "version".to_owned(),
    ];
    columns.extend(kind.fields().iter().map(|field| field.name.to_owned()));
    if kind == Kind::GrantPresets {
        value["is_default"] = row.is_default.unwrap_or(false).into();
        columns.push("is_default".into());
    }
    let table = kind.table();
    let sql = if create {
        format!(
            "INSERT INTO {table} ({}) SELECT {} FROM jsonb_populate_record(NULL::{table},$1::jsonb) r",
            columns.join(","),
            columns
                .iter()
                .map(|s| format!("r.{s}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        let sets = columns
            .iter()
            .filter(|name| !matches!(name.as_str(), "tenant_id" | "id"))
            .map(|name| format!("{name}=r.{name}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "UPDATE {table} t SET {sets} FROM jsonb_populate_record(NULL::{table},$1::jsonb) r WHERE t.tenant_id=r.tenant_id AND t.id=r.id"
        )
    };
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(value.to_string())
        .execute(c)
        .await?;
    Ok(())
}
pub(super) async fn delete(
    c: &mut AnyConnection,
    tenant: &str,
    kind: Kind,
    id: &str,
) -> DbResult<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {} WHERE tenant_id=$1 AND id=$2",
        kind.table()
    )))
    .bind(tenant)
    .bind(id)
    .execute(c)
    .await?;
    Ok(())
}
