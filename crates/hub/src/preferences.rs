//! What a person told the hub they want (increment 119): what to call them, how the app
//! looks, how long "approve and allow" lasts, whether kinds are offered, which kinds always
//! wait for them, and whether their machines may buzz their phone.
//!
//! One row per person under their tenant's row-level security (migration 0020), the
//! preferences as versioned JSON. `SCHEMA` is the shape this hub writes. Reading goes through
//! `upgrade`, so a change of shape is one step there and old rows need no migration; fields
//! this hub does not know stay in `extra` and are written back, so an older hub never loses
//! what a newer one stored. Every change lands on the tenant's hash chain as
//! `preferences.changed`, naming what changed (a name by that fact alone), which is how
//! signals sinks see people's choices evolve.

use std::sync::Arc;

use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::Row;

use crate::{
    accounts::tenants::record_in,
    board::approval::MAX_STANDING_DAYS,
    config::valid_name,
    store::{Backend, Store, store_error, transaction},
};

#[cfg(test)]
mod tests;

/// The shape this hub writes.
pub const SCHEMA: u32 = 1;
/// How long "approve and allow" may be set to, in days.
pub const ALLOW_CHOICES: [u32; 3] = [7, 30, MAX_STANDING_DAYS];
/// The kinds one person may name as always waiting for them.
const MAX_ALWAYS_ASK: usize = 30;
/// The longest name a person may give.
const MAX_NAME: usize = 40;
pub const EVENT: &str = "preferences.changed";

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// How the app looks: the phone's own setting, or always one of the two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "system" => Some(Theme::System),
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
}

/// One person's preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema: u32,
    /// What to call them; never used to decide anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub theme: Theme,
    /// How many days "approve and allow" lasts.
    pub allow_days: u32,
    /// Offer the kinds they keep approving.
    pub offer_ahead: bool,
    /// Kinds that always wait for them: not offered, and they cannot allow them ahead of time.
    pub always_ask: Vec<String>,
    /// Their machines may push to their phone (offline, update failed).
    pub machine_pushes: bool,
    /// What a newer hub stored that this one does not know.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            name: None,
            theme: Theme::System,
            allow_days: 30,
            offer_ahead: true,
            always_ask: Vec::new(),
            machine_pushes: true,
            extra: Map::new(),
        }
    }
}

impl Preferences {
    /// Refused with why, when a person's choices are not ones this hub takes.
    pub fn validate(&self) -> BridgeResult<()> {
        if let Some(name) = &self.name
            && (name.trim().is_empty()
                || name.chars().count() > MAX_NAME
                || name.chars().any(char::is_control))
        {
            return Err(invalid(format!(
                "a name is 1 to {MAX_NAME} characters on one line"
            )));
        }
        if !ALLOW_CHOICES.contains(&self.allow_days) {
            return Err(invalid(format!(
                "allow_days must be one of {ALLOW_CHOICES:?}"
            )));
        }
        if self.always_ask.len() > MAX_ALWAYS_ASK {
            return Err(invalid(format!("at most {MAX_ALWAYS_ASK} kinds")));
        }
        if let Some(bad) = self.always_ask.iter().find(|kind| !valid_name(kind)) {
            return Err(invalid(format!(
                "{bad:?} is not a kind: lowercase letters, digits and dashes"
            )));
        }
        Ok(())
    }

    /// Whether this kind always waits for this person.
    pub fn always_asks(&self, kind: &str) -> bool {
        self.always_ask.iter().any(|k| k == kind)
    }
}

/// Kinds as a person types them: separated by commas or spaces, in any case, once each.
pub fn parse_kinds(text: &str) -> Vec<String> {
    let mut kinds: Vec<String> = Vec::new();
    for kind in text.split(|c: char| c == ',' || c.is_whitespace()) {
        let kind = kind.trim().to_lowercase();
        if !kind.is_empty() && !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// A stored row in today's shape. To change the shape: raise `SCHEMA` and add the step here,
/// from the version that needed it. A row from a newer hub is read as far as this one
/// understands it and keeps its number.
pub fn upgrade(mut raw: Value) -> Preferences {
    let version = raw.get("schema").and_then(Value::as_u64).unwrap_or(0) as u32;
    if version < 1
        && let Some(object) = raw.as_object_mut()
    {
        // Before there was a number there was no shape to carry over.
        object.insert("schema".into(), Value::from(1));
    }
    let mut preferences: Preferences = serde_json::from_value(raw).unwrap_or_default();
    preferences.schema = preferences.schema.max(SCHEMA);
    preferences
}

/// What saving changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub preferences: Preferences,
    /// Kinds that were just added to always asking.
    pub added_always_ask: Vec<String>,
}

/// What changed, for the chain: each field and its new value; a name by that fact alone.
fn changes(before: &Preferences, after: &Preferences) -> Vec<String> {
    let mut changed = Vec::new();
    if before.name != after.name {
        changed.push("name".to_string());
    }
    if before.theme != after.theme {
        changed.push(format!("theme: {}", after.theme.as_str()));
    }
    if before.allow_days != after.allow_days {
        changed.push(format!("allow_days: {}", after.allow_days));
    }
    if before.offer_ahead != after.offer_ahead {
        changed.push(format!("offer_ahead: {}", after.offer_ahead));
    }
    if before.always_ask != after.always_ask {
        changed.push(format!("always_ask: {}", after.always_ask.join(", ")));
    }
    if before.machine_pushes != after.machine_pushes {
        changed.push(format!("machine_pushes: {}", after.machine_pushes));
    }
    changed
}

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::new("invalid", message)
}

