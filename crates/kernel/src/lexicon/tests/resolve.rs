//! The resolver and the text scanner, on the shipped seed.

use super::*;

fn resolve(phrase: &str, context: &str) -> Resolution {
    run(lexicon().resolve(ALICE, None, phrase, context)).unwrap()
}

fn resolved(concept: &str) -> Outcome {
    Outcome::Resolved {
        concept: concept.into(),
    }
}

#[test]
fn the_seeds_disambiguation_rows_route_by_the_words_around_them() {
    assert_eq!(resolve("users", "").outcome, resolved("boundary/iam"));
    let counted = resolve("users", "how many users signed up last week");
    assert_eq!(
        counted.outcome,
        Outcome::Elsewhere {
            what: "reporting and analytics".into()
        }
    );
    assert_eq!(counted.origin, Some(Origin::Rule));
    assert_eq!(resolve("chat", "").outcome, resolved("boundary/messaging"));
    assert_eq!(
        resolve("chat", "add a chat with the AI").outcome,
        resolved("boundary/llm")
    );
    assert!(matches!(
        resolve("backend", "").outcome,
        Outcome::Ask { .. }
    ));
    assert_eq!(
        resolve("backend", "a backend where I manage staff").outcome,
        resolved("boundary/admin")
    );
    assert_eq!(
        resolve("admin user", "").outcome,
        Outcome::Split {
            concepts: vec!["boundary/admin".into(), "boundary/iam".into()]
        }
    );
    let trained = resolve("train it on my data", "");
    assert_eq!(trained.outcome, resolved("boundary/llm"));
    assert!(trained.note.unwrap().contains("retrieval"));
    assert!(matches!(
        resolve("settings", "").outcome,
        Outcome::Ask { .. }
    ));
    assert_eq!(
        resolve("settings", "settings for everyone").outcome,
        resolved("boundary/admin")
    );
    assert_eq!(
        resolve("the ORM", "we use prisma").outcome,
        resolved("stack/prisma")
    );
}

#[test]
fn shared_terms_are_ambiguous_until_the_context_names_one() {
    let alone = resolve("role", "");
    assert_eq!(
        alone.outcome,
        Outcome::Ambiguous {
            concepts: vec!["boundary/iam".into(), "boundary/admin".into()]
        }
    );
    assert_eq!(alone.label, Some(LabelKind::Term));
    let picked = resolve("role", "an admin role that can ban a user");
    assert_eq!(picked.outcome, resolved("boundary/admin"));
    assert!(picked.by_context);
    assert_eq!(resolve("billing", "").outcome, Outcome::Unknown);
}

#[test]
fn the_persons_words_come_first_and_retired_ones_still_resolve_last() {
    let l = lexicon();
    let mine = define(
        &l,
        "r1",
        Scope::Personal,
        wording("chat", "the AI sidebar", &["boundary/llm"]),
    );
    let r = run(l.resolve(ALICE, None, "chat", "")).unwrap();
    assert_eq!(
        (r.outcome, r.origin),
        (resolved("boundary/llm"), Some(Origin::Personal))
    );
    let retired = define(
        &l,
        "r2",
        Scope::Personal,
        wording("the hub", "the old admin page", &["boundary/admin"]),
    );
    run(l.apply(&on(&retired, "r3", Operation::Retire))).unwrap();
    let r = run(l.resolve(ALICE, None, "the hub", "")).unwrap();
    assert!(r.retired);
    assert_eq!(r.outcome, resolved("boundary/admin"));
    run(l.apply(&on(&mine, "r4", Operation::Dismiss))).unwrap();
    let r = run(l.resolve(ALICE, None, "chat", "")).unwrap();
    assert_eq!(
        (r.outcome, r.origin),
        (resolved("boundary/messaging"), Some(Origin::Dictionary))
    );
}

#[test]
fn scanning_text_finds_the_longest_phrases_and_respects_spelling() {
    let l = lexicon();
    define(
        &l,
        "r1",
        Scope::Personal,
        wording("the pod", "my dev environment", &[]),
    );
    let found = run(l.scan(
        ALICE,
        None,
        "Let users sign in with Google, add chat with my docs, write it in Go, and go ahead on the pod.",
        false,
    ))
    .unwrap();
    let got: Vec<(&str, Vec<String>)> = found
        .iter()
        .map(|f| (f.written.as_str(), f.resolution.concepts()))
        .collect();
    assert_eq!(
        got,
        [
            ("users", vec!["boundary/iam".to_string()]),
            ("sign in", vec!["boundary/iam".to_string()]),
            ("chat with my docs", vec!["boundary/llm".to_string()]),
            ("Go", vec!["stack/go".to_string()]),
            ("the pod", vec![]),
        ]
    );
    assert_eq!(found[4].resolution.outcome, Outcome::Personal);
    let weak = run(l.scan(ALICE, None, "the database tables", true)).unwrap();
    let weak: Vec<(&str, Vec<String>)> = weak
        .iter()
        .map(|f| (f.written.as_str(), f.resolution.concepts()))
        .collect();
    let sql = vec!["stack/sql".to_string()];
    assert_eq!(weak, [("the database", sql.clone()), ("tables", sql)]);
    assert!(
        run(l.scan(ALICE, None, "the database tables", false))
            .unwrap()
            .is_empty()
    );
}
