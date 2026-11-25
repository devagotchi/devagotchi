//! Making and reading one account: a new one (and the owner, one per hub), finding one, and the checks on its id and name.

use devagotchi_contracts::{BridgeError, BridgeResult};
use sqlx::AnyConnection;

use super::{Account, Accounts, account_in, new_account_id, owner_in, tenants, valid_account_id};
use crate::{
    machines::now_secs,
    store::{Failed, Lookup, Store, secs, transaction},
};

impl Accounts {
    /// Make an account. There is at most one owner, and an id is never reused.
    pub async fn create(&self, id: &str, name: &str, owner: bool) -> BridgeResult<Account> {
        let name = check_name(id, name)?;
        let now = now_secs();
        let tenant = transaction!(&self.store, |connection| {
            let tenant = tenants::home_in(connection).await?;
            self.store.scope(connection, &tenant).await?;
            create_in(&self.store, connection, id, name, owner, &tenant, now).await?;
            Ok(tenant)
        })?;
        Ok(Account {
            id: id.to_string(),
            name: name.to_string(),
            owner,
            tenant,
            ..Account::default()
        })
    }

    /// The hub's owner, made if there is none yet. `configured` is the config's `owner`, a
    /// subject from an earlier identity provider: the owner keeps it as the account id, so
    /// every board, machine and audit row keyed by it stays theirs. Without one, a new id.
    pub async fn ensure_owner(
        &self,
        configured: Option<&str>,
        name: &str,
        random: [u8; 16],
    ) -> BridgeResult<Account> {
        if let Some(owner) = self.owner().await? {
            return match configured {
                Some(id) if id != owner.id => Err(BridgeError::new(
                    "owner_mismatch",
                    format!(
                        "this hub's owner account is {}, not the config's owner {id}",
                        owner.id
                    ),
                )),
                _ => Ok(owner),
            };
        }
        let id = configured
            .map(str::to_string)
            .unwrap_or_else(|| new_account_id(random));
        self.create(&id, name, true).await
    }

    pub async fn get(&self, id: &str) -> BridgeResult<Option<Account>> {
        transaction!(&self.store, |connection| {
            if self.enter(connection, Lookup::Account(id)).await?.is_none() {
                return Ok(None);
            }
            Ok(account_in(connection, id).await?)
        })
    }

    pub async fn owner(&self) -> BridgeResult<Option<Account>> {
        transaction!(&self.store, |connection| {
            Ok(self.owner_in(connection).await?)
        })
    }

    /// The hub's owner, inside the caller's transaction, which is scoped to the owner's
    /// tenant from here on (to nothing when there is no owner).
    pub(crate) async fn owner_in(
        &self,
        connection: &mut AnyConnection,
    ) -> Result<Option<Account>, sqlx::Error> {
        if self.enter(connection, Lookup::Owner).await?.is_none() {
            return Ok(None);
        }
        owner_in(connection).await
    }
}

pub(crate) fn account_disabled(account: &str) -> BridgeError {
    BridgeError::new("account_disabled", format!("{account} is disabled"))
}

/// An account id and name the hub accepts; the name trimmed.
pub(super) fn check_name<'a>(id: &str, name: &'a str) -> BridgeResult<&'a str> {
    if !valid_account_id(id) {
        return Err(BridgeError::new(
            "invalid_account",
            format!("{id:?} is not an account id"),
        ));
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(BridgeError::new(
            "invalid_account",
            "a name is 1 to 64 printable characters",
        ));
    }
    Ok(name)
}

/// `create`, inside a transaction the caller holds, scoped to `tenant`; `name` already
/// checked. Ids and the owner are one per hub, so those checks look across tenants.
pub(super) async fn create_in(
    store: &Store,
    connection: &mut AnyConnection,
    id: &str,
    name: &str,
    owner: bool,
    tenant: &str,
    now: u64,
) -> Result<(), Failed> {
    if store
        .tenant_of(connection, Lookup::Account(id))
        .await?
        .is_some()
    {
        return Err(BridgeError::new("account_exists", format!("{id} already exists")).into());
    }
    if owner {
        let has_owner = store.tenant_of(connection, Lookup::Owner).await?.is_some();
        if has_owner {
            return Err(BridgeError::new("owner_exists", "this hub already has an owner").into());
        }
    }
    sqlx::query(
        "INSERT INTO auth_accounts (id, name, owner, created_at, tenant_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(name)
    .bind(i64::from(owner))
    .bind(secs(now))
    .bind(tenant)
    .execute(&mut *connection)
    .await?;
    let event = if owner {
        "account.owner_created"
    } else {
        "account.created"
    };
    tenants::record_in(connection, tenant, id, event, "", now).await?;
    Ok(())
}
