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
        eprintln!(
            "{}",
            serde_json::json!({
                "level": "error", "event": "audit.write_failed", "tool": tool,
                "outcome": outcome, "message": "The tool result was returned; its audit entry could not be saved. Inspect the audit store."
            })
        );
    }

    pub(super) fn alert(&self) -> Option<serde_json::Value> {
        let count = self.count.load(Ordering::Relaxed);
        (count != 0).then(|| serde_json::json!({
            "code": "audit.write_failed", "failures": count,
            "message": "Audit entries are missing. Inspect the hub log and audit store. This alert remains until the hub restarts."
        }))
    }
}

impl Audit {
    /// One append for something already done that no MCP call made (a machine's link
    /// connecting, dropping, enrolling, updating): a failure raises the operator alert, and
    /// what happened stands.
    pub(crate) async fn record_after(&self, entry: &AuditEntry<'_>) {
        if self.record(entry).await.is_err() {
            self.write_failures.raise(entry.tool, entry.outcome);
        }
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
        let outcome = match &result {
            Ok(_) => "ok",
            Err(error) => error.code.as_str(),
        };
        self.record_completion(subject, tool, place, target, outcome)
            .await;
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
        if self
            .record(subject, tool, place, target, outcome)
            .await
            .is_err()
        {
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

    /// Decision D1 (operator, 2026-11-19): after an effect, a failed audit write never turns
    /// the answer into a refusal. The real result comes back, success or the effect's own
    /// error, and the latched alert says entries are missing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_lost_audit_keeps_the_real_result_and_raises_the_alert() {
        let hosted = devagotchi_store::or_skip!(crate::tenancy::tests::tenants::hosted_with(
            None,
            |_, _| serde_json::json!({})
        ));
        let hub = &hosted.hub;
        let subject = crate::identity::Subject {
            id: "300000000000000009".into(),
            via: "hub",
            name: None,
            tenant: None,
            org: None,
            provisional_until: None,
            agent: None,
            claim_code: None,
            provisional_warning: false,
            access: None,
            client: None,
        };
        // While the store answers, the line is written and nothing is raised.
        let kept = hub
            .recorded(&subject, "write_file", "m", "/a", Ok::<u32, BridgeError>(7))
            .await;
        assert_eq!(kept.ok().map(|rmcp::Json(n)| n), Some(7));
        assert!(hub.audit.write_failure_alert().is_none());

        hub.audit.store().pool().close().await;
        let done = hub
            .recorded(&subject, "write_file", "m", "/a", Ok::<u32, BridgeError>(8))
            .await;
        assert_eq!(
            done.ok().map(|rmcp::Json(n)| n),
            Some(8),
            "the effect's result survives"
        );
        let refused = hub
            .recorded::<u32>(
                &subject,
                "write_file",
                "m",
                "/b",
                Err(BridgeError::new("edit_mismatch", "no")),
            )
            .await;
        assert_eq!(
            refused.err().map(|rmcp::Json(e)| e.code),
            Some("edit_mismatch".to_string())
        );
        let alert = hub
            .audit
            .write_failure_alert()
            .expect("the alert is raised");
        assert_eq!(alert["failures"], 2);
    }
}
