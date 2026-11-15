use super::*;

#[test]
fn a_stores_place_never_shows_its_user_password_or_query() {
    for (url, place) in [
        (
            "postgres://hub:s3cret@db.internal:5432/devagotchi",
            "db.internal:5432/devagotchi",
        ),
        (
            "postgresql://hub@10.40.0.30/devagotchi?sslmode=require&password=x",
            "10.40.0.30/devagotchi",
        ),
        (
            "postgres:///devagotchi?host=/run/postgresql",
            "local socket/devagotchi",
        ),
        ("postgres://a:b@c@host/db", "host/db"),
    ] {
        assert_eq!(place_of(url), place, "{url}");
        assert!(!place_of(url).contains("s3cret"));
    }
    let file = std::env::temp_dir().join("accounts.sqlite3");
    assert_eq!(Path::new(&place_of(&sqlite_url(&file))), file);
}

/// Each domain's table prefixes (`.ai/patterns/layout.md`, "Tables carry their domain").
const DOMAIN_PREFIXES: [(&str, &[&str]); 8] = [
    ("identity", &["auth_", "oauth_"]),
    ("tenancy", &["tenancy_", "org_", "project_"]),
    ("agenthub", &["board_"]),
    ("hub settings and secrets", &["hub_"]),
    ("machines", &["machine_"]),
    ("notify", &["push_", "mail_", "signal_"]),
    ("ops", &["audit_", "analytics_"]),
    ("connectors", &["connector_"]),
];

/// Tables made before the rule, without a domain prefix. The list only shrinks: renaming one
/// removes it here, and a table not listed must carry a prefix.
const UNPREFIXED: [&str; 5] = [
    "api_keys",
    "connections",
    "passwords",
    "person_preferences",
    "sign_in_settings",
];

#[tokio::test]
async fn every_table_in_the_hub_schema_carries_its_domain_prefix() {
    let Some(test) = testing::postgres().await else {
        return;
    };
    let mut connection = test.store.read().await.unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = 'devagotchi' AND table_name NOT LIKE '\\_sqlx%'
         ORDER BY 1",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    assert!(!tables.is_empty(), "the migrated store has no tables");
    let prefixed = |name: &str| {
        DOMAIN_PREFIXES
            .iter()
            .any(|(_, prefixes)| prefixes.iter().any(|p| name.starts_with(p)))
    };
    let mut broken = Vec::new();
    for table in &tables {
        if !prefixed(table) && !UNPREFIXED.contains(&table.as_str()) {
            broken.push(format!(
                "{table}: no domain prefix; name it after its domain ({})",
                DOMAIN_PREFIXES
                    .iter()
                    .map(|(domain, p)| format!("{domain}: {}", p.join(" ")))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    for listed in UNPREFIXED {
        if !tables.iter().any(|t| t == listed) {
            broken.push(format!(
                "{listed}: listed as unprefixed but gone; remove it from UNPREFIXED"
            ));
        }
    }
    assert!(broken.is_empty(), "table names:\n  {}", broken.join("\n  "));
}
