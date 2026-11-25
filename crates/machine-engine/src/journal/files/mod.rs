//! File receipts in the existing broker journal.
//! Owns: file_admissions, using the permanent grant_admissions identity registry.
//! Serves: bounded read admissions and durable, non-repeating mutation receipts.
//! Settings: broker max_operations bounds retained payloads; identities are permanent.
//! Depends on: the journal connection and closed shared file contracts.
use super::{Journal, grants, persistence};
use devagotchi_contracts::{
    BridgeError, BridgeResult, FileExecutionState, FileOutcome, GrantAuthority, GrantBinding,
    GrantCall,
};
use rusqlite::{OptionalExtension, params};

pub(crate) struct FileRecord {
    pub id: String,
    pub actor_hash: String,
    pub intent_hash: String,
    pub config_identity: String,
    pub input_hash: String,
    pub call: Option<GrantCall>,
    pub outcome: Option<FileOutcome>,
    pub state: FilePhase,
    pub expires_at: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilePhase {
    Reading,
    Pending,
    Finished,
    Retired,
}
impl FilePhase {
    fn parse(value: &str) -> BridgeResult<Self> {
        match value {
            "reading" => Ok(Self::Reading),
            "pending" => Ok(Self::Pending),
            "finished" => Ok(Self::Finished),
            "retired" => Ok(Self::Retired),
            _ => Err(persistence("unknown file admission state")),
        }
    }
}
impl FileRecord {
    pub(crate) fn status(&self, now: i64) -> FileExecutionState {
        match self.state {
            FilePhase::Retired => FileExecutionState::Retired {},
            FilePhase::Pending if now >= self.expires_at => FileExecutionState::Uncertain {},
            FilePhase::Pending => FileExecutionState::Active {
                expires_at: self.expires_at,
            },
            FilePhase::Reading if now < self.expires_at => FileExecutionState::Active {
                expires_at: self.expires_at,
            },
            FilePhase::Reading | FilePhase::Finished => FileExecutionState::Finished {
                error_code: self
                    .outcome
                    .as_ref()
                    .and_then(FileOutcome::error_code)
                    .map(str::to_owned)
                    .or_else(|| {
                        (self.state == FilePhase::Reading).then(|| "authority_expired".into())
                    }),
            },
        }
    }
}

pub(crate) struct NewFile<'a> {
    pub id: &'a str,
    pub actor_hash: &'a str,
    pub intent_hash: &'a str,
    pub config_identity: &'a str,
    pub call: &'a GrantCall,
    pub mutation: bool,
}

impl Journal {
    pub(super) fn initialize_files(&self) -> BridgeResult<()> {
        self.connection.execute_batch("CREATE TABLE IF NOT EXISTS file_admissions (
            operation_key TEXT PRIMARY KEY NOT NULL,
            operation_id TEXT UNIQUE NOT NULL,
            actor_hash TEXT NOT NULL, intent_hash TEXT NOT NULL,
            config_identity TEXT NOT NULL, input_hash TEXT NOT NULL,
            call_json TEXT, outcome_json TEXT,
            state TEXT NOT NULL CHECK(state IN ('reading','pending','finished','retired')),
            issued_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
            pages INTEGER NOT NULL DEFAULT 0 CHECK(pages>=0));
            CREATE TRIGGER IF NOT EXISTS file_admissions_no_delete BEFORE DELETE ON file_admissions
                BEGIN SELECT RAISE(ABORT,'file retry identities are permanent'); END;
            CREATE TRIGGER IF NOT EXISTS file_admissions_no_identity_update BEFORE UPDATE ON file_admissions
                WHEN NEW.operation_key<>OLD.operation_key OR NEW.operation_id<>OLD.operation_id
                OR NEW.actor_hash<>OLD.actor_hash OR NEW.intent_hash<>OLD.intent_hash
                OR NEW.config_identity<>OLD.config_identity OR NEW.input_hash<>OLD.input_hash
                OR (OLD.state='retired' AND NEW.state<>'retired')
                OR (OLD.state='finished' AND NEW.state NOT IN ('finished','retired'))
                OR (OLD.state='pending' AND NEW.state='reading')
                BEGIN SELECT RAISE(ABORT,'file retry identity cannot be reassigned or revived'); END;
            CREATE TRIGGER IF NOT EXISTS file_admissions_no_replace BEFORE INSERT ON file_admissions
                WHEN EXISTS(SELECT 1 FROM file_admissions WHERE operation_key=NEW.operation_key)
                BEGIN SELECT RAISE(ABORT,'file retry identities cannot be replaced'); END;
            UPDATE file_admissions SET expires_at=0 WHERE state IN ('reading','pending');")
            .map_err(persistence)
    }

