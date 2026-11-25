//! Bounded, board-scoped SSE over committed history. A slow reader owns at most one page;
//! every emitted event passes live credential, key, profile and board checks again.
use std::{collections::VecDeque, convert::Infallible, time::Duration};

use axum::{
    extract::{Query, State, rejection::QueryRejection},
    http::header,
    response::{
        IntoResponse, Sse,
        sse::{Event, KeepAlive},
    },
};
use devagotchi_contracts::{BridgeError, BridgeResult};
use serde::Deserialize;
use tokio::sync::watch;

use super::{Caller, Reply, invalid};
use crate::{
    AppState,
    agenthub::{Board, EventFilter},
};

const PAGE: usize = 100;
const RECHECK: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    project: String,
    after_seq: Option<i64>,
    from: Option<String>,
}

impl Input {
    fn cursor(&self, caller: &Caller) -> Result<Option<i64>, super::Failure> {
        if self.project.is_empty() {
            return Err(invalid("project is required"));
        }
        let mut values = caller.headers.get_all("last-event-id").iter();
        let header = values
            .next()
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|s| s.parse::<i64>().ok())
                    .ok_or_else(|| invalid("Last-Event-ID must be a nonnegative integer"))
            })
            .transpose()?;
        if values.next().is_some() {
            return Err(invalid("supply only one Last-Event-ID"));
        }
        if self.after_seq.is_some_and(|seq| seq < 0)
            || matches!((header, self.after_seq), (Some(a), Some(b)) if a != b)
        {
            return Err(invalid("stream cursors must be nonnegative and agree"));
        }
        if self.from.as_deref().is_some_and(|from| from != "now") {
            return Err(invalid("from must be now when supplied"));
        }
        let cursor = header.or(self.after_seq);
        if self.from.is_some() {
            if cursor.is_some() {
                return Err(invalid("from=now cannot be combined with a replay cursor"));
            }
            Ok(None)
        } else {
            Ok(Some(cursor.unwrap_or(0)))
        }
    }
}

fn identity(board: &Board) -> (String, String) {
    (
        board.hosted().tenant().into(),
        board.hosted().board_id().into(),
    )
}

struct Reader {
    state: AppState,
    caller: Caller,
    project: String,
    board: (String, String),
    changed: watch::Receiver<u64>,
    after: i64,
    pending: VecDeque<devagotchi_agenthub::Event>,
    retry: bool,
}

impl Reader {
    async fn read(&mut self) -> BridgeResult<()> {
        self.caller.refresh(&self.state).await?;
        let expected = self.board.clone();
        let after = self.after;
        let fetch = self.pending.is_empty();
        let page = self
            .caller
            .read_scoped(&self.project, move |board, _| {
                if identity(board) != expected {
                    return Err(BridgeError::new("not_granted", "stream board changed"));
                }
                if fetch {
                    board.events(after, PAGE)
                } else {
                    Ok(Vec::new())
                }
            })
            .await?;
        self.pending.extend(page);
        Ok(())
    }

    async fn next(mut self) -> Option<(Result<Event, Infallible>, Self)> {
        loop {
            let checked = tokio::time::timeout(READ_TIMEOUT, self.read()).await;
            let error = match checked {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some(BridgeError::new("stream_timeout", "stream read timed out")),
            };
            if let Some(error) = error {
                self.caller
                    .hub
                    .record_completion(
                        &self.caller.subject,
                        "board_stream",
                        &self.project,
                        "",
                        &error.code,
                    )
                    .await;
                return None;
            }
            if self.retry {
                self.retry = false;
                return Some((Ok(Event::default().retry(Duration::from_secs(3))), self));
            }
            if let Some(event) = self.pending.pop_front() {
                let frame = Event::default()
                    .event("hub")
                    .id(event.seq.to_string())
                    .json_data(&event)
                    .ok()?;
                self.after = event.seq;
                return Some((Ok(frame), self));
            }
            // Also poll history after the timeout: LISTEN reconnect/lost notifications
            // cannot strand a reader. Dropping the response drops this future and receiver.
            tokio::select! {
                changed = self.changed.changed() => { if changed.is_err() { return None; } },
                _ = tokio::time::sleep(RECHECK) => {},
            }
        }
    }
}

pub(super) async fn open(
    State(state): State<AppState>,
    mut caller: Caller,
    query: Result<Query<Input>, QueryRejection>,
) -> Reply {
    super::reads::read(&caller)?;
    let Query(input) = query.map_err(|_| invalid("supply project and a valid stream cursor"))?;
    let cursor = input.cursor(&caller)?;
    if !caller.headers.contains_key(header::AUTHORIZATION)
        && caller.headers.contains_key(header::ORIGIN)
        && !crate::web::from_this_hub(&caller.headers, &state.hub.config.public_url)
    {
        return Err(BridgeError::new("not_granted", "browser streams come from this hub").into());
    }
    let initial = tokio::time::timeout(READ_TIMEOUT, async {
        caller.refresh(&state).await?;
        caller
            .read_scoped(&input.project, move |board, _| {
                let changed = board.changes(); // subscribe BEFORE the first history read
                let head = board
                    .events_matching(&EventFilter {
                        descending: true,
                        limit: Some(1),
                        ..Default::default()
                    })?
                    .first()
                    .map(|event| event.seq)
                    .unwrap_or(0);
                let after = cursor.unwrap_or(head);
                if after > head {
                    return Err(BridgeError::new(
                        "invalid",
                        "stream cursor is ahead of this board",
                    ));
                }
                Ok((identity(board), changed, after))
            })
            .await
    })
    .await
    .unwrap_or_else(|_| Err(BridgeError::new("stream_timeout", "stream read timed out")));
    let outcome = initial
        .as_ref()
        .err()
        .map(|e| e.code.as_str())
        .unwrap_or("ok");
    caller
        .hub
        .record_completion(&caller.subject, "board_stream", &input.project, "", outcome)
        .await;
    let (board, changed, after) = initial?;
    let reader = Reader {
        state,
        caller,
        project: input.project,
        board,
        changed,
        after,
        pending: VecDeque::new(),
        retry: true,
    };
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        Sse::new(futures_util::stream::unfold(reader, Reader::next))
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15))),
    )
        .into_response())
}
