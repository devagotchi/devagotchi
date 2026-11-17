//! A person's lexicon handed to Clyffy and taken back through `lexicon_handoff`, on the hub's
//! Postgres: one writer at a time, nothing decided for the person, another person untouched.

use devagotchi_kernel::{
    handoff::{LexiconBundleV1, Side},
    lexicon::{Entry, Provenance, Scope, State, Wording},
};
use serde_json::{Value, json};

use super::text;
use crate::tenancy::tests::tenants::{Hosted, Person, hosted_with, refused};

async fn handoff(hub: &Hosted, person: &mut Person, arguments: Value) -> Value {
    hub.call(person, "lexicon_handoff", arguments).await
}

fn out(answer: &Value) -> &Value {
    &answer["structuredContent"]
}

async fn propose(hub: &Hosted, person: &mut Person, request: &str, phrase: &str) -> Value {
    hub.call(
        person,
        "lexicon_propose",
        json!({ "request_id": request, "phrase": phrase, "meaning": "a guess" }),
    )
    .await
}

fn clyffys(owner: &str, id: &str, phrase: &str, meaning: &str) -> Entry {
    Entry {
        id: id.into(),
        owner_id: owner.into(),
        scope: Scope::Personal,
        wording: Wording {
            phrase: phrase.into(),
            meaning: meaning.into(),
            ..Wording::default()
        },
        state: State::Confirmed,
        revision: 1,
        created_at: 8.0,
        updated_at: 8.0,
        provenance: Provenance::user(),
    }
}

fn bundle(owner: &str, epoch: u64, entries: &[Entry]) -> Value {
    serde_json::to_value(LexiconBundleV1::new(
        owner,
        Side::Clyffy,
        epoch,
        9.0,
        entries,
    ))
    .unwrap()
}

fn step(action: &str, bundle: Option<Value>) -> Value {
    match bundle {
        Some(bundle) => json!({ "action": action, "bundle": bundle }),
        None => json!({ "action": action }),
    }
}

fn stands(answer: &Value, holder: &str, epoch: u64) {
    assert_eq!(out(answer)["holder"], holder, "{answer}");
    assert_eq!(out(answer)["epoch"], epoch, "{answer}");
}

