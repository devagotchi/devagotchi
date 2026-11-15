//! The protocol, exercised the way the Go hub's `hubtest` did: fencing, expiry,
//! the distinct-reviewer lane, readiness through dependencies, hierarchy, the chain.

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use crate::{
    Actor, AnchorPatch, Anchors, Board, DepKind, Filter, NewTicket, NextFilter, Patch, Settings,
    StateKind, Workflow,
};

pub(super) struct Fixture {
    pub(super) board: Board,
    now: Arc<AtomicI64>,
    _dir: tempfile::TempDir,
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let now = Arc::new(AtomicI64::new(1_700_000_000_000));
    let clock = now.clone();
    let board = Board::open_with_clock(
        &dir.path().join("board.sqlite3"),
        "devagotchi",
        Arc::new(move || clock.load(Ordering::SeqCst)),
    )
    .expect("open board");
    Fixture {
        board,
        now,
        _dir: dir,
    }
}

impl Fixture {
    fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }

    fn new_ticket(&self, actor: &Actor<'_>, title: &str) -> String {
        self.board
            .create(
                actor,
                NewTicket {
                    title: title.into(),
                    ..NewTicket::default()
                },
            )
            .expect("create")
            .id
    }
}

mod anchors;
mod approvals;
mod hosted;
mod hosted_lifecycle;
mod inbox;
mod lanes;
mod schema;
mod standing;
mod updates;
mod workflow;

/// A new database on the test Postgres with AgentHub's tables, or `None` without one.
pub(super) async fn postgres() -> Option<devagotchi_store::testing::TestStore> {
    devagotchi_store::testing::postgres_with(&[&crate::migrations::MIGRATIONS]).await
}

/// The stores a test runs on: [`postgres`], or none.
pub(super) async fn stores() -> Vec<devagotchi_store::testing::TestStore> {
    postgres().await.into_iter().collect()
}

/// The board change channel a hosted board notifies, heard on `url`.
pub(super) async fn board_changes(url: &str) -> sqlx::postgres::PgListener {
    let mut listener = sqlx::postgres::PgListener::connect(url).await.unwrap();
    listener.listen("devagotchi_board_changes").await.unwrap();
    listener
}