    pub(crate) fn file_record(&self, key: &str) -> BridgeResult<Option<FileRecord>> {
        let raw = self
            .connection
            .query_row(
                "SELECT operation_id,actor_hash,intent_hash,config_identity,input_hash,call_json,
            outcome_json,state,expires_at FROM file_admissions WHERE operation_key=?1",
                [key],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(persistence)?;
        raw.map(
            |(
                id,
                actor_hash,
                intent_hash,
                config_identity,
                input_hash,
                call,
                outcome,
                state,
                expires_at,
            )| {
                Ok(FileRecord {
                    id,
                    actor_hash,
                    intent_hash,
                    config_identity,
                    input_hash,
                    call: call
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .map_err(persistence)?,
                    outcome: outcome
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .map_err(persistence)?,
                    state: FilePhase::parse(&state)?,
                    expires_at,
                })
            },
        )
        .transpose()
    }

    pub(crate) fn insert_file(
        &mut self,
        authority: &GrantAuthority,
        new: NewFile<'_>,
    ) -> BridgeResult<()> {
        self.check_grant(&authority.binding)?;
        let binding = &authority.binding;
        let call = serde_json::to_string(new.call).map_err(persistence)?;
        let hash = grants::binding_hash(binding)?;
        let tx = self.connection.transaction().map_err(persistence)?;
        let collision: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM operations WHERE idempotency_key=?1
            UNION ALL SELECT 1 FROM retired_keys WHERE idempotency_key=?1)",
                [&binding.operation_key],
                |r| r.get(0),
            )
            .map_err(persistence)?;
        if collision {
            return Err(BridgeError::new(
                "idempotency_conflict",
                "retry key already belongs to a process",
            ));
        }
        tx.execute("INSERT INTO grant_admissions(hub,machine,generation,admission_id,operation_key,binding_hash)
            VALUES(?1,?2,?3,?4,?5,?6)", params![binding.origin.hub,binding.origin.machine,binding.origin.generation,
                binding.admission_id,binding.operation_key,hash]).map_err(persistence)?;
        tx.execute("INSERT INTO file_admissions(operation_key,operation_id,actor_hash,intent_hash,config_identity,
            input_hash,call_json,state,issued_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![binding.operation_key,new.id,new.actor_hash,new.intent_hash,new.config_identity,binding.input_sha256,
                call,if new.mutation {"pending"} else {"reading"},authority.issued_at,authority.expires_at]).map_err(persistence)?;
        tx.commit().map_err(persistence)
    }

    pub(crate) fn file_page(
        &mut self,
        authority: &GrantAuthority,
        max_pages: usize,
        now: i64,
    ) -> BridgeResult<()> {
        self.check_grant(&authority.binding)?;
        let changed = self
            .connection
            .execute(
                "UPDATE file_admissions SET pages=pages+1,issued_at=?1,expires_at=?2
            WHERE operation_key=?3 AND state='reading' AND expires_at>?4 AND pages<?5
            AND (issued_at<?1 OR (issued_at=?1 AND expires_at=?2))",
                params![
                    authority.issued_at,
                    authority.expires_at,
                    authority.binding.operation_key,
                    now,
                    max_pages
                ],
            )
            .map_err(persistence)?;
        if changed != 1 {
            return Err(BridgeError::new(
                "authority_expired",
                "file admission ended, has a stale authority, or exhausted its page bound",
            ));
        }
        Ok(())
    }

    pub(crate) fn finish_file(
        &mut self,
        binding: &GrantBinding,
        outcome: Option<&FileOutcome>,
    ) -> BridgeResult<()> {
        // Completion persists an already performed result even if a revocation arrived.
        self.check_file_binding(binding)?;
        let json = outcome
            .map(serde_json::to_string)
            .transpose()
            .map_err(persistence)?;
        let changed = self
            .connection
            .execute(
                "UPDATE file_admissions SET state='finished',outcome_json=COALESCE(?1,outcome_json)
            WHERE operation_key=?2 AND state IN ('pending','reading')",
                params![json, binding.operation_key],
            )
            .map_err(persistence)?;
        if changed != 1 {
            return Err(persistence(
                "file completion did not update one retained admission; outcome could not be saved",
            ));
        }
        Ok(())
    }

    /// Keep only the first diagnostic code for a read intent, never file contents or paths.
    pub(crate) fn note_file_error(&self, binding: &GrantBinding, code: &str) -> BridgeResult<()> {
        self.check_file_binding(binding)?;
        let outcome = FileOutcome::Error(BridgeError::new(code, "file read failed"));
        let json = serde_json::to_string(&outcome).map_err(persistence)?;
        self.connection
            .execute(
                "UPDATE file_admissions SET outcome_json=COALESCE(outcome_json,?1)
            WHERE operation_key=?2 AND state='reading'",
                params![json, binding.operation_key],
            )
            .map_err(persistence)?;
        Ok(())
    }

    pub(crate) fn check_file_binding(&self, binding: &GrantBinding) -> BridgeResult<()> {
        binding.validate()?;
        let hash = grants::binding_hash(binding)?;
        let stored: String = self
            .connection
            .query_row(
                "SELECT binding_hash FROM grant_admissions WHERE operation_key=?1",
                [&binding.operation_key],
                |r| r.get(0),
            )
            .map_err(persistence)?;
        if hash != stored {
            return Err(BridgeError::new(
                "authority_mismatch",
                "file completion belongs to another admission",
            ));
        }
        Ok(())
    }

    /// Room for one more admission. A `pending` row is a mutation that may still be doing
    /// its IO: it is retired only once the journal's opening marked it abandoned
    /// (`expires_at` 0), never because its authority's time ran out mid-change. Retiring a
    /// live one would leave its `finish_file` with no row, a fault that stops the broker.
    pub(crate) fn retire_files(&mut self, keep: usize, now: i64) -> BridgeResult<()> {
        let active: usize = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM file_admissions
            WHERE (state='reading' AND expires_at>?1) OR (state='pending' AND expires_at<>0)",
                [now],
                |row| row.get(0),
            )
            .map_err(persistence)?;
        if active > keep {
            return Err(BridgeError::new(
                "file_capacity",
                "broker file admission capacity is full",
            ));
        }
        let keep = keep.saturating_sub(active);
        self.connection
            .execute(
                "UPDATE file_admissions SET state='retired',call_json=NULL,outcome_json=NULL
            WHERE operation_key IN (SELECT operation_key FROM file_admissions
                WHERE state='finished' OR (state='reading' AND expires_at<=?1)
                    OR (state='pending' AND expires_at=0)
                ORDER BY rowid DESC LIMIT -1 OFFSET ?2)",
                params![now, keep],
            )
            .map_err(persistence)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
