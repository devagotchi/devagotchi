//! Calls to a machine: sending one and waiting for its answer, whether it is connected, and the recent jobs a call leaves behind.

use std::{sync::atomic::Ordering, time::Duration};

use devagotchi_contracts::{
    API_VERSION, BridgeError, BridgeResult, RequestBody, ResponseBody, WireRequest, WireResponse,
};
use tokio::sync::{mpsc, oneshot};

use super::{CALL_QUEUE, Job, Links, RECENT_JOBS, Waiter, offline};

pub(super) const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// What a routed start ran, as the machine page shows it: at most 80 characters.
pub(super) fn command_of(body: &RequestBody) -> Option<String> {
    let RequestBody::OperationStart(input) = body else {
        return None;
    };
    let line = match &input.command {
        Some(command) => command.clone(),
        None => std::iter::once(input.executable.as_str())
            .chain(input.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" "),
    };
    Some(line.chars().take(80).collect())
}

impl Links {
    /// Send one broker request over the device's link and wait for its answer.
    pub async fn call(&self, device: &str, body: RequestBody) -> BridgeResult<ResponseBody> {
        match self.devices.get(device).await? {
            None => {
                return Err(BridgeError::new(
                    "not_enrolled",
                    format!(
                        "{device} has never enrolled; the hub operator issues a code with `devagotchi hub enroll`"
                    ),
                ));
            }
            Some(record) if record.revoked_at.is_some() => {
                return Err(BridgeError::new(
                    "device_revoked",
                    format!("{device} is revoked"),
                ));
            }
            Some(_) => {}
        }
        let session = self
            .sessions
            .lock()
            .expect("links")
            .get(device)
            .cloned()
            .ok_or_else(|| offline(device))?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let _capacity = session
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| BridgeError::new("link_busy", "machine call capacity is full"))?;
        let (tx, rx) = oneshot::channel();
        session.pending.lock().expect("pending").insert(id, tx);
        let _waiter = Waiter {
            pending: session.pending.clone(),
            id,
        };
        // A wait is answered when the operation changes or its own timeout passes.
        let within = CALL_TIMEOUT + body.held_for();
        let started = command_of(&body);
        let request = WireRequest {
            api_version: API_VERSION,
            request: body,
        };
        match session.calls.try_send((id, request)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(BridgeError::new(
                    "link_busy",
                    format!("{device} already has {CALL_QUEUE} calls waiting; retry shortly"),
                ));
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(offline(device)),
        }
        let response = match tokio::time::timeout(within, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(offline(device)),
            Err(_) => {
                return Err(BridgeError::new(
                    "link_timeout",
                    format!(
                        "{device} did not answer within {}s; an admitted operation keeps running there",
                        within.as_secs()
                    ),
                ));
            }
        };
        match response {
            WireResponse::Ok(body) => {
                self.jobs_saw(device, started, &body);
                Ok(body)
            }
            WireResponse::Error(error) => Err(error),
        }
    }

    /// `device` as if its link had just connected, for tests elsewhere in the hub: it shows
    /// online, and a workflow waiting on it wakes. The receiver gets the calls routed to it.
    #[cfg(test)]
    pub(crate) fn connect_for_test(&self, device: &str) -> mpsc::Receiver<(u64, WireRequest)> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (session, (received, _frames)) = super::Session::fake(id, None, false);
        self.sessions
            .lock()
            .expect("links")
            .insert(device.to_string(), session);
        self.changed();
        received
    }

    /// Answer call `id` on `device`'s session as its link would, for tests elsewhere in the hub.
    #[cfg(test)]
    pub(crate) fn answer_for_test(&self, device: &str, id: u64, response: WireResponse) {
        let session = self.sessions.lock().expect("links").get(device).cloned();
        let waiter =
            session.and_then(|session| session.pending.lock().expect("pending").remove(&id));
        if let Some(waiter) = waiter {
            let _ = waiter.send(response);
        }
    }

    pub fn connected(&self, device: &str) -> bool {
        self.sessions.lock().expect("links").contains_key(device)
    }

    /// A broker answer through `device`'s link: a start (`started` is what it ran) adds a
    /// job; any later answer about a kept operation moves its state and exit code.
    pub(crate) fn jobs_saw(&self, device: &str, started: Option<String>, answer: &ResponseBody) {
        let (view, start) = match answer {
            ResponseBody::Start(output) => (&output.operation, started),
            ResponseBody::Operation(view) => (view, None),
            ResponseBody::Output(page) => (&page.operation, None),
            ResponseBody::InputSent(sent) => (&sent.operation, None),
            ResponseBody::Operations(_)
            | ResponseBody::File(_)
            | ResponseBody::Listing(_)
            | ResponseBody::Written(_)
            | ResponseBody::Revision(_)
            | ResponseBody::Info(_)
            | ResponseBody::Search(_)
            | ResponseBody::Config(_)
            | ResponseBody::Processes(_)
            | ResponseBody::Killed(_)
            | ResponseBody::Url(_)
            | ResponseBody::Capabilities(_) => return,
        };
        let state = serde_json::to_value(&view.state)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        let mut jobs = self.jobs.lock().expect("jobs");
        let kept = jobs.entry(device.to_string()).or_default();
        if let Some(job) = kept
            .iter_mut()
            .find(|j| j.operation_id == view.operation_id)
        {
            job.state = state;
            job.exit_code = view.exit_code;
        } else if let Some(command) = start {
            kept.push_front(Job {
                operation_id: view.operation_id.clone(),
                at: view.created_at_ms / 1000,
                command,
                state,
                exit_code: view.exit_code,
            });
            kept.truncate(RECENT_JOBS);
        }
    }

    /// `device`'s recent routed operations, newest first.
    pub fn recent_jobs(&self, device: &str) -> Vec<Job> {
        let jobs = self.jobs.lock().expect("jobs");
        jobs.get(device)
            .map(|kept| kept.iter().cloned().collect())
            .unwrap_or_default()
    }
}