#[tokio::test]
async fn a_persons_words_go_to_clyffy_and_come_back() {
    let hub = or_skip!(hosted_with(None, |_, _| json!({})));
    let mut alice = hub.first_connect("Claude").await;
    let mut bob = hub.first_connect("ChatGPT").await;

    let panel = hub
        .call(
            &mut alice,
            "lexicon_propose",
            json!({ "request_id": "r1", "phrase": "the panel", "meaning": "the admin UI",
                    "start": "confirmed" }),
        )
        .await;
    assert_eq!(out(&panel)["entry"]["state"], "confirmed", "{panel}");
    let guess = propose(&hub, &mut alice, "r2", "the board").await;
    let guess_id = out(&guess)["entry"]["id"].as_str().unwrap().to_string();
    let status = handoff(&hub, &mut alice, step("status", None)).await;
    stands(&status, "devagotchi", 1);
    let owner = out(&status)["owner_id"].as_str().unwrap().to_string();

    // Clyffy confirmed another meaning for the panel: the preview leaves it to the person.
    let clash = clyffys(&owner, "c1", "The Panel", "the billing page");
    let seen = handoff(
        &hub,
        &mut alice,
        step("preview", Some(bundle(&owner, 1, &[clash.clone()]))),
    )
    .await;
    assert_eq!(
        out(&seen)["conflicts"].as_array().unwrap().len(),
        1,
        "{seen}"
    );
    stands(
        &handoff(&hub, &mut alice, step("status", None)).await,
        "devagotchi",
        1,
    );

    // Begun, this hub writes nothing more, and a handover never decides a conflict.
    stands(
        &handoff(&hub, &mut alice, step("begin", None)).await,
        "handing_over",
        2,
    );
    let late = propose(&hub, &mut alice, "r3", "the deck").await;
    assert!(
        refused(&late) && text(&late).contains("handing_over"),
        "{late}"
    );
    let both = handoff(
        &hub,
        &mut alice,
        step("export", Some(bundle(&owner, 2, &[clash]))),
    )
    .await;
    assert!(
        refused(&both) && text(&both).contains("settle them first"),
        "{both}"
    );

    // Clyffy's own confirmed word for the board replaces the guess, which stays as dismissed.
    let board = clyffys(&owner, "c2", "the board", "the kanban");
    let export = handoff(
        &hub,
        &mut alice,
        step("export", Some(bundle(&owner, 2, &[board]))),
    )
    .await;
    assert_eq!(
        (
            out(&export)["replaced"].clone(),
            out(&export)["entries"].clone()
        ),
        (json!(1), json!(3)),
        "{export}"
    );
    let merged: LexiconBundleV1 = serde_json::from_value(out(&export)["bundle"].clone()).unwrap();
    assert_eq!((merged.from, merged.epoch), (Side::Devagotchi, 2));
    let resolved = hub
        .call(
            &mut alice,
            "lexicon_resolve",
            json!({ "phrase": "the board" }),
        )
        .await;
    assert_eq!(
        out(&resolved)["resolution"]["meaning"],
        "the kanban",
        "{resolved}"
    );

    assert!(refused(
        &handoff(
            &hub,
            &mut alice,
            json!({ "action": "complete", "epoch": 1 })
        )
        .await
    ));
    let complete = handoff(
        &hub,
        &mut alice,
        json!({ "action": "complete", "epoch": 2 }),
    )
    .await;
    stands(&complete, "clyffy", 2);
    assert!(refused(&propose(&hub, &mut alice, "r4", "the deck").await));

    // While Clyffy holds it, its bundles are the mirror: here it deleted the guess and added
    // a word. A bundle from before the handover is refused.
    let mut now = merged.entries();
    for entry in now.iter_mut().filter(|e| e.id == guess_id) {
        entry.state = State::Deleted;
        entry.revision += 1;
        entry.wording = Wording::default();
    }
    now.push(clyffys(&owner, "c3", "the deck", "the slides"));
    let mirrored = handoff(
        &hub,
        &mut alice,
        step("mirror", Some(bundle(&owner, 2, &now))),
    )
    .await;
    assert_eq!(out(&mirrored)["entries"], 4, "{mirrored}");
    assert!(refused(
        &handoff(
            &hub,
            &mut alice,
            step("mirror", Some(bundle(&owner, 1, &now)))
        )
        .await
    ));
    let deck = hub
        .call(
            &mut alice,
            "lexicon_resolve",
            json!({ "phrase": "the deck" }),
        )
        .await;
    assert_eq!(out(&deck)["resolution"]["meaning"], "the slides", "{deck}");
    let history = hub
        .call(
            &mut alice,
            "lexicon_review",
            json!({ "action": "history", "id": guess_id }),
        )
        .await;
    assert!(
        !text(&history).contains("a guess"),
        "deleted there, erased here: {history}"
    );

    // Bob's lexicon never moved, and Alice's bundle is not his to send.
    stands(
        &handoff(&hub, &mut bob, step("status", None)).await,
        "devagotchi",
        1,
    );
    assert!(!refused(&propose(&hub, &mut bob, "b1", "the deck").await));
    assert!(refused(
        &handoff(
            &hub,
            &mut bob,
            step("mirror", Some(bundle(&owner, 1, &now)))
        )
        .await
    ));

    // Leaving Clyffy: its last bundle comes back, and this hub writes again.
    let home = handoff(
        &hub,
        &mut alice,
        step("restore", Some(bundle(&owner, 2, &now))),
    )
    .await;
    stands(&home, "devagotchi", 3);
    assert_eq!(out(&home)["entries"], 4);
    assert!(!refused(
        &propose(&hub, &mut alice, "r5", "the roadmap").await
    ));
    assert!(refused(
        &handoff(
            &hub,
            &mut alice,
            step("mirror", Some(bundle(&owner, 2, &now)))
        )
        .await
    ));
}
