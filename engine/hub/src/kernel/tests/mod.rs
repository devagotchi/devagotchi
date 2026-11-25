//! The lexicon on the hub's Postgres behaves as the kernel's reference store does, and its
//! tools act for the signed-in person only.

use devagotchi_kernel::{
    dictionary::Dictionary,
    lexicon::{
        Change, Entry, Filter, Lexicon, LexiconError, MemoryStore, Operation, Provenance, Scope,
        Store, Wording,
    },
    words::normalize,
};
use serde_json::{Value, json};

use super::Stored;
use crate::{
    store::testing::postgres,
    tenancy::tests::tenants::{hosted_with, refused},
};

mod handoff;

const ALICE: &str = "github:1001";
const BOB: &str = "github:1002";

fn project() -> Scope {
    Scope::Project {
        project_id: "p1".into(),
    }
}

fn wording(phrase: &str, meaning: &str) -> Wording {
    Wording {
        phrase: phrase.into(),
        meaning: meaning.into(),
        ..Wording::default()
    }
}

fn new(request: &str, scope: Scope, operation: Operation) -> Change {
    let provenance = Provenance {
        project_id: match &scope {
            Scope::Project { project_id } => Some(project_id.clone()),
            Scope::Personal => None,
        },
        ..Provenance::user()
    };
    Change {
        request_id: request.into(),
        owner_id: ALICE.into(),
        scope,
        record_id: None,
        expected_revision: None,
        provenance,
        operation,
    }
}

fn moved(request: &str, scope: Scope, id: &str, at: u64, operation: Operation) -> Change {
    Change {
        record_id: Some(id.into()),
        expected_revision: Some(at),
        ..new(request, scope, operation)
    }
}

/// One line per step: what it came to, without the ids a store makes up.
fn line<T>(step: &str, outcome: &Result<T, LexiconError>, show: impl Fn(&T) -> String) -> String {
    match outcome {
        Ok(value) => format!("{step}: {}", show(value)),
        Err(error) => format!("{step}: {}", error.code()),
    }
}

/// The same story on any store, as the lines it leaves.
async fn story<S: Store>(lexicon: &Lexicon<'static, S>) -> Vec<String> {
    let mut out = Vec::new();
    let entry = |e: &Entry| {
        format!(
            "{} r{} {:?}",
            e.state.as_str(),
            e.revision,
            e.wording.phrase
        )
    };
    let mine = lexicon
        .apply(&new(
            "r1",
            Scope::Personal,
            Operation::Define {
                wording: wording("The  Dashboard", "the admin's home page"),
            },
        ))
        .await;
    out.push(line("define", &mine, entry));
    let mine = mine.unwrap();
    let twice = new(
        "r2",
        Scope::Personal,
        Operation::Propose {
            wording: wording("the dashboard", "another"),
        },
    );
    out.push(line("same phrase", &lexicon.apply(&twice).await, entry));
    let proposal = new(
        "r3",
        project(),
        Operation::Propose {
            wording: wording("the dashboard", "the customer's overview"),
        },
    );
    let ours = lexicon.apply(&proposal).await.unwrap();
    let again = lexicon.apply(&proposal).await.unwrap();
    out.push(format!("retry returns it: {}", again.id == ours.id));
    let reused = new(
        "r3",
        project(),
        Operation::Park {
            wording: wording("other", ""),
        },
    );
    out.push(line(
        "request id reused",
        &lexicon.apply(&reused).await,
        entry,
    ));
    let confirm = moved("r4", project(), &ours.id, 1, Operation::Confirm);
    out.push(line("confirm", &lexicon.apply(&confirm).await, entry));
    let stale = moved("r5", project(), &ours.id, 1, Operation::Dismiss);
    out.push(line("stale revision", &lexicon.apply(&stale).await, entry));
    let resolved = lexicon
        .resolve(ALICE, Some("p1"), "the dashboard", "")
        .await
        .unwrap();
    out.push(format!(
        "resolve in p1: {:?} {:?}",
        resolved.origin, resolved.meaning
    ));
    let resolved = lexicon
        .resolve(ALICE, None, "the dashboard", "")
        .await
        .unwrap();
    out.push(format!("resolve alone: {:?}", resolved.meaning));
    out.push(line(
        "bob reads it",
        &lexicon.get(BOB, &ours.id).await,
        entry,
    ));
    // In the store's order, by normalized phrase; two entries with one phrase (their ids
    // differ from store to store) shown in a fixed order.
    let listed = |entries: &Vec<Entry>| {
        let keys: Vec<String> = entries
            .iter()
            .map(|e| normalize(&e.wording.phrase))
            .collect();
        let ordered = keys.windows(2).all(|w| w[0] <= w[1]);
        let mut shown: Vec<(String, String)> =
            keys.into_iter().zip(entries.iter().map(entry)).collect();
        shown.sort();
        let shown: Vec<String> = shown.into_iter().map(|(_, e)| e).collect();
        format!("ordered {ordered}: {}", shown.join(", "))
    };
    out.push(line(
        "bob's list",
        &lexicon.list(BOB, &Scope::Personal, Filter::All).await,
        listed,
    ));
    let retire = moved("r6", Scope::Personal, &mine.id, 1, Operation::Retire);
    out.push(line("retire", &lexicon.apply(&retire).await, entry));
    let fresh = new(
        "r7",
        Scope::Personal,
        Operation::Park {
            wording: wording("the dashboard", ""),
        },
    );
    out.push(line(
        "phrase free again",
        &lexicon.apply(&fresh).await,
        entry,
    ));
    for (request, phrase) in [("r8", "zeta"), ("r9", "Alpha"), ("r10", "beta")] {
        let add = new(
            request,
            Scope::Personal,
            Operation::Propose {
                wording: wording(phrase, "a word"),
            },
        );
        lexicon.apply(&add).await.unwrap();
    }
    out.push(line(
        "list",
        &lexicon.list(ALICE, &Scope::Personal, Filter::All).await,
        listed,
    ));
    let delete = moved("r11", project(), &ours.id, 2, Operation::Delete);
    out.push(line("delete", &lexicon.apply(&delete).await, entry));
    let history = lexicon.history(ALICE, &ours.id).await.unwrap();
    out.push(format!(
        "history: {} events, seq rising {}, words left {}",
        history.len(),
        history.windows(2).all(|w| w[0].seq < w[1].seq),
        history.iter().any(|e| !e.entry.wording.phrase.is_empty())
    ));
    let replayed = lexicon.apply(&proposal).await.unwrap();
    out.push(format!(
        "replay after delete: {:?}",
        replayed.wording.phrase
    ));
    out
}

