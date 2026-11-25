use super::*;

fn row(journal: &Journal, key: &str, state: &str, expires_at: i64) {
    journal
        .connection
        .execute(
            "INSERT INTO file_admissions (operation_key, operation_id, actor_hash, intent_hash,
                config_identity, input_hash, state, issued_at, expires_at)
            VALUES (?1, ?1, 'a', 'i', 'c', 'h', ?2, 1, ?3)",
            params![key, state, expires_at],
        )
        .unwrap();
}

fn state_of(journal: &Journal, key: &str) -> String {
    journal
        .connection
        .query_row(
            "SELECT state FROM file_admissions WHERE operation_key=?1",
            [key],
            |r| r.get(0),
        )
        .unwrap()
}

/// A mutation still doing its IO is never retired because its authority's time ran out:
/// its `finish_file` would find no row and fault the broker (review of #291, finding 2).
/// Only a row the journal's opening marked abandoned (`expires_at` 0) and finished rows go.
#[test]
fn a_live_mutation_is_never_retired_mid_change() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path()).unwrap();
    row(&journal, "live", "pending", 100);
    row(&journal, "abandoned", "pending", 0);
    row(&journal, "done", "finished", 100);
    row(&journal, "stale-read", "reading", 100);
    // The live mutation still counts against capacity, so keep room for it.
    journal.retire_files(1, 1_000).unwrap();
    assert_eq!(state_of(&journal, "live"), "pending");
    assert_eq!(state_of(&journal, "abandoned"), "retired");
    assert_eq!(state_of(&journal, "done"), "retired");
    assert_eq!(state_of(&journal, "stale-read"), "retired");
    // With no room beyond it, the live mutation is what fills the capacity.
    assert_eq!(
        journal.retire_files(0, 1_000).unwrap_err().code,
        "file_capacity"
    );
}