/// The people's preferences on this hub.
#[derive(Clone)]
pub struct PreferenceStore {
    store: Store,
    /// Unix seconds; tests move it.
    clock: Clock,
}

impl PreferenceStore {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            clock: Arc::new(|| crate::board::system_clock()() / 1000),
        }
    }

    /// This person's preferences; the defaults for someone who never set any.
    pub async fn get(&self, tenant: &str, subject: &str) -> BridgeResult<Preferences> {
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let row = sqlx::query(
                "SELECT body FROM person_preferences WHERE tenant_id = $1 AND subject = $2",
            )
            .bind(tenant)
            .bind(subject)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(match row {
                Some(row) => stored(&row.try_get::<String, _>(0)?),
                None => Preferences::default(),
            })
        })
    }

    /// Save `next` as this person's preferences. Nothing is written or recorded when nothing
    /// changed. What this hub does not know is kept from the row that was there.
    pub async fn save(
        &self,
        tenant: &str,
        subject: &str,
        next: &Preferences,
    ) -> BridgeResult<Saved> {
        next.validate()?;
        let now = (self.clock)();
        transaction!(&self.store, |connection| {
            self.store.scope(connection, tenant).await?;
            let row = sqlx::query(
                "SELECT body FROM person_preferences WHERE tenant_id = $1 AND subject = $2",
            )
            .bind(tenant)
            .bind(subject)
            .fetch_optional(&mut *connection)
            .await?;
            let before = match row {
                Some(row) => stored(&row.try_get::<String, _>(0)?),
                None => Preferences::default(),
            };
            let mut after = next.clone();
            after.schema = before.schema.max(SCHEMA);
            after.extra = before.extra.clone();
            let changed = changes(&before, &after);
            let added_always_ask: Vec<String> = after
                .always_ask
                .iter()
                .filter(|kind| !before.always_asks(kind))
                .cloned()
                .collect();
            if !changed.is_empty() {
                let body = serde_json::to_string(&after).map_err(|error| {
                    BridgeError::new("board_store", format!("preferences: {error}"))
                })?;
                sqlx::query(
                    "INSERT INTO person_preferences
                       (tenant_id, subject, schema_version, body, machine_pushes, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (tenant_id, subject) DO UPDATE SET
                       schema_version = excluded.schema_version, body = excluded.body,
                       machine_pushes = excluded.machine_pushes, updated_at = excluded.updated_at",
                )
                .bind(tenant)
                .bind(subject)
                .bind(after.schema as i32)
                .bind(&body)
                .bind(i32::from(after.machine_pushes))
                .bind(now)
                .execute(&mut *connection)
                .await?;
                record_in(
                    connection,
                    tenant,
                    subject,
                    EVENT,
                    &changed.join("; "),
                    now as u64,
                )
                .await?;
            }
            Ok(Saved {
                preferences: after,
                added_always_ask,
            })
        })
    }

    /// Whether a machine may push to this person, wherever their row is.
    pub async fn machine_pushes(&self, subject: &str) -> BridgeResult<bool> {
        let sql = match self.store.backend() {
            Backend::Postgres => "SELECT devagotchi.machine_pushes_of($1)",
            Backend::Sqlite => {
                "SELECT COALESCE(MIN(machine_pushes), 1) FROM person_preferences WHERE subject = $1"
            }
        };
        let mut connection = self.store.read().await?;
        let on: i32 = sqlx::query_scalar(sql)
            .bind(subject)
            .fetch_one(&mut *connection)
            .await
            .map_err(store_error)?;
        Ok(on != 0)
    }
}

/// The preferences in a stored row. A row this hub cannot read is the defaults, never a page
/// that fails.
fn stored(body: &str) -> Preferences {
    serde_json::from_str(body).map(upgrade).unwrap_or_default()
}

/// Of `people`, those whose machines may push to them. Asked of nobody when the hub keeps no
/// preferences; a store that fails counts as yes, since the push says a machine is in trouble.
pub async fn wanting_machine_pushes(
    preferences: Option<&PreferenceStore>,
    people: Vec<String>,
) -> Vec<String> {
    let Some(preferences) = preferences else {
        return people;
    };
    let mut wanting = Vec::with_capacity(people.len());
    for person in people {
        if preferences.machine_pushes(&person).await.unwrap_or(true) {
            wanting.push(person);
        }
    }
    wanting
}
