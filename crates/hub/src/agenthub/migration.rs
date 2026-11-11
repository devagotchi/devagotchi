//! Read-only legacy snapshots and transactional imports into the hub's scoped store.
//! Import does not cut over callers; stop writers and retain the original as the backup.

use std::{collections::BTreeMap, path::Path};

use devagotchi_contracts::BridgeResult;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row as _;

use super::{GENESIS, SCHEMA_VERSION, event_from_row, event_hash, fail, store_error};
use crate::store::{Backend, Store, transaction};

const SNAPSHOT_MAX_BYTES: usize = 256 * 1024 * 1024;
const SNAPSHOT_MAX_ROWS: usize = 1_000_000;

pub const USAGE: &str = "devagotchi hub import-board --state-dir ABS_PATH --config ABS_JSON \
--source ABS_SQLITE --tenant t-UUID [--apply]
Preview a verified board snapshot by default; --apply imports it transactionally into
Postgres. Retains the source, refuses a different existing import and does not change
live routing. Stop source writers and back up before importing.";

struct Table {
    source: &'static str,
    target: &'static str,
    columns: &'static str,
    order: &'static str,
}

const TABLES: &[Table] = &[
    Table {
        source: "meta",
        target: "board_metadata",
        columns: "key,value",
        order: "key",
    },
    Table {
        source: "settings",
        target: "board_settings",
        columns: "key,value",
        order: "key",
    },
    Table {
        source: "fence",
        target: "board_fence",
        columns: "id,value",
        order: "id",
    },
    Table {
        source: "tickets",
        target: "board_tickets",
        columns: "id,kind,title,description,state,priority,labels,parent,child_seq,assignee,holder,lease_id,lease_ttl_ms,lease_expires_at,heartbeat_at,fence,version,expiry_count,last_worker,progress,created_by,created_at,updated_at,closed_at,machine,repo,environment,agent,session,fingerprint,occurrences,last_seen_at",
        order: "id",
    },
    Table {
        source: "deps",
        target: "board_deps",
        columns: "ticket,depends_on,kind,created_by,created_at",
        order: "ticket,depends_on,kind",
    },
    Table {
        source: "notes",
        target: "board_notes",
        columns: "seq,ticket,author,kind,body,to_principal,fence,created_at",
        order: "seq",
    },
    Table {
        source: "events",
        target: "board_events",
        columns: "seq,at,actor,action,ticket,data,fence,prev_hash,hash",
        order: "seq",
    },
];

#[derive(Debug, Clone, Serialize)]
enum Cell {
    Integer(Option<i64>),
    Text(Option<String>),
}

#[derive(Debug, Serialize)]
struct SnapshotTable {
    source: &'static str,
    rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub tenant: String,
    pub project: String,
    pub source_hash: String,
    pub chain_head: String,
    pub fence: i64,
    pub counts: BTreeMap<String, i64>,
}

#[derive(Debug)]
pub struct Snapshot {
    project: String,
    source_hash: String,
    chain_head: String,
    fence: i64,
    tables: Vec<SnapshotTable>,
}

fn integer_column(table: &str, column: &str) -> bool {
    (table == "fence" && matches!(column, "id" | "value"))
        || matches!(
            column,
            "priority"
                | "child_seq"
                | "lease_ttl_ms"
                | "lease_expires_at"
                | "heartbeat_at"
                | "fence"
                | "version"
                | "expiry_count"
                | "created_at"
                | "updated_at"
                | "closed_at"
                | "occurrences"
                | "last_seen_at"
                | "seq"
                | "at"
        )
}

