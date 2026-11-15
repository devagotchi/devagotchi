//! Stores for a test: a fresh database on the test Postgres that
//! `DEVAGOTCHI_TEST_POSTGRES_URL` names (CI's hub-postgres job sets it). Without one, a test
//! that needs a store skips (`or_skip!`, or an empty [`stores`]).

use super::*;

pub const POSTGRES_ENV: &str = "DEVAGOTCHI_TEST_POSTGRES_URL";

/// A store under test, with what keeps it alive and how to open it again.
pub struct TestStore {
    pub store: Store,
    pub url: String,
    pub dir: tempfile::TempDir,
}

/// The stores a test runs on: one new database on the test Postgres, or none without one.
pub async fn stores() -> Vec<TestStore> {
    postgres().await.into_iter().collect()
}

/// The store at `url` brought to migration `version` and no further, as a hub from
/// before a later migration left it. `Store::open` runs the rest.
pub async fn migrated_to(url: &str, version: i64) {
    static DRIVERS: Once = Once::new();
    DRIVERS.call_once(install_default_drivers);
    let mut connection = sqlx::AnyConnection::connect(url).await.unwrap();
    POSTGRES.run_to(version, &mut connection).await.unwrap();
    connection.close().await.unwrap();
}

/// A new, empty database on the test server, or `None` without one.
pub async fn postgres() -> Option<TestStore> {
    let url = empty_postgres().await?;
    Some(TestStore {
        store: Store::open(&url).await.unwrap(),
        url,
        dir: tempfile::tempdir().unwrap(),
    })
}

/// The URL of a new database on the test server with nothing in it yet, or `None`
/// without a server.
pub async fn empty_postgres() -> Option<String> {
    let server = std::env::var(POSTGRES_ENV).ok().filter(|u| !u.is_empty())?;
    static DRIVERS: Once = Once::new();
    DRIVERS.call_once(install_default_drivers);
    let name = format!("dvg_test_{}", hex::encode(random::<8>()));
    let admin = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&server)
        .await
        .expect("the test Postgres server");
    admin
        .execute(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .await
        .unwrap();
    admin.close().await;
    let base = server.split('?').next().unwrap();
    let (root, _) = base.rsplit_once('/').unwrap();
    Some(format!("{root}/{name}"))
}

/// What a test fixture that needs the test Postgres gives, or the end of the test when
/// `DEVAGOTCHI_TEST_POSTGRES_URL` names none: `let w = or_skip!(world());` (it awaits the
/// fixture). CI's `hub-postgres` job runs these with a server.
#[macro_export]
macro_rules! or_skip {
    ($fixture:expr) => {
        match $fixture.await {
            Some(found) => found,
            None => return,
        }
    };
}
