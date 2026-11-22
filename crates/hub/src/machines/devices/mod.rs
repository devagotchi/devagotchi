//! Machines enrolled to reach this hub over the link (plan N20), kept in the hub's Postgres
//! (migration 0026: `machine_devices`, `machine_enroll_codes`; phones in `push_subscriptions`,
//! `crate::notify::push::subscriptions`). A device is a name bound to one Ed25519 public key. Binding happens once, with a one-time code; the code itself is
//! never stored, only its SHA-256. The hub operator issues codes for the brokers in the hub
//! config; a signed-in account issues them for machines of its own, which only that account
//! reaches (owned machines, increment 29a). People's phones and browsers that receive
//! notifications (Web Push) are kept here too.
//!
//! Hub-wide rows, not under a tenant's row-level security: the link proves a machine by its
//! name before any tenant is known, and a configured machine belongs to no account. `owner`
//! decides who reaches a machine.

use std::time::{SystemTime, UNIX_EPOCH};

use devagotchi_contracts::{BridgeError, BridgeResult};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::store::{Store, secs, transaction, unsecs};
mod access;
mod control;
pub(crate) use access::MachineBinding;
pub(crate) use control::MachineControl;

/// How long an enrollment code stays valid.
pub const CODE_TTL_SECS: u64 = 15 * 60;

/// How many machines an account may hold on a hub it does not own: joined and not
/// revoked, plus names it holds an unused code for.
pub const OWNED_LIMIT: usize = 16;

pub struct Devices {
    /// Read by the push subscriptions' methods (`crate::notify::push::subscriptions`).
    pub(crate) store: Store,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Device {
    pub name: String,
    pub public_key: String,
    pub enrolled_at: u64,
    pub revoked_at: Option<u64>,
    /// The account that added this machine with a code of its own; `None` for a broker
    /// the hub operator configured.
    pub owner: Option<String>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn code_hash(code: &str) -> String {
    hex::encode(Sha256::digest(code.trim().as_bytes()))
}

fn name_taken(device: &str) -> BridgeError {
    BridgeError::new(
        "name_taken",
        format!("{device:?} is already in use on this hub; choose another name"),
    )
}

const DEVICE_COLUMNS: &str = "name, public_key, enrolled_at, revoked_at, owner";

fn device_row(row: &sqlx::any::AnyRow) -> Result<Device, sqlx::Error> {
    Ok(Device {
        name: row.try_get("name")?,
        public_key: row.try_get("public_key")?,
        enrolled_at: unsecs(row.try_get("enrolled_at")?),
        revoked_at: row.try_get::<Option<i64>, _>("revoked_at")?.map(unsecs),
        owner: row.try_get("owner")?,
    })
}

/// The machines an owner holds: enrolled and not revoked, or with a live code (`$3` is a
/// machine not to count, the one being added).
const HELD: &str =
    "SELECT (SELECT COUNT(*) FROM machine_devices WHERE owner = $1 AND revoked_at IS NULL)
      + (SELECT COUNT(DISTINCT device) FROM machine_enroll_codes
           WHERE owner = $1 AND used_at IS NULL AND expires_at >= $2
             AND device != $3
             AND device NOT IN (SELECT name FROM machine_devices WHERE owner = $1))";

/// Mark every unused code for `device` as spent.
async fn cancel_codes(
    connection: &mut sqlx::AnyConnection,
    device: &str,
    now: u64,
) -> Result<usize, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE machine_enroll_codes SET used_at = $1 WHERE device = $2 AND used_at IS NULL",
    )
    .bind(secs(now))
    .bind(device)
    .execute(&mut *connection)
    .await?;
    Ok(usize::try_from(done.rows_affected()).unwrap_or(0))
}

async fn held(
    connection: &mut sqlx::AnyConnection,
    owner: &str,
    now: u64,
    except: &str,
) -> Result<usize, sqlx::Error> {
    let held: i64 = sqlx::query_scalar(HELD)
        .bind(owner)
        .bind(secs(now))
        .bind(except)
        .fetch_one(&mut *connection)
        .await?;
    Ok(usize::try_from(held).unwrap_or(0))
}

