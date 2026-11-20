use devagotchi_contracts::{Caller, GetInput, LOCAL_ACCOUNT, SendInput, UPDATE_FAILED};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;

use super::*;
use crate::update::Updater;

fn input_from(caller: Option<Caller>) -> RequestBody {
    RequestBody::OperationInput(SendInput {
        operation_id: "op".into(),
        text: Some("x".into()),
        keys: Vec::new(),
        take_over: true,
        caller,
    })
}

#[test]
fn a_hub_speaks_only_for_its_own_accounts_never_for_this_machines_user() {
    let local = Caller {
        account: LOCAL_ACCOUNT.into(),
        client: "adapter-1".into(),
    };
    let refused = from_a_hub(&input_from(Some(local))).unwrap_err();
    assert_eq!(refused.code, "not_your_operation");
    let other = Caller {
        account: "someone".into(),
        client: "c".into(),
    };
    assert!(from_a_hub(&input_from(Some(other))).is_err());
    assert!(from_a_hub(&input_from(Some(Caller::hub("acct-1", "session-1")))).is_ok());
    // No caller: the broker itself refuses what needs one (input, an interactive start).
    assert!(from_a_hub(&input_from(None)).is_ok());
    let get = RequestBody::OperationGet(GetInput {
        operation_id: "op".into(),
        caller: None,
    });
    assert!(from_a_hub(&get).is_ok());
}

/// Only `unavailable` is worth retrying; any other refusal stops the link (exit 2), so a
/// supervisor does not hammer a hub that said no.
#[test]
fn only_an_unavailable_hub_is_retried() {
    assert!(matches!(
        refusal(REFUSAL_UNAVAILABLE.into(), "store down".into()),
        Ended::Retry(_)
    ));
    for code in ["refused", "protocol_version", "device_revoked"] {
        match refusal(code.into(), "m".into()) {
            Ended::Refused(error) => assert_eq!(error.code, code),
            other => panic!("{code} should stop the link, got {other:?}"),
        }
    }
}

const SHORT: Liveness = Liveness {
    heartbeat: Duration::from_millis(50),
    idle: Duration::from_millis(300),
    send: Duration::from_secs(2),
};
/// For a test that the link stays up: 20 heartbeats to an idle period, so a shared CI runner
/// that stalls the test's fake hub for a few hundred milliseconds (macOS, 2026-11-20: 300ms
/// was not enough twice in a row) does not read as a hub gone silent.
const PATIENT: Liveness = Liveness {
    heartbeat: Duration::from_millis(50),
    idle: Duration::from_secs(1),
    send: Duration::from_secs(2),
};
/// Longest any of these tests may take: a hang fails instead of holding CI.
const BOUND: Duration = Duration::from_secs(20);

/// A hub on a loopback port this test plays, and a config enrolled for it.
async fn fake_hub() -> (TcpListener, LinkConfig) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = LinkConfig {
        hub: format!("http://{}", listener.local_addr().unwrap()),
        device: "gus".into(),
        broker: "unused".into(),
    };
    (listener, config)
}

async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (stream, _) = listener.accept().await.unwrap();
    tokio_tungstenite::accept_async(stream).await.unwrap()
}

async fn next_link_frame(hub: &mut WebSocketStream<TcpStream>) -> LinkFrame {
    loop {
        match hub.next().await {
            Some(Ok(Message::Text(text))) => return serde_json::from_str(&text).unwrap(),
            Some(Ok(_)) => continue,
            other => panic!("the link went away: {other:?}"),
        }
    }
}

async fn to_link(hub: &mut WebSocketStream<TcpStream>, frame: HubFrame) {
    hub.send(Message::text(serde_json::to_string(&frame).unwrap()))
        .await
        .unwrap();
}

/// Challenge, hello, welcome: what every serving connection starts with.
async fn open(hub: &mut WebSocketStream<TcpStream>, config: &LinkConfig) -> LinkFrame {
    let challenge = HubFrame::Challenge {
        nonce: "n".into(),
        hub: config.hub.clone(),
    };
    to_link(hub, challenge).await;
    let hello = next_link_frame(hub).await;
    to_link(
        hub,
        HubFrame::Welcome {
            device: config.device.clone(),
        },
    )
    .await;
    hello
}

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