fn at_rest() -> f64 {
    1_700_000_000.0
}

#[tokio::test]
async fn the_postgres_store_tells_the_same_story_as_the_reference() {
    let test = or_skip!(postgres());
    let reference =
        story(&Lexicon::new(MemoryStore::new(), Dictionary::seed()).with_clock(at_rest)).await;
    let hosted = story(
        &Lexicon::new(Stored::new(test.store.clone()), Dictionary::seed()).with_clock(at_rest),
    )
    .await;
    assert_eq!(hosted, reference);
    // And the story is the one the kernel's rules tell.
    assert_eq!(
        reference,
        [
            "define: confirmed r1 \"The Dashboard\"",
            "same phrase: conflict",
            "retry returns it: true",
            "request id reused: conflict",
            "confirm: confirmed r2 \"the dashboard\"",
            "stale revision: conflict",
            "resolve in p1: Some(Project) Some(\"the customer's overview\")",
            "resolve alone: Some(\"the admin's home page\")",
            "bob reads it: not_found",
            "bob's list: ordered true: ",
            "retire: retired r2 \"The Dashboard\"",
            "phrase free again: inconclusive r1 \"the dashboard\"",
            "list: ordered true: proposed r1 \"Alpha\", proposed r1 \"beta\", inconclusive r1 \"the dashboard\", retired r2 \"The Dashboard\", proposed r1 \"zeta\"",
            "delete: deleted r3 \"\"",
            "history: 3 events, seq rising true, words left false",
            "replay after delete: \"\"",
        ]
    );
}

#[tokio::test]
async fn one_live_phrase_holds_when_two_writes_race() {
    let test = or_skip!(postgres());
    let lexicon = Lexicon::new(Stored::new(test.store.clone()), Dictionary::seed());
    let one = new(
        "a",
        Scope::Personal,
        Operation::Propose {
            wording: wording("race", "first"),
        },
    );
    let two = new(
        "b",
        Scope::Personal,
        Operation::Propose {
            wording: wording("Race", "second"),
        },
    );
    let (first, second) = tokio::join!(lexicon.apply(&one), lexicon.apply(&two));
    assert_eq!(
        [first.is_ok(), second.is_ok()]
            .iter()
            .filter(|ok| **ok)
            .count(),
        1,
        "{first:?} {second:?}"
    );
    let live = lexicon
        .list(ALICE, &Scope::Personal, Filter::Reviewable)
        .await
        .unwrap();
    assert_eq!(live.len(), 1);
}