impl Devices {
    /// The registry in the hub's store.
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Issue a one-time code that lets one machine bind its key to `device`. A new code
    /// supersedes every unused one for the same device, so at most one is ever live.
    pub async fn issue_code(&self, device: &str, random: [u8; 16]) -> BridgeResult<String> {
        let code = format!("dvg-{}", hex::encode(random));
        let now = now_secs();
        transaction!(&self.store, |connection| {
            cancel_codes(connection, device, now).await?;
            sqlx::query(
                "INSERT INTO machine_enroll_codes (code_hash, device, expires_at) VALUES ($1, $2, $3)",
            )
            .bind(code_hash(&code))
            .bind(device)
            .bind(secs(now + CODE_TTL_SECS))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })?;
        Ok(code)
    }

    /// Issue a one-time code that lets one machine join as `device`, owned by `owner`:
    /// once it binds, `owner` reaches it and nobody else does. The name must be free on
    /// this hub, or already this owner's (a new code re-adds the machine with a new key).
    /// The caller refuses names the hub config uses. A new code supersedes the owner's
    /// unused one for the name; an unused, unexpired code another account holds keeps the
    /// name for that account until it is spent or expires.
    pub async fn issue_owned_code(
        &self,
        owner: &str,
        device: &str,
        random: [u8; 16],
        limit: usize,
    ) -> BridgeResult<String> {
        let code = format!("dvg-{}", hex::encode(random));
        let now = now_secs();
        transaction!(&self.store, |connection| {
            let holder =
                sqlx::query("SELECT owner, revoked_at FROM machine_devices WHERE name = $1")
                    .bind(device)
                    .fetch_optional(&mut *connection)
                    .await?
                    .map(|row| {
                        Ok::<_, sqlx::Error>((
                            row.try_get::<Option<String>, _>("owner")?,
                            row.try_get::<Option<i64>, _>("revoked_at")?,
                        ))
                    })
                    .transpose()?;
            if holder
                .as_ref()
                .is_some_and(|(holder, _)| holder.as_deref() != Some(owner))
            {
                return Err(name_taken(device).into());
            }
            // A machine it holds and has not revoked takes no new place; a revoked one does.
            let new_place = !matches!(holder, Some((_, None)));
            let pending_elsewhere = sqlx::query(
                "SELECT 1 FROM machine_enroll_codes WHERE device = $1 AND used_at IS NULL
                   AND expires_at >= $2 AND (owner IS NULL OR owner != $3)",
            )
            .bind(device)
            .bind(secs(now))
            .bind(owner)
            .fetch_optional(&mut *connection)
            .await?
            .is_some();
            if pending_elsewhere {
                return Err(name_taken(device).into());
            }
            if new_place {
                let held = held(connection, owner, now, device).await?;
                if held >= limit {
                    return Err(BridgeError::new(
                        "machine_limit",
                        format!(
                            "this account already has {held} machines on this hub (the most is {limit}); revoke one first"
                        ),
                    )
                    .into());
                }
            }
            cancel_codes(connection, device, now).await?;
            sqlx::query(
                "INSERT INTO machine_enroll_codes (code_hash, device, expires_at, owner) VALUES ($1, $2, $3, $4)",
            )
            .bind(code_hash(&code))
            .bind(device)
            .bind(secs(now + CODE_TTL_SECS))
            .bind(owner)
            .execute(&mut *connection)
            .await?;
            Ok(())
        })?;
        Ok(code)
    }

