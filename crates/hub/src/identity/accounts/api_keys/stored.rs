//! Stored restrictions must never turn malformed data into an unrestricted key.

use serde::de::DeserializeOwned;
use sqlx::{Row, any::AnyRow};

use super::{MAX_NAMED, Scope};

fn invalid(column: &str) -> sqlx::Error {
    sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        // Do not include stored content in errors or logs.
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid stored API key restrictions",
        )
        .into(),
    }
}

fn decode<T: DeserializeOwned>(row: &AnyRow, column: &str) -> Result<T, sqlx::Error> {
    let raw: String = row.try_get(column)?;
    serde_json::from_str(&raw).map_err(|_| invalid(column))
}

pub(super) fn scopes(row: &AnyRow) -> Result<Vec<Scope>, sqlx::Error> {
    let scopes: Vec<Scope> = decode(row, "scopes")?;
    if scopes.is_empty() {
        return Err(invalid("scopes"));
    }
    Ok(scopes)
}

pub(super) fn names(row: &AnyRow, column: &str) -> Result<Vec<String>, sqlx::Error> {
    let names: Vec<String> = decode(row, column)?;
    if names.len() > MAX_NAMED
        || names
            .iter()
            .any(|name| name.is_empty() || name.len() > 128 || name.chars().any(char::is_control))
    {
        return Err(invalid(column));
    }
    Ok(names)
}