#[tokio::test]
async fn a_hub_that_names_another_hub_is_refused_as_hub_mismatch() {
    let (listener, config) = fake_hub().await;
    let hub = async {
        let mut hub = accept(&listener).await;
        let challenge = HubFrame::Challenge {
            nonce: "n".into(),
            hub: "https://another.example".into(),
        };
        to_link(&mut hub, challenge).await;
        hub
    };
    let key = key();
    let link = connect_with(&config, &key, Mode::Enroll { code: "dvg-x" }, SHORT);
    let (ended, _hub) = tokio::time::timeout(BOUND, async { tokio::join!(link, hub) })
        .await
        .unwrap();
    match ended {
        Ended::Refused(error) => assert_eq!(error.code, "hub_mismatch"),
        other => panic!("expected hub_mismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn an_update_the_link_cannot_take_is_answered_failed() {
    let (listener, config) = fake_hub().await;
    let state = tempfile::tempdir().unwrap();
    let updater = Updater::new(&config, state.path(), "main-abc1234", false);
    let hub = async {
        let mut hub = accept(&listener).await;
        let hello = open(&mut hub, &config).await;
        let update = HubFrame::Update {
            version: "not a build".into(),
            force: true,
        };
        to_link(&mut hub, update).await;
        let status = loop {
            match next_link_frame(&mut hub).await {
                LinkFrame::UpdateStatus { state, detail } => break (state, detail),
                LinkFrame::Heartbeat => continue,
                other => panic!("unexpected {other:?}"),
            }
        };
        drop(hub);
        (hello, status)
    };
    let key = key();
    let mode = Mode::Serve {
        updater: &updater,
        state_dir: state.path(),
    };
    let link = connect_with(&config, &key, mode, SHORT);
    let (ended, (hello, (state, detail))) =
        tokio::time::timeout(BOUND, async { tokio::join!(link, hub) })
            .await
            .unwrap();
    assert!(
        matches!(hello, LinkFrame::Hello { build: Some(ref b), enroll_code: None, kind: None, .. } if b == "main-abc1234"),
        "{hello:?}"
    );
    assert_eq!(state, UPDATE_FAILED);
    assert!(detail.unwrap().contains("not a plain version"));
    assert!(matches!(ended, Ended::Retry(_)), "{ended:?}");
}

/// A hub that stops answering (a NAT that forgot the connection, a host gone to sleep) is
/// given up after the idle window, though every heartbeat write still lands in the buffer.
#[tokio::test]
async fn a_silent_hub_is_given_up_after_the_idle_window() {
    let (listener, config) = fake_hub().await;
    let state = tempfile::tempdir().unwrap();
    let updater = Updater::new(&config, state.path(), "main-abc1234", false);
    let (opened, mut held) = tokio::sync::oneshot::channel();
    let hub = async {
        let mut hub = accept(&listener).await;
        open(&mut hub, &config).await;
        // Never read again, so never Pong: the socket stays open and says nothing.
        let _ = opened.send(hub);
    };
    let key = key();
    let mode = Mode::Serve {
        updater: &updater,
        state_dir: state.path(),
    };
    let started = tokio::time::Instant::now();
    let link = connect_with(&config, &key, mode, SHORT);
    let (ended, ()) = tokio::time::timeout(BOUND, async { tokio::join!(link, hub) })
        .await
        .unwrap();
    match ended {
        Ended::Retry(why) => assert!(why.contains("silent"), "{why}"),
        other => panic!("expected a retry, got {other:?}"),
    }
    assert!(started.elapsed() >= SHORT.idle);
    assert!(held.try_recv().is_ok(), "the hub kept the socket open");
}

/// The hub sends nothing while it has nothing to ask; its answers to the link's Pings are
/// what keep the link from giving it up.
#[tokio::test]
async fn a_hub_that_only_answers_pings_keeps_the_link() {
    let (listener, config) = fake_hub().await;
    let state = tempfile::tempdir().unwrap();
    let updater = Updater::new(&config, state.path(), "main-abc1234", false);
    let hub = async {
        let mut hub = accept(&listener).await;
        open(&mut hub, &config).await;
        // Reading answers each Ping with a Pong; nothing else is ever sent, for three idle
        // periods: without the Pongs the link would have given the hub up twice over.
        let mut pings = 0;
        let quiet = tokio::time::sleep(PATIENT.idle * 3);
        tokio::pin!(quiet);
        loop {
            tokio::select! {
                () = &mut quiet => break,
                message = hub.next() => match message {
                    Some(Ok(Message::Ping(_))) => pings += 1,
                    Some(Ok(_)) => {}
                    other => panic!("the link went away: {other:?}"),
                },
            }
        }
        pings
    };
    let key = key();
    let mode = Mode::Serve {
        updater: &updater,
        state_dir: state.path(),
    };
    let link = connect_with(&config, &key, mode, PATIENT);
    let pings = tokio::time::timeout(BOUND, async {
        tokio::select! {
            ended = link => panic!("the link gave up a hub that answers: {ended:?}"),
            pings = hub => pings,
        }
    })
    .await
    .unwrap();
    assert!(pings >= 3, "the link pings on every heartbeat: {pings}");
}
