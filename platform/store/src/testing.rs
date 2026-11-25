//! Stores for a test: a fresh database on the test Postgres that
//! `DEVAGOTCHI_TEST_POSTGRES_URL` names (CI's hub-postgres job sets it), migrated with the
//! migrations the test's crate keeps. Without one, a test that needs a store skips
//! (`or_skip!`) and says so (`SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL`), unless
//! `DEVAGOTCHI_TEST_REQUIRE_POSTGRES=1` (CI's hub-postgres job), where a missing server is a
//! failure: a test there never passes without running.

use super::*;

pub const POSTGRES_ENV: &str = "DEVAGOTCHI_TEST_POSTGRES_URL";
/// `1`: a test that needs the test Postgres fails without one, instead of skipping.
pub const REQUIRE_ENV: &str = "DEVAGOTCHI_TEST_REQUIRE_POSTGRES";
/// What a skipped test prints, so a run that skipped says so.
pub const SKIPPED: &str = "SKIPPED: no DEVAGOTCHI_TEST_POSTGRES_URL";

/// The test Postgres server's URL, or `None` to skip; panics when `REQUIRE_ENV` is `1` and
/// there is none. Every test that needs the server takes it from here.
pub fn server_url() -> Option<String> {
    let url = std::env::var(POSTGRES_ENV).ok().filter(|u| !u.is_empty());
    if url.is_none() && std::env::var(REQUIRE_ENV).is_ok_and(|v| v == "1") {
        panic!("{REQUIRE_ENV}=1, but {POSTGRES_ENV} is not set: this test would have skipped");
    }
    url
}

/// A store under test, with what keeps it alive and how to open it again.
pub struct TestStore {
    pub store: Store,
    pub url: String,
    pub dir: tempfile::TempDir,
}

/// A new database on the test server, brought up to date with `migrations`, or `None`
/// without a server.
pub async fn postgres_with(migrations: &[&Migrator]) -> Option<TestStore> {
    let url = empty_postgres().await?;
    Some(TestStore {
        store: Store::open(&url, migrations).await.unwrap(),
        url,
        dir: tempfile::tempdir().unwrap(),
    })
}

/// The URL of a new database on the test server with nothing in it yet, or `None`
/// without a server.
pub async fn empty_postgres() -> Option<String> {
    let server = server_url()?;
    static DRIVERS: Once = Once::new();
    DRIVERS.call_once(install_default_drivers);
    let name = format!("{PREFIX}{}_{}", now_secs(), hex::encode(random::<8>()));
    let admin = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&server)
        .await
        .expect("the test Postgres server");
    static SWEPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !SWEPT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        sweep(&admin).await;
    }
    admin
        .execute(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .await
        .unwrap();
    admin.close().await;
    let base = server.split('?').next().unwrap();
    let (root, _) = base.rsplit_once('/').unwrap();
    Some(format!("{root}/{name}"))
}

/// A test database's name: this prefix, the second it was made, then random hex.
const PREFIX: &str = "dvg_test_";
/// How old a test database is before the next test run drops it. No test runs this long; a
/// run elsewhere on the same server keeps its own, which are younger.
const STALE_SECS: u64 = 2 * 60 * 60;

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// The test databases older than `STALE_SECS`, which a run left behind (tests do not drop
/// their own: a store's pool outlives the test that made it), dropped once per process. A
/// developer's test server otherwise fills until it runs out of shared memory.
pub(crate) async fn sweep(admin: &sqlx::AnyPool) {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT datname::text FROM pg_database WHERE datname LIKE 'dvg\\_test\\_%'")
            .fetch_all(admin)
            .await
            .unwrap_or_default();
    let cutoff = now_secs().saturating_sub(STALE_SECS);
    for (name,) in names {
        if made_at(&name).is_some_and(|made| made < cutoff) {
            let drop = format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)");
            let _ = admin.execute(sqlx::AssertSqlSafe(drop)).await;
        }
    }
}

/// The second a test database was made, from its name; `None` for any other database. A name
/// from before names carried the second (`dvg_test_` and 16 hex digits) counts as made at 0.
pub(crate) fn made_at(name: &str) -> Option<u64> {
    let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
    let rest = name.strip_prefix(PREFIX)?;
    match rest.split_once('_') {
        Some((secs, random)) if hex(random) => secs.parse().ok(),
        None if rest.len() == 16 && hex(rest) => Some(0),
        _ => None,
    }
}

/// `migrations` run on the database at `url` up to `version` and no further, as a build
/// from before a later migration left it. `Store::open` runs the rest.
pub async fn migrated_to(url: &str, migrations: &Migrator, version: i64) {
    static DRIVERS: Once = Once::new();
    DRIVERS.call_once(install_default_drivers);
    let mut connection = sqlx::AnyConnection::connect(url).await.unwrap();
    migrations.run_to(version, &mut connection).await.unwrap();
    connection.close().await.unwrap();
}

/// Every migration in `migrations` is LF: sqlx checks each applied migration by checksum, so a
/// build from a checkout that turned LF into CRLF (Windows, `core.autocrlf`) refuses a database
/// a Linux build migrated ("migration 1 was previously applied but has been modified").
/// `.gitattributes` keeps them LF; each crate's tests call this on its own.
pub fn assert_line_endings(migrations: &Migrator) {
    for migration in migrations.iter() {
        assert!(
            !migration.sql.as_ref().contains('\r'),
            "migration {} ({}) carries a CR: see .gitattributes",
            migration.version,
            migration.description
        );
    }
}

/// What a test fixture that needs the test Postgres gives, or the end of the test when
/// `DEVAGOTCHI_TEST_POSTGRES_URL` names none: `let w = or_skip!(world());` (it awaits the
/// fixture). CI's `hub-postgres` job runs these with a server.
#[macro_export]
macro_rules! or_skip {
    ($fixture:expr) => {
        match $fixture.await {
            Some(found) => found,
            None => {
                eprintln!("{}", $crate::testing::SKIPPED);
                return;
            }
        }
    };
}