impl Snapshot {
    pub fn read(path: &Path) -> BridgeResult<Self> {
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(store_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(store_error)?;
        let tx = connection.transaction().map_err(store_error)?;
        let health: String = tx
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(store_error)?;
        if health != "ok" {
            return Err(fail(
                "board_damaged",
                "source SQLite integrity check failed",
            ));
        }
        let invalid_fk = tx
            .prepare("PRAGMA foreign_key_check")
            .map_err(store_error)?
            .exists([])
            .map_err(store_error)?;
        if invalid_fk {
            return Err(fail("board_damaged", "source contains broken foreign keys"));
        }
        let meta = |key| -> BridgeResult<String> {
            tx.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                .map_err(store_error)
        };
        if meta("schema_version")? != SCHEMA_VERSION.to_string() {
            return Err(fail(
                "board_version",
                "source board must be migrated by its current CLI before import",
            ));
        }
        let project = meta("project")?;
        if !crate::config::valid_name(&project) {
            return Err(fail(
                "invalid",
                "source project must be a valid hub board name",
            ));
        }
        if meta("prefix")?.trim().is_empty() {
            return Err(fail("board_damaged", "source board has no ID prefix"));
        }
        let settings: String = tx
            .query_row(
                "SELECT value FROM settings WHERE key = 'settings'",
                [],
                |r| r.get(0),
            )
            .map_err(store_error)?;
        serde_json::from_str::<super::Settings>(&settings)
            .map_err(|error| fail("board_damaged", format!("source settings: {error}")))?;
        let workflow: String = tx
            .query_row(
                "SELECT value FROM settings WHERE key = 'workflow'",
                [],
                |r| r.get(0),
            )
            .map_err(store_error)?;
        let workflow: super::Workflow = serde_json::from_str(&workflow)
            .map_err(|error| fail("board_damaged", format!("source workflow: {error}")))?;
        workflow
            .validate()
            .map_err(|why| fail("board_damaged", why))?;

        let fence: i64 = tx
            .query_row("SELECT value FROM fence WHERE id = 1", [], |r| r.get(0))
            .map_err(store_error)?;
        let used: i64 = tx
            .query_row("SELECT coalesce(max(fence), 0) FROM tickets", [], |r| {
                r.get(0)
            })
            .map_err(store_error)?;
        if fence < used || fence < 0 {
            return Err(fail(
                "board_damaged",
                "source fence counter is behind its tickets",
            ));
        }
        let mut tables = Vec::new();
        let mut bytes = 0usize;
        let mut count = 0usize;
        for table in TABLES {
            let columns: Vec<_> = table.columns.split(',').collect();
            let sql = format!(
                "SELECT {} FROM {} ORDER BY {}",
                table.columns, table.source, table.order
            );
            let mut statement = tx.prepare(&sql).map_err(store_error)?;
            let mut cursor = statement.query([]).map_err(store_error)?;
            let mut rows = Vec::new();
            while let Some(row) = cursor.next().map_err(store_error)? {
                count += 1;
                let mut cells = Vec::with_capacity(columns.len());
                for (i, column) in columns.iter().enumerate() {
                    let cell = match row.get_ref(i).map_err(store_error)? {
                        ValueRef::Null if integer_column(table.source, column) => {
                            Cell::Integer(None)
                        }
                        ValueRef::Null => Cell::Text(None),
                        ValueRef::Integer(value) if integer_column(table.source, column) => {
                            Cell::Integer(Some(value))
                        }
                        ValueRef::Text(value) if !integer_column(table.source, column) => {
                            bytes += value.len();
                            Cell::Text(Some(
                                std::str::from_utf8(value)
                                    .map_err(|_| fail("board_damaged", "non-UTF8 source text"))?
                                    .to_string(),
                            ))
                        }
                        _ => return Err(fail("board_damaged", "unexpected source column type")),
                    };
                    bytes += 16;
                    if bytes > SNAPSHOT_MAX_BYTES || count > SNAPSHOT_MAX_ROWS {
                        return Err(fail(
                            "board_import_limit",
                            "snapshot exceeds the 256 MiB / 1,000,000 row import limit",
                        ));
                    }
                    cells.push(cell);
                }
                rows.push(cells);
            }
            tables.push(SnapshotTable {
                source: table.source,
                rows,
            });
        }
        let mut chain_head = GENESIS.to_string();
        {
            let mut statement = tx
                .prepare("SELECT seq, at, actor, action, ticket, data, fence, prev_hash, hash FROM events ORDER BY seq")
                .map_err(store_error)?;
            let events = statement
                .query_map([], event_from_row)
                .map_err(store_error)?;
            for (i, event) in events.enumerate() {
                let event = event.map_err(store_error)?;
                if event.seq != i as i64 + 1
                    || event.prev_hash != chain_head
                    || event.hash != event_hash(&chain_head, &event)
                {
                    return Err(fail(
                        "board_damaged",
                        "source event chain verification failed",
                    ));
                }
                chain_head = event.hash;
            }
        }
        tx.commit().map_err(store_error)?;
        let encoded =
            serde_json::to_vec(&tables).map_err(|error| fail("board_import", error.to_string()))?;
        Ok(Self {
            project,
            source_hash: hex::encode(Sha256::digest(encoded)),
            chain_head,
            fence,
            tables,
        })
    }

    pub fn receipt(&self, tenant: &str) -> Receipt {
        Receipt {
            tenant: tenant.to_string(),
            project: self.project.clone(),
            source_hash: self.source_hash.clone(),
            chain_head: self.chain_head.clone(),
            fence: self.fence,
            counts: self
                .tables
                .iter()
                .map(|table| (table.source.to_string(), table.rows.len() as i64))
                .collect(),
        }
    }

    pub async fn import_postgres(&self, store: &Store, tenant: &str) -> BridgeResult<Receipt> {
        if store.backend() != Backend::Postgres {
            return Err(fail(
                "invalid_config",
                "hub board import requires Postgres storage",
            ));
        }
        self.import(store, tenant).await
    }

    async fn import(&self, store: &Store, tenant: &str) -> BridgeResult<Receipt> {
        if !crate::accounts::tenants::valid_tenant_id(tenant) {
            return Err(fail(
                "invalid",
                "import needs an existing tenant ID, not an account ID",
            ));
        }
        let receipt = self.receipt(tenant);
        let encoded = serde_json::to_string(&receipt)
            .map_err(|error| fail("board_import", error.to_string()))?;
        let now = (super::system_clock())();
        transaction!(store, |connection| {
            store.scope(connection, tenant).await?;
            let tenant_exists: i64 =
                sqlx::query_scalar("SELECT count(*) FROM tenancy_tenants WHERE id = $1")
                    .bind(tenant)
                    .fetch_one(&mut *connection)
                    .await?;
            if tenant_exists != 1 {
                return Err(fail("not_found", "target tenant does not exist").into());
            }
            // The unique project row is also the concurrent-import serialization point.
            sqlx::query(
                "INSERT INTO board_projects (tenant_id, id, source_hash, imported_at)
                 VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id, id) DO NOTHING",
            )
            .bind(tenant)
            .bind(&self.project)
            .bind(&self.source_hash)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT receipt FROM board_imports WHERE tenant_id = $1 AND board_id = $2",
            )
            .bind(tenant)
            .bind(&self.project)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(existing) = existing {
                if existing != encoded {
                    return Err(fail("version_conflict",
                        "this project already has a different import; reconcile explicitly, never overwrite").into());
                }
                return Ok(receipt.clone());
            }
            let source_hash: String = sqlx::query_scalar(
                "SELECT source_hash FROM board_projects WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant)
            .bind(&self.project)
            .fetch_one(&mut *connection)
            .await?;
            if source_hash != self.source_hash {
                return Err(
                    fail("version_conflict", "target project has a different source").into(),
                );
            }
            for (table, data) in TABLES.iter().zip(&self.tables) {
                let placeholders = (1..=table.columns.split(',').count() + 2)
                    .map(|i| format!("${i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "INSERT INTO {} (tenant_id,board_id,{}) VALUES ({placeholders})",
                    table.target, table.columns
                );
                for row in &data.rows {
                    // Identifiers come only from TABLES; every source value is bound.
                    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                        .bind(tenant)
                        .bind(&self.project);
                    for cell in row {
                        query = match cell {
                            Cell::Integer(value) => query.bind(*value),
                            Cell::Text(value) => query.bind(value.as_deref()),
                        };
                    }
                    query.execute(&mut *connection).await?;
                }
                let sql = format!(
                    "SELECT count(*) FROM {} WHERE tenant_id = $1 AND board_id = $2",
                    table.target
                );
                let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(tenant)
                    .bind(&self.project)
                    .fetch_one(&mut *connection)
                    .await?;
                if count != data.rows.len() as i64 {
                    return Err(fail("board_import", "target row count differs from source").into());
                }
                // Match SQLite's binary text ordering even on locale-aware Postgres clusters.
                let order = table
                    .order
                    .split(',')
                    .map(|column| {
                        if store.backend() == Backend::Postgres
                            && !integer_column(table.source, column)
                        {
                            format!("{column} COLLATE \"C\"")
                        } else {
                            column.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT {} FROM {} WHERE tenant_id = $1 AND board_id = $2 ORDER BY {}",
                    table.columns, table.target, order
                );
                let imported = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(tenant)
                    .bind(&self.project)
                    .fetch_all(&mut *connection)
                    .await?;
                for (imported, original) in imported.iter().zip(&data.rows) {
                    for (i, cell) in original.iter().enumerate() {
                        let matches = match cell {
                            Cell::Integer(expected) => {
                                imported.try_get::<Option<i64>, _>(i)? == *expected
                            }
                            Cell::Text(expected) => {
                                imported.try_get::<Option<String>, _>(i)? == *expected
                            }
                        };
                        if !matches {
                            return Err(
                                fail("board_import", "target values differ from source").into()
                            );
                        }
                    }
                }
            }
            sqlx::query(
                "INSERT INTO board_imports (tenant_id, board_id, source_hash, receipt)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(tenant)
            .bind(&self.project)
            .bind(&self.source_hash)
            .bind(&encoded)
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO board_outbox (tenant_id, board_id, event_id, kind, payload, created_at)
                 VALUES ($1, $2, $3, 'board.imported', $4, $5)",
            ).bind(tenant).bind(&self.project).bind(format!("import:{}", self.source_hash))
                .bind(&encoded).bind(now).execute(&mut *connection).await?;
            Ok(receipt.clone())
        })
    }
}

#[cfg(test)]
mod tests;
