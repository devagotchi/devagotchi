//! An approval waits for a person and records the verdict.

use super::*;

#[test]
fn an_approval_waits_for_a_person_and_records_the_verdict() {
    use crate::approval::Verdict;
    let f = fixture();
    let agent = Actor::agent("claude");
    let person = Actor::human("jesse");
    let asked = f
        .board
        .request_approval(
            &agent,
            "Restart the hub",
            "systemctl restart devagotchi-hub",
            "new build",
            "start the old build",
        )
        .expect("request");
    assert_eq!(asked.status, "pending");
    assert_eq!(asked.requested_by, "claude");
    assert!(asked.description.starts_with("What: systemctl restart"));
    let ticket = f.board.get(&asked.id).expect("get").ticket;
    assert_eq!(
        ticket.state, "needs_attention",
        "people see it; no agent queue holds it"
    );
    assert!(
        f.board
            .ready(&agent, 10)
            .expect("ready")
            .iter()
            .all(|t| t.id != asked.id)
    );
    let pending = f.board.pending_approvals().expect("pending");
    assert_eq!(pending.len(), 1);

    // Agents cannot decide or move it; only a person, on the version they saw.
    let refused = f
        .board
        .decide_approval(&agent, &asked.id, asked.version, Verdict::Approved, "")
        .unwrap_err();
    assert_eq!(refused.code, "humans_only");
    let moved = f
        .board
        .update(
            &agent,
            &asked.id,
            ticket.version,
            Patch {
                state: Some("ready".into()),
                ..Patch::default()
            },
        )
        .unwrap_err();
    assert_eq!(moved.code, "approval_decide_only");
    let stale = f
        .board
        .decide_approval(&person, &asked.id, asked.version + 1, Verdict::Approved, "")
        .unwrap_err();
    assert_eq!(stale.code, "approval_changed");

    f.advance(5_000);
    let decided = f
        .board
        .decide_approval(
            &person,
            &asked.id,
            asked.version,
            Verdict::Approved,
            "go ahead",
        )
        .expect("decide");
    assert_eq!(decided.status, "approved");
    assert_eq!(decided.decided_by.as_deref(), Some("jesse"));
    assert_eq!(decided.note.as_deref(), Some("go ahead"));
    assert_eq!(f.board.get(&asked.id).expect("get").ticket.state, "done");
    assert!(f.board.pending_approvals().expect("pending").is_empty());
    let again = f
        .board
        .decide_approval(&person, &asked.id, decided.version, Verdict::Denied, "")
        .unwrap_err();
    assert_eq!(again.code, "already_decided");
    let events = f.board.events(0, 100).expect("events");
    assert!(events.iter().any(|e| e.action == "approval.requested"));
    assert!(
        events
            .iter()
            .any(|e| e.action == "approval.decided" && e.data["verdict"] == "approved")
    );
    assert!(f.board.verify_chain().expect("verify").ok);

    let denied = f
        .board
        .request_approval(&agent, "Drop the table", "DROP TABLE x", "", "")
        .expect("request");
    let denied = f
        .board
        .decide_approval(&person, &denied.id, denied.version, Verdict::Denied, "")
        .expect("deny");
    assert_eq!(denied.status, "denied");
    assert_eq!(denied.note, None);
    assert_eq!(
        f.board.get(&denied.id).expect("get").ticket.state,
        "cancelled"
    );
    assert_eq!(
        f.board
            .request_approval(&agent, "", "x", "", "")
            .unwrap_err()
            .code,
        "invalid"
    );
}