    /// Spend `code` for `device` and bind `public_key` to it. Re-enrolling a name with a
    /// fresh code replaces its key and clears a revocation: a new code is the operator's
    /// or the owner's act. A machine the hub config does not name (`configured` false)
    /// joins only with a code an account issued; it then belongs to that account.
    pub async fn enroll(
        &self,
        device: &str,
        code: &str,
        public_key: &str,
        configured: bool,
    ) -> BridgeResult<Device> {
        let refused = |message: &str| BridgeError::new("enroll_refused", message);
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|error| BridgeError::new("random", error.to_string()))?;
        let generation = format!("g-{}", hex::encode(random));
        let now = now_secs();
        let owner = transaction!(&self.store, |connection| {
            let row = sqlx::query(
                "SELECT device, expires_at, used_at, owner FROM machine_enroll_codes WHERE code_hash = $1",
            )
            .bind(code_hash(code))
            .fetch_optional(&mut *connection)
            .await?;
            let owner = match row {
                None => return Err(refused("unknown enrollment code").into()),
                Some(row) => {
                    let bound: String = row.try_get("device")?;
                    let expires = unsecs(row.try_get("expires_at")?);
                    let used: Option<i64> = row.try_get("used_at")?;
                    let owner: Option<String> = row.try_get("owner")?;
                    if bound != device {
                        return Err(refused("this code was issued for another device").into());
                    }
                    if used.is_some() {
                        return Err(refused("this code was already used").into());
                    }
                    if expires < now {
                        return Err(refused("this code has expired").into());
                    }
                    owner
                }
            };
            if owner.is_none() && !configured {
                return Err(refused("this hub's configuration does not name this device").into());
            }
            // Spend this code and any other unused one for the device: a binding ends them all.
            cancel_codes(connection, device, now).await?;
            sqlx::query(
                "INSERT INTO machine_devices (name, public_key, enrolled_at, revoked_at, owner, generation)
                 VALUES ($1, $2, $3, NULL, $4, $5)
                 ON CONFLICT (name) DO UPDATE SET public_key = excluded.public_key,
                   enrolled_at = excluded.enrolled_at, revoked_at = NULL, owner = excluded.owner,
                   generation = excluded.generation, access_paused=false, gui_paused=false, access_version=1",
            )
            .bind(device)
            .bind(public_key)
            .bind(secs(now))
            .bind(owner.clone())
            .bind(&generation)
            .execute(&mut *connection)
            .await?;
            Ok(owner)
        })?;
        Ok(Device {
            name: device.to_string(),
            public_key: public_key.to_string(),
            enrolled_at: now,
            revoked_at: None,
            owner,
        })
    }

    /// How many machines `owner` holds: enrolled and not revoked, or with a live code.
    pub async fn held(&self, owner: &str) -> BridgeResult<usize> {
        transaction!(&self.store, |connection| {
            Ok(held(connection, owner, now_secs(), "").await?)
        })
    }

    /// The account a live code (unused, unexpired) was issued to, if an account issued it.
    pub async fn code_owner(&self, code: &str) -> BridgeResult<Option<String>> {
        transaction!(&self.store, |connection| {
            let owner: Option<Option<String>> = sqlx::query_scalar(
                "SELECT owner FROM machine_enroll_codes
                 WHERE code_hash = $1 AND used_at IS NULL AND expires_at >= $2",
            )
            .bind(code_hash(code))
            .bind(secs(now_secs()))
            .fetch_optional(&mut *connection)
            .await?;
            Ok(owner.flatten())
        })
    }

    /// The device a live code (unused, unexpired) is for, without spending it: the join
    /// script is written for that name.
    pub async fn live_code(&self, code: &str) -> BridgeResult<Option<String>> {
        transaction!(&self.store, |connection| {
            Ok(sqlx::query_scalar(
                "SELECT device FROM machine_enroll_codes
                 WHERE code_hash = $1 AND used_at IS NULL AND expires_at >= $2",
            )
            .bind(code_hash(code))
            .bind(secs(now_secs()))
            .fetch_optional(&mut *connection)
            .await?)
        })
    }

    /// The machines `owner` added, by name, revoked ones included.
    pub async fn owned_by(&self, owner: &str) -> BridgeResult<Vec<Device>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {DEVICE_COLUMNS} FROM machine_devices WHERE owner = $1 ORDER BY name"
            )))
            .bind(owner)
            .fetch_all(&mut *connection)
            .await?;
            Ok(rows.iter().map(device_row).collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Revoke one of `owner`'s machines and cancel its unused codes (also a code still
    /// pending for a name that never joined). Another account's machine, or a configured
    /// one, answers `not_found`, so names of other accounts' machines are not revealed.
    pub async fn revoke_owned(&self, owner: &str, device: &str) -> BridgeResult<(bool, usize)> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            let holder: Option<Option<String>> =
                sqlx::query_scalar("SELECT owner FROM machine_devices WHERE name = $1")
                    .bind(device)
                    .fetch_optional(&mut *connection)
                    .await?;
            let pending_mine = sqlx::query(
                "SELECT 1 FROM machine_enroll_codes WHERE device = $1 AND used_at IS NULL AND owner = $2",
            )
            .bind(device)
            .bind(owner)
            .fetch_optional(&mut *connection)
            .await?
            .is_some();
            let mine = match holder {
                Some(holder) => holder.as_deref() == Some(owner),
                None => pending_mine,
            };
            if !mine {
                return Err(BridgeError::new(
                    "not_found",
                    format!("you have no machine named {device:?}"),
                )
                .into());
            }
            Ok(revoke_in(connection, device, now).await?)
        })
    }

    pub async fn get(&self, device: &str) -> BridgeResult<Option<Device>> {
        transaction!(&self.store, |connection| {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {DEVICE_COLUMNS} FROM machine_devices WHERE name = $1"
            )))
            .bind(device)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(row.as_ref().map(device_row).transpose()?)
        })
    }

    pub async fn is_revoked(&self, device: &str) -> BridgeResult<bool> {
        Ok(self
            .get(device)
            .await?
            .is_none_or(|d| d.revoked_at.is_some()))
    }

    /// Revoke a device and cancel every unused code for it. Its open link closes within
    /// seconds and nothing can enroll under the name until the operator issues a new code.
    /// Returns whether an active device was revoked and how many codes were cancelled.
    pub async fn revoke(&self, device: &str) -> BridgeResult<(bool, usize)> {
        let now = now_secs();
        transaction!(&self.store, |connection| {
            Ok(revoke_in(connection, device, now).await?)
        })
    }

    pub async fn list(&self) -> BridgeResult<Vec<Device>> {
        transaction!(&self.store, |connection| {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {DEVICE_COLUMNS} FROM machine_devices ORDER BY name"
            )))
            .fetch_all(&mut *connection)
            .await?;
            Ok(rows.iter().map(device_row).collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// Rewrite machines, unused codes, and push subscriptions that still name `from`
    /// (an identity-provider subject) so they name the Devagotchi account `to`.
    #[cfg(test)]
    pub async fn reassign_owner(&self, from: &str, to: &str) -> BridgeResult<usize> {
        if from.is_empty() || to.is_empty() || from == to {
            return Ok(0);
        }
        transaction!(&self.store, |connection| {
            let mut moved = 0u64;
            for sql in [
                "UPDATE machine_devices SET owner = $1 WHERE owner = $2",
                "UPDATE machine_enroll_codes SET owner = $1 WHERE owner = $2",
                "UPDATE push_subscriptions SET subject = $1 WHERE subject = $2",
            ] {
                moved += sqlx::query(sql)
                    .bind(to)
                    .bind(from)
                    .execute(&mut *connection)
                    .await?
                    .rows_affected();
            }
            Ok(usize::try_from(moved).unwrap_or(0))
        })
    }

    #[cfg(test)]
    async fn expire_codes(&self) {
        transaction!(&self.store, |connection| {
            sqlx::query("UPDATE machine_enroll_codes SET expires_at = 0")
                .execute(&mut *connection)
                .await?;
            Ok(())
        })
        .unwrap();
    }
}

/// Cancel `device`'s unused codes and revoke it if it is active.
async fn revoke_in(
    connection: &mut sqlx::AnyConnection,
    device: &str,
    now: u64,
) -> Result<(bool, usize), sqlx::Error> {
    let cancelled = cancel_codes(connection, device, now).await?;
    let revoked = sqlx::query(
        "UPDATE machine_devices SET revoked_at = $1 WHERE name = $2 AND revoked_at IS NULL",
    )
    .bind(secs(now))
    .bind(device)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok((revoked == 1, cancelled))
}

/// A registry on a new database of the test server, or `None` without one
/// (`DEVAGOTCHI_TEST_POSTGRES_URL`; CI's hub-postgres job sets it).
#[cfg(test)]
pub(crate) async fn test_registry() -> Option<std::sync::Arc<Devices>> {
    let test = crate::store::testing::postgres().await?;
    Some(std::sync::Arc::new(Devices::new(test.store)))
}

#[cfg(test)]
mod tests;
