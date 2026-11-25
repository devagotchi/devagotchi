-- get_recent_tool_calls and get_usage_stats (machines/history.rs) read one subject's calls,
-- newest first, out of every subject's: an index on (subject, seq) keeps that a short read
-- however long the chain grows.
CREATE INDEX audit_entries_subject ON devagotchi.audit_entries (subject, seq DESC);