pub(super) fn text(answer: &Value) -> String {
    answer.to_string()
}

#[tokio::test]
async fn the_tools_keep_each_persons_words_to_themselves() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("ChatGPT").await;

    let all = hub.call(&mut alice, "dictionary_get", json!({})).await;
    let concepts = all["structuredContent"]["concepts"].as_array().unwrap();
    assert!(concepts.len() >= 20, "{all}");
    let one = hub
        .call(
            &mut alice,
            "dictionary_get",
            json!({ "concept": "boundary/iam" }),
        )
        .await;
    assert_eq!(
        one["structuredContent"]["concepts"][0]["concept"]["id"],
        "boundary/iam"
    );
    assert!(refused(
        &hub.call(
            &mut alice,
            "dictionary_get",
            json!({ "concept": "boundary/none" })
        )
        .await
    ));

    let proposed = hub
        .call(
            &mut alice,
            "lexicon_propose",
            json!({ "request_id": "q1", "phrase": "the board", "meaning": "our ticket board",
                    "project_id": "p1" }),
        )
        .await;
    let entry = &proposed["structuredContent"]["entry"];
    assert_eq!(entry["state"], "proposed", "{proposed}");
    let id = entry["id"].as_str().unwrap().to_string();
    // A proposal does not resolve ahead of the dictionary until the person confirms it.
    let before = hub
        .call(
            &mut alice,
            "lexicon_resolve",
            json!({ "phrase": "the board", "project_id": "p1" }),
        )
        .await;
    assert_ne!(
        before["structuredContent"]["resolution"]["origin"],
        "project"
    );
    let confirmed = hub
        .call(
            &mut alice,
            "lexicon_review",
            json!({ "action": "confirm", "project_id": "p1", "id": id,
                    "expected_revision": 1, "request_id": "q2" }),
        )
        .await;
    assert_eq!(
        confirmed["structuredContent"]["entry"]["state"],
        "confirmed"
    );
    let after = hub
        .call(
            &mut alice,
            "lexicon_resolve",
            json!({ "phrase": "The Board", "project_id": "p1" }),
        )
        .await;
    let resolution = &after["structuredContent"]["resolution"];
    assert_eq!(resolution["origin"], "project", "{after}");
    assert_eq!(resolution["meaning"], "our ticket board");
    let scanned = hub
        .call(
            &mut alice,
            "lexicon_resolve",
            json!({ "text": "Put it on the board, then wire up Next.js", "project_id": "p1" }),
        )
        .await;
    let found = scanned["structuredContent"]["found"].as_array().unwrap();
    assert!(found.len() >= 2, "{scanned}");
    let history = hub
        .call(
            &mut alice,
            "lexicon_review",
            json!({ "action": "history", "id": id }),
        )
        .await;
    assert_eq!(
        history["structuredContent"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Bob sees none of it, and cannot move it.
    let listed = hub
        .call(
            &mut bob,
            "lexicon_review",
            json!({ "action": "list", "project_id": "p1", "filter": "all" }),
        )
        .await;
    assert_eq!(
        listed["structuredContent"]["entries"],
        json!([]),
        "{listed}"
    );
    let theirs = hub
        .call(
            &mut bob,
            "lexicon_review",
            json!({ "action": "delete", "project_id": "p1", "id": id,
                    "expected_revision": 2, "request_id": "q3" }),
        )
        .await;
    assert!(refused(&theirs), "{theirs}");
    assert!(!text(&theirs).contains("our ticket board"));
    let bobs = hub
        .call(
            &mut bob,
            "lexicon_resolve",
            json!({ "phrase": "the board", "project_id": "p1" }),
        )
        .await;
    assert!(!text(&bobs).contains("our ticket board"), "{bobs}");

    // A call that says neither what to resolve nor what to scan is refused.
    assert!(refused(
        &hub.call(&mut alice, "lexicon_resolve", json!({})).await
    ));
    // A change without the revision it read is refused before it reaches the lexicon.
    assert!(refused(
        &hub.call(
            &mut alice,
            "lexicon_review",
            json!({ "action": "dismiss", "id": id, "request_id": "q4" }),
        )
        .await
    ));
}
