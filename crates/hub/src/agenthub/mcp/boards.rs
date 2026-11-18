//! The boards a hub serves: opened on first use and kept open, the account boards it
//! binds, and the fence each subject was last handed per ticket.

use std::{collections::BTreeMap, collections::HashMap, sync::Arc, sync::Mutex, time::Duration};

use devagotchi_contracts::{BridgeError, BridgeResult};

use super::{Accounts, Boards, Bound, SHARED_OWNER};
use crate::{
    agenthub::{Actor, Board},
    config::{DEFAULT_FREE_LIMIT, HubConfig, valid_name},
    identity::Subject,
    tenancy::orgs,
};

impl Boards {
    /// The boards a hub with `config` serves, from its Postgres `store`, with the hub's
    /// organisations and people's preferences beside them.
    pub(crate) fn new(config: &HubConfig, store: crate::store::Store) -> BridgeResult<Self> {
        let accounts = config.account_boards.as_ref().map(|a| Accounts {
            owner: config.owner_handle(),
            free_limit: a.free_limit.unwrap_or(DEFAULT_FREE_LIMIT) as usize,
            unlimited: a.unlimited.iter().cloned().collect(),
        });
        Ok(Self {
            hosted: crate::agenthub::registry::Registry::new(store.clone())?,
            orgs: Some(orgs::Orgs::new(store.clone())),
            preferences: Some(crate::identity::PreferenceStore::new(store)),
            warehouse: std::sync::RwLock::new(None),
            sinks: std::sync::OnceLock::new(),
            configured: config.boards.iter().map(|b| b.project.clone()).collect(),
            accounts,
            site: config.site_cell(),
            public_url: config.public_url.clone(),
            open: Mutex::new(BTreeMap::new()),
            fences: Mutex::new(HashMap::new()),
            activity: Arc::new(tokio::sync::Notify::new()),
            inbox_checks: Mutex::new(HashMap::new()),
            wake_limits: Arc::new(crate::agenthub::wake::Limiter::default()),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.configured.is_empty() && self.accounts.is_none()
    }

    /// This hub's organisations, when it keeps them (always under `hub serve`).
    pub(crate) fn orgs(&self) -> Option<&orgs::Orgs> {
        self.orgs.as_ref()
    }

    /// What people told this hub they want, when it keeps it (always under `hub serve`).
    pub(crate) fn preferences(&self) -> Option<&crate::identity::PreferenceStore> {
        self.preferences.as_ref()
    }

    /// Export these boards' events to ClickHouse through `warehouse`, or stop exporting
    /// (`None`). The export it replaces is stopped; its caller runs the new one.
    pub(crate) fn set_warehouse(&self, warehouse: Option<Arc<crate::ops::warehouse::Warehouse>>) {
        let old = std::mem::replace(
            &mut *self.warehouse.write().expect("warehouse lock"),
            warehouse,
        );
        if let Some(old) = old {
            old.stop();
        }
    }

    /// The ClickHouse export, when the hub has one.
    pub(crate) fn warehouse(&self) -> Option<Arc<crate::ops::warehouse::Warehouse>> {
        self.warehouse.read().expect("warehouse lock").clone()
    }

    /// How many signal sinks this hub sends to.
    pub(crate) fn signal_sink_count(&self) -> usize {
        self.sinks.get().map_or(0, Vec::len)
    }

    /// The signal sinks (set once, at start).
    pub(crate) fn set_sinks(&self, sinks: Vec<Arc<crate::notify::signals::Sink>>) {
        let _ = self.sinks.set(sinks);
    }

    /// The signal sinks, none when the hub has none.
    pub(crate) fn sinks(&self) -> &[Arc<crate::notify::signals::Sink>] {
        self.sinks.get().map(Vec::as_slice).unwrap_or_default()
    }

    /// Run a store call to the end from synchronous board code, as the hosted boards do.
    pub(crate) fn block<T>(
        &self,
        future: impl std::future::Future<Output = BridgeResult<T>>,
    ) -> BridgeResult<T> {
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|error| BridgeError::new("board_runtime", error.to_string()))?;
        crate::agenthub::hosted::blocking(&handle, future)
    }

    pub(crate) async fn postgres_listener(url: &str) -> BridgeResult<sqlx::postgres::PgListener> {
        let mut listener = sqlx::postgres::PgListener::connect(url)
            .await
            .map_err(crate::store::store_error)?;
        listener
            .listen("devagotchi_board_changes")
            .await
            .map_err(crate::store::store_error)?;
        Ok(listener)
    }

    pub(crate) async fn listen_postgres(self: Arc<Self>, mut listener: sqlx::postgres::PgListener) {
        loop {
            let notification = match listener.try_recv().await {
                Ok(Some(notification)) => {
                    match serde_json::from_str::<(String, String)>(notification.payload()) {
                        Ok(scope) => Some(scope),
                        Err(error) => {
                            eprintln!("devagotchi hub: invalid board notification: {error}");
                            continue;
                        }
                    }
                }
                Ok(None) => {
                    eprintln!(
                        "devagotchi hub: board notification connection reconnected; refreshing open boards"
                    );
                    None
                }
                Err(error) => {
                    eprintln!("devagotchi hub: board notifications unavailable: {error}");
                    self.wake_hosted(None);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            self.wake_hosted(notification.as_ref());
        }
    }

    pub(super) fn wake_hosted(&self, notification: Option<&(String, String)>) {
        let open = self.open.lock().expect("boards lock");
        for board in open.values() {
            if notification
                .is_none_or(|(tenant, id)| board.hosted().matches_notification(tenant, id))
            {
                board.mark_changed();
            }
        }
        self.activity.notify_one();
    }

    /// Whether this hub hosts account boards at all.
    pub fn hosts_accounts(&self) -> bool {
        self.accounts.is_some()
    }

    /// The account boards `subject` holds here, by name, sorted.
    pub fn account_boards(&self, subject: &str) -> BridgeResult<Vec<String>> {
        self.hosted.blocking(self.hosted.names(subject))
    }

    /// Who owns the boards `subject` makes and finds: its org when the identity provider
    /// named one (B13), otherwise the account itself. Org ids and account ids never
    /// collide: an org's is `org-<id>`.
    pub fn owner_of(subject: &Subject) -> String {
        match &subject.org {
            Some(org) => format!("org-{org}"),
            None => subject.id.clone(),
        }
    }

    /// The account's own board named `project`, if it has one.
    pub fn open_account(&self, subject: &str, project: &str) -> BridgeResult<Option<Arc<Board>>> {
        if self.accounts.is_none() {
            return Ok(None);
        }
        if !valid_name(project) {
            return Ok(None);
        }
        let key = format!("{subject}/{project}");
        if let Some(board) = self.open.lock().expect("boards lock").get(&key) {
            return Ok(Some(board.clone()));
        }
        let board = self
            .hosted
            .blocking(self.hosted.open(subject, project, false, None))?;
        Ok(board.map(|(board, _)| {
            self.open
                .lock()
                .expect("boards lock")
                .entry(key)
                .or_insert(board)
                .clone()
        }))
    }

    /// Make (or find) the account's board for `project`. `shared` is every configured
    /// board the account already reaches: the same name cannot mean two boards.
    pub fn bind(&self, subject: &str, project: &str, shared: &[String]) -> BridgeResult<Bound> {
        self.bind_for(subject, &[subject], project, shared)
    }

    /// Make (or find) `owner`'s board for `project` (an account, or `org-<id>`); the free
    /// limit counts `owner`'s boards and is lifted when any of `who` (the caller, its org)
    /// is unlimited.
    pub fn bind_for(
        &self,
        subject: &str,
        who: &[&str],
        project: &str,
        shared: &[String],
    ) -> BridgeResult<Bound> {
        self.bind_held(subject, who, project, shared, None)
    }

    /// `bind_for`, with the limit a plan sets in place of the free limit (`planned`:
    /// `Some(Some(n))` at most `n`, `Some(None)` none; `tenancy/gate.rs`). The owner and the
    /// `unlimited` accounts have none either way.
    pub fn bind_held(
        &self,
        subject: &str,
        who: &[&str],
        project: &str,
        shared: &[String],
        planned: Option<Option<usize>>,
    ) -> BridgeResult<Bound> {
        let Some(accounts) = &self.accounts else {
            return Err(BridgeError::new(
                "not_offered",
                "this hub does not host account boards; its boards are the ones its operator configured",
            ));
        };
        if !valid_name(project) {
            return Err(BridgeError::new(
                "invalid",
                format!("project {project:?} is not [a-z0-9][a-z0-9-]{{0,62}}"),
            ));
        }
        if shared.iter().any(|name| name == project) {
            return Err(BridgeError::new(
                "name_taken",
                format!(
                    "{project:?} is a board this hub already lets you work on; bind another name"
                ),
            ));
        }
        let held = self.account_boards(subject)?;
        let limit = (!who.iter().any(|id| {
            accounts.unlimited.contains(*id) || accounts.owner.get().is_some_and(|o| o == id)
        }))
        .then_some(planned.unwrap_or(Some(accounts.free_limit)))
        .flatten();
        let existed = held.iter().any(|name| name == project);
        if let Some(limit) = limit
            && !existed
            && held.len() >= limit
        {
            return Err(BridgeError::new(
                "plan_limit",
                format!(
                    "a free {} holds {limit} project{} on this hub and this one already has {}; unbind one, or ask the operator",
                    if subject.starts_with("org-") {
                        "org"
                    } else {
                        "account"
                    },
                    if limit == 1 { "" } else { "s" },
                    held.join(", ")
                ),
            ));
        }
        let created = !existed && self.make_owned(subject, project, limit)?;
        let mut boards = self.account_boards(subject)?;
        boards.extend(shared.iter().cloned());
        Ok(Bound {
            project: project.to_string(),
            created,
            boards,
            limit,
        })
    }

    /// Make `owner`'s board for `project` (an account, an identity provider's `org-<id>`
    /// or an organisation's `org-t-<uuid>`) with its getting-started ticket; whether it was
    /// new. `limit` caps how many `owner` holds (hosted boards check it in the store).
    pub(super) fn make_owned(
        &self,
        owner: &str,
        project: &str,
        limit: Option<usize>,
    ) -> BridgeResult<bool> {
        let (board, fresh) = self
            .hosted
            .blocking(self.hosted.open(owner, project, true, limit))?
            .ok_or_else(|| BridgeError::new("board_store", "created board was not returned"))?;
        let (title, steps) = self.site.get().welcome(&self.public_url);
        board.welcome_if_new(&Actor::agent("devagotchi"), &title, &steps)?;
        self.open
            .lock()
            .expect("boards lock")
            .entry(format!("{owner}/{project}"))
            .or_insert(board);
        Ok(fresh)
    }
    /// Make (or find) an organisation's board for `project`: no free limit, since an
    /// organisation's plan is its own (`crate::orgs`). Whether it was new.
    pub(crate) fn make_for_org(&self, owner: &str, project: &str) -> BridgeResult<bool> {
        if !valid_name(project) {
            return Err(BridgeError::new(
                "invalid",
                format!("project {project:?} is not [a-z0-9][a-z0-9-]{{0,62}}"),
            ));
        }
        if self.open_account(owner, project)?.is_some() {
            return Ok(false);
        }
        self.make_owned(owner, project, None)
    }

    /// The configured board named `project` (owned by `@shared`), made on its first use with
    /// the steps to check the hub works, so the first session never finds it empty.
    pub fn open(&self, project: &str) -> BridgeResult<Arc<Board>> {
        if let Some(board) = self.open.lock().expect("boards lock").get(project) {
            return Ok(board.clone());
        }
        if !self.configured.contains(project) {
            return Err(BridgeError::new(
                "unknown_board",
                format!("no board for project {project:?} is configured on this hub"),
            ));
        }
        let (board, _) = self
            .hosted
            .blocking(self.hosted.open(SHARED_OWNER, project, true, None))?
            .ok_or_else(|| BridgeError::new("board_store", "configured board was not returned"))?;
        let (title, steps) = self.site.get().welcome(&self.public_url);
        board.welcome_if_new(&Actor::agent("devagotchi"), &title, &steps)?;
        Ok(self
            .open
            .lock()
            .expect("boards lock")
            .entry(project.into())
            .or_insert(board)
            .clone())
    }
    /// Keep leases honest without a sweep timer: sleep until the first lease on any open
    /// board lapses, or until a board changes (a claim, heartbeat or release moves that
    /// moment), then expire what has lapsed. Runs for the life of the hub.
    pub async fn reap_on_time(self: Arc<Self>) {
        let hosted = &self.hosted;
        loop {
            match hosted.next_lease_delay().await {
                Ok(Some(delay)) if delay <= 0 => {
                    if let Err(error) = hosted.reap_due().await {
                        eprintln!("devagotchi hub: persisted lease recovery failed: {error}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
                Ok(Some(delay)) => {
                    tokio::select! {
                        _=self.activity.notified()=>{}
                        _=tokio::time::sleep(Duration::from_millis(delay as u64))=>{}
                    }
                }
                Ok(None) => self.activity.notified().await,
                Err(error) => {
                    eprintln!("devagotchi hub: persisted lease schedule unavailable: {error}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    pub(super) fn remember_fence(&self, subject: &str, project: &str, ticket: &str, fence: i64) {
        self.fences.lock().expect("fence lock").insert(
            (subject.to_string(), project.to_string(), ticket.to_string()),
            fence,
        );
    }

    pub(super) fn forget_fence(&self, subject: &str, project: &str, ticket: &str) {
        self.fences.lock().expect("fence lock").remove(&(
            subject.to_string(),
            project.to_string(),
            ticket.to_string(),
        ));
    }

    pub(super) fn fence_for(
        &self,
        subject: &str,
        project: &str,
        ticket: &str,
        given: Option<i64>,
    ) -> BridgeResult<i64> {
        if let Some(fence) = given {
            return Ok(fence);
        }
        self.fences
            .lock()
            .expect("fence lock")
            .get(&(subject.to_string(), project.to_string(), ticket.to_string()))
            .copied()
            .ok_or_else(|| {
                BridgeError::new(
                    "stale_fence",
                    format!(
                        "no fence known for {ticket}: pass the fence from your claim, or claim it again"
                    ),
                )
            })
    }
}
