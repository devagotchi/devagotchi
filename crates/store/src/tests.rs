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
}

#[test]
fn a_test_database_name_carries_the_second_it_was_made() {
    assert_eq!(
        testing::made_at("dvg_test_1700000000_00ff"),
        Some(1_700_000_000)
    );
    for other in [
        "dvg_test_00ff",
        "dvg_test_x_00ff",
        "dvg_test_1_zz",
        "devagotchi",
        "postgres",
    ] {
        assert_eq!(testing::made_at(other), None, "{other}");
    }
}

#[tokio::test]
async fn a_test_run_drops_the_test_databases_an_earlier_run_left() {
    let Ok(server) = std::env::var(testing::POSTGRES_ENV) else {
        return;
    };
    DRIVERS_FOR_TESTS.call_once(install_default_drivers);
    let admin = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&server)
        .await
        .unwrap();
    let suffix = hex::encode(random::<4>());
    let stale = format!("dvg_test_1_{suffix}");
    let fresh = format!("dvg_test_{}_{suffix}", testing::now_secs());
    for name in [&stale, &fresh] {
        let create = format!("CREATE DATABASE {name}");
        admin.execute(sqlx::AssertSqlSafe(create)).await.unwrap();
    }
    testing::sweep(&admin).await;
    let left: Vec<(String,)> =
        sqlx::query_as("SELECT datname::text FROM pg_database WHERE datname LIKE '%' || $1")
            .bind(&suffix)
            .fetch_all(&admin)
            .await
            .unwrap();
    let left: Vec<String> = left.into_iter().map(|(name,)| name).collect();
    let drop = format!("DROP DATABASE IF EXISTS {fresh} WITH (FORCE)");
    admin.execute(sqlx::AssertSqlSafe(drop)).await.unwrap();
    assert_eq!(left, vec![fresh.clone()], "only the stale one goes");
}

static DRIVERS_FOR_TESTS: Once = Once::new();
