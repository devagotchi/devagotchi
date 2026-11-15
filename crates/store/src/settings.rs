//! Where the store is, as a person or a deploy names it: its URL (the environment's, else the
//! config's, never with a password in a file), its schema, and the place shown for it.

/// The environment variable that names the store, over the config's `storage.url`.
pub const STORAGE_URL_ENV: &str = "DEVAGOTCHI_HUB_STORAGE_URL";

/// The Postgres a hub uses: the environment's URL, else the config's; `None` when neither
/// names one.
pub fn url_for(configured: Option<&str>) -> Option<String> {
    match std::env::var(STORAGE_URL_ENV) {
        Ok(url) if !url.trim().is_empty() => Some(url.trim().to_string()),
        _ => configured.map(str::to_string),
    }
}

/// Whether a URL is one the store opens, and keeps its password out of a file: `postgres://`
/// or `postgresql://` without a password.
pub fn check_configured_url(url: &str) -> Result<(), String> {
    if let Some(rest) = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
    {
        let authority = rest.split(['/', '?']).next().unwrap_or("");
        if authority
            .rsplit_once('@')
            .is_some_and(|(user, _)| user.contains(':'))
        {
            return Err(format!(
                "storage.url must not carry a password; set {STORAGE_URL_ENV} from the secret store instead"
            ));
        }
        return Ok(());
    }
    Err("storage.url is postgres://… (the hub keeps everything in Postgres)".to_string())
}

/// The environment variable that names the Postgres schema the hub's tables live in, over
/// the config's `storage.schema`.
pub const STORAGE_SCHEMA_ENV: &str = "DEVAGOTCHI_HUB_STORAGE_SCHEMA";

/// A Postgres schema name the hub accepts: 1 to 63 of `[a-z0-9_]`, not starting with a digit
/// or `pg_`, so it needs no quoting and is never one of Postgres's own.
pub fn valid_schema(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && !name.starts_with("pg_")
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The schema a hub's tables live in on Postgres: the environment's, else the config's.
pub fn schema_for(configured: Option<&str>) -> Option<String> {
    match std::env::var(STORAGE_SCHEMA_ENV) {
        Ok(schema) if !schema.trim().is_empty() => Some(schema.trim().to_string()),
        _ => configured.map(str::to_string),
    }
}

/// A store's URL without what must not be shown: the user and password before `@`, and the
/// query (which may carry a password too).
pub fn place_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if host.is_empty() {
        "local socket"
    } else {
        host
    };
    format!("{host}/{path}")
}
