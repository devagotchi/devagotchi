//! Completion auditing never rewrites the result of an already attempted effect.
//! The operator alert is independent of Postgres: structured stderr plus a latched health
//! signal. A later successful append cannot erase a missing audit entry.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct WriteFailures {
    count: AtomicU64,
}

impl WriteFailures {
    fn raise(&self, tool: &str, outcome: &str) {
        self.count.fetch_add(1, Ordering::Relaxed);
        // No arguments, output, subject, target or database error text in the public alert.
        eprintln!("{}", serde_json::json!({
            "level": "error", "event": "audit.write_failed", "tool": tool,
            "outcome": outcome, "message": "The tool result was returned; its audit entry could not be saved. Inspect the audit store."
        }));
    }

    pub(super) fn alert(&self) -> Option<serde_json::Value> {
        let count = self.count.load(Ordering::Relaxed);
        (count != 0).then(|| serde_json::json!({
            "code": "audit.write_failed", "failures": count,
            "message": "Audit entries are missing. Inspect the hub log and audit store. This alert remains until the hub restarts."
        }))
    }
}

impl crate::mcp::Hub {
    /// Attempt exactly one audit append after work. Preserve its actual result even when
    /// the audit store fails: retrying the effect could duplicate it.
    pub(crate) async fn recorded<T>(
        &self,
        subject: &crate::identity::Subject,
        tool: &str,
        place: &str,
        target: &str,
        result: BridgeResult<T>,
    ) -> Result<rmcp::Json<T>, rmcp::Json<BridgeError>> {
        let outcome = match &result { Ok(_) => "ok", Err(error) => error.code.as_str() };
        self.record_completion(subject, tool, place, target, outcome).await;
        result.map(rmcp::Json).map_err(rmcp::Json)
    }

    pub(crate) async fn record_completion(
        &self,
        subject: &crate::identity::Subject,
        tool: &str,
        place: &str,
        target: &str,
        outcome: &str,
    ) {
        if self.record(subject, tool, place, target, outcome).await.is_err() {
            self.audit.write_failures.raise(tool, outcome);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_failure_alert_is_latched_and_contains_no_call_content() {
        let failures = WriteFailures::default();
        assert!(failures.alert().is_none());
        failures.raise("write_file", "ok");
        failures.raise("run", "running");
        let alert = failures.alert().unwrap();
        assert_eq!(alert["code"], "audit.write_failed");
        assert_eq!(alert["failures"], 2);
        assert!(alert.get("target").is_none());
    }
}
