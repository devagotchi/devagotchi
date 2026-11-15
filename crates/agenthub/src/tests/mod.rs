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
mod lanes;
mod schema;
mod updates;
mod workflow;
