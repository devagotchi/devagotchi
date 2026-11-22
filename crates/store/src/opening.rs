//! PostgreSQL bootstrap, ordered domain migrations and restricted connection pools.
use super::*;

impl Store {
    /// Open the store at `url` and bring it up to date with `migrations`, in order.
    pub async fn open(url: &str, migrations: &[&Migrator]) -> BridgeResult<Self> {
        Self::open_in(url, None, migrations).await
    }

    /// `open`, with the migration records in their own Postgres `schema`, made if missing: on
    /// a database other things share (a Supabase project, whose `public` schema its REST API
    /// serves to anyone with the project's public key), nothing of the hub's is in `public`.
    /// Every connection sets its `search_path` to it.
    pub async fn open_in(
        url: &str,
        schema: Option<&str>,
        migrations: &[&Migrator],
    ) -> BridgeResult<Self> {
        if let Some(schema) = schema
            && !valid_schema(schema)
        {
            return Err(BridgeError::new(
                "account_store",
                format!(
                    "{schema:?} is not a schema name: 1 to 63 of [a-z0-9_], not starting with a digit or pg_"
                ),
            ));
        }
        static DRIVERS: Once = Once::new();
        DRIVERS.call_once(install_default_drivers);
        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            return Err(BridgeError::new(
                "account_store",
                "the store's URL is postgres://… (the hub keeps everything in Postgres)",
            ));
        }
        let schema = schema.map(str::to_string);
        let home_schema = schema.clone().unwrap_or_else(|| "public".to_string());
        // The migrations run on a connection of their own, as the login role (it may make
        // schemas and roles), with the schema the store began in first on the path: the
        // migration record stays where it always was, and 0007 moves the tables into
        // `devagotchi`.
        {
            let mut admin = sqlx::AnyConnection::connect(url)
                .await
                .map_err(store_error)?;
            if let Some(schema) = &schema {
                admin
                    .execute(sqlx::AssertSqlSafe(format!(
                        "CREATE SCHEMA IF NOT EXISTS {schema}"
                    )))
                    .await
                    .map_err(store_error)?;
            }
            admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "CREATE SCHEMA IF NOT EXISTS {TABLES_SCHEMA}; SET search_path TO {home_schema}, {TABLES_SCHEMA}"
                )))
                .await
                .map_err(store_error)?;
            for migrator in migrations
                .iter()
                .copied()
                .chain(std::iter::once(&receipts::MIGRATIONS))
            {
                migrator
                    .run(&mut admin)
                    .await
                    .map_err(|error| BridgeError::new("account_store", error.to_string()))?;
            }
            let _ = admin.close().await;
        }
        let set_path = format!("SET search_path TO {TABLES_SCHEMA}, {home_schema}");
        let connect = |switch_role: bool| {
            let set_path = set_path.clone();
            AnyPoolOptions::new()
                .max_connections(16)
                .after_connect(move |connection, _| {
                    let set_path = set_path.clone();
                    Box::pin(async move {
                        // Checked names (`valid_schema`): no quoting, nothing to inject.
                        connection.execute(sqlx::AssertSqlSafe(set_path)).await?;
                        // A role is the server's, not the database's: another database may have
                        // made it. Take it only where this one granted it the tables' schema.
                        if switch_role {
                            let may: bool = sqlx::query_scalar(
                                "SELECT CASE WHEN EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub')
                                 THEN pg_has_role(current_user, 'devagotchi_hub', 'MEMBER')
                                  AND has_schema_privilege('devagotchi_hub', 'devagotchi', 'USAGE') ELSE false END",
                            )
                            .fetch_one(&mut *connection)
                            .await?;
                            if may {
                                connection
                                    .execute(sqlx::AssertSqlSafe(format!("SET ROLE {HUB_ROLE}")))
                                    .await?;
                            }
                        }
                        Ok(())
                    })
                })
                .connect(url)
        };
        let pool = connect(true).await.map_err(store_error)?;
        #[cfg(any(test, feature = "testing"))]
        let admin = connect(false).await.map_err(store_error)?;
        let enforced = {
            let mut connection = pool.acquire().await.map_err(store_error)?;
            // Held to row-level security: neither superuser nor BYPASSRLS.
            let held: bool = sqlx::query_scalar(
                "SELECT NOT (rolsuper OR rolbypassrls) FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(store_error)?;
            // The tenant lookup, where there is one (the hub's), must see every tenant
            // to find one: its owner is not held.
            let finds: bool = sqlx::query_scalar(
                "SELECT COALESCE(bool_or(r.rolsuper OR r.rolbypassrls), true) FROM pg_proc p
                 JOIN pg_roles r ON r.oid = p.proowner
                 WHERE p.proname = 'tenant_of' AND p.pronamespace = 'devagotchi'::regnamespace",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(store_error)?;
            if held && !finds {
                return Err(BridgeError::new(
                    "account_store",
                    "the store's tables belong to a role held to row-level security, so the hub cannot find a tenant; migrate it as a role with BYPASSRLS (or superuser) and run the hub as another (crates/hub/README.md, \"The store\")",
                ));
            }
            held
        };
        Ok(Self {
            pool,
            enforced,
            place: place_of(url),
            schema,
            #[cfg(any(test, feature = "testing"))]
            admin,
        })
    }
}
