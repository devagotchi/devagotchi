//! Remote file admission and execution, through the existing filesystem owner.
//! Owns: no tables; file receipts belong to the machine journal.
//! Serves: file capability, prepare, page/operation execute and finish envelopes.
//! Settings: broker mutation queue, IO deadline and retained-operation limit.
//! Depends on: shared contracts, native checked IO and the existing grant registry.
use super::*;
use crate::files::Scope;
use crate::journal::files::{FilePhase, FileRecord, NewFile};
use sha2::{Digest, Sha256};

fn hash(value: &impl serde::Serialize) -> BridgeResult<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|e| invalid(&e.to_string()))?,
    )))
}
fn identities(
    origin: &GrantOrigin,
    actor: &GrantActor,
    invocation: &FileInvocation,
) -> BridgeResult<(String, String)> {
    caller(origin, actor)?;
    invocation.validate()?;
    // Client display labels can change on reconnect; the authenticated principal cannot.
    Ok((
        hash(&(origin, &actor.account, &actor.principal))?,
        hash(invocation)?,
    ))
}
fn matches(record: &FileRecord, actor: &str, intent: &str, config: &str) -> BridgeResult<()> {
    if record.actor_hash != actor
        || record.intent_hash != intent
        || record.config_identity != config
    {
        return Err(denied(
            "idempotency_conflict",
            "file retry key belongs to different inputs, principal or broker configuration",
        ));
    }
    Ok(())
}
fn now() -> BridgeResult<i64> {
    now_ms()
        .try_into()
        .map_err(|_| invalid("broker clock overflow"))
}

impl Broker {
    pub(super) fn prepare_files(
        &self,
        origin: GrantOrigin,
        actor: GrantActor,
        level: GrantLevel,
        invocation: FileInvocation,
    ) -> BridgeResult<RemoteReply> {
        supported(level)?;
        let (actor_hash, intent_hash) = identities(&origin, &actor, &invocation)?;
        let prior = {
            let state = self.shared.lock();
            if state.keys.contains_key(&invocation.key)
                || state.journal.was_retired(&invocation.key)?
            {
                return Err(denied(
                    "idempotency_conflict",
                    "file retry key belongs to a process",
                ));
            }
            state.journal.file_record(&invocation.key)?
        };
        let (call, input_sha256) = if let Some(record) = prior {
            matches(
                &record,
                &actor_hash,
                &intent_hash,
                &self.shared.config_identity,
            )?;
            let mut call = record.call.ok_or_else(|| {
                denied(
                    "idempotency_key_retired",
                    "file receipt was retired; this key never executes again",
                )
            })?;
            call.level = level;
            (call, record.input_hash)
        } else {
            let files = Scope {
                workspace: &self.shared.workspace,
                policy: self.shared.config.exec_policy,
                authority: None,
            };
            files.project(&invocation, level, &self.shared.config_identity)?
        };
        Ok(RemoteReply::Prepared {
            action: invocation.action(),
            operation_key: invocation.key,
            input_sha256,
            call,
        })
    }

    pub(super) async fn execute_files(
        &self,
        authority: GrantAuthority,
        client: String,
        invocation: FileInvocation,
        step: FileStep,
    ) -> BridgeResult<RemoteReply> {
        let clock = AuthorityClock::new(&authority)?;
        let deadline = (*clock.deadline.borrow()).min(
            Instant::now() + devagotchi_ipc::IO_TIMEOUT.saturating_sub(Duration::from_secs(1)),
        );
        supported(authority.binding.effective.sandbox_level)?;
        let actor = GrantActor {
            account: authority.binding.account.clone(),
            principal: authority.binding.principal.clone(),
            client,
        };
        let (actor_hash, intent_hash) = identities(&authority.binding.origin, &actor, &invocation)?;
        if invocation.key != authority.binding.operation_key {
            return Err(denied(
                "authority_mismatch",
                "file retry key differs from its admission",
            ));
        }
        let operation = invocation.request_for(&step)?;
        let mutation = invocation.action() == GrantAction::FileWrite;
        let gate = if mutation {
            Some(
                self.mutation_gate(super::super::files::MUTATION_WAIT)
                    .await?,
            )
        } else {
            None
        };
        // The blocking worker owns admission, the gate, execution and durable completion.
        // Dropping the transport wait cannot drop a pending mutation between these steps.
        let shared = Arc::clone(&self.shared);
        tokio::task::spawn_blocking(move || {
            let _gate = gate;
            if Instant::now() >= deadline {
                return Err(denied(
                    "authority_expired",
                    "file authority expired while waiting",
                ));
            }
            let files = Scope {
                workspace: &shared.workspace,
                policy: shared.config.exec_policy,
                authority: None,
            };
            let id;
            let call;
            {
                let mut state = shared.lock();
                if state.closing {
                    return Err(denied("shutting_down", "broker admission is closed"));
                }
                if let Some(error) = &state.persistence_fault {
                    return Err(error.clone());
                }
                state.journal.check_grant(&authority.binding)?;
                if state.keys.contains_key(&invocation.key)
                    || state.journal.was_retired(&invocation.key)?
                {
                    return Err(denied(
                        "idempotency_conflict",
                        "file retry key belongs to a process",
                    ));
                }
                if let Some(record) = state.journal.file_record(&invocation.key)? {
                    matches(&record, &actor_hash, &intent_hash, &shared.config_identity)?;
                    if record.input_hash != authority.binding.input_sha256 {
                        return Err(denied(
                            "authority_mismatch",
                            "file authority differs from its receipt",
                        ));
                    }
                    call = record.call.ok_or_else(|| {
                        denied(
                            "idempotency_key_retired",
                            "file receipt retired; key never executes again",
                        )
                    })?;
                    check_call(&authority, &call)?;
                    if mutation {
                        let outcome = record.outcome.ok_or_else(|| {
                            denied(
                                "file_outcome_uncertain",
                                "file mutation has no provable result; it will not be repeated",
                            )
                        })?;
                        return Ok(RemoteReply::FileExecuted {
                            operation_id: record.id,
                            outcome,
                            replayed: true,
                        });
                    }
                    id = record.id;
                } else {
                    let (projection, digest) = files.project(
                        &invocation,
                        authority.binding.effective.sandbox_level,
                        &shared.config_identity,
                    )?;
                    if digest != authority.binding.input_sha256 {
                        return Err(denied(
                            "authority_mismatch",
                            "file paths changed after preparation",
                        ));
                    }
                    check_call(&authority, &projection)?;
                    call = projection;
                    id = uuid::Uuid::new_v4().to_string();
                    let keep = state.limits.max_operations;
                    state.journal.retire_files(keep.saturating_sub(1), now()?)?;
                    state.journal.insert_file(
                        &authority,
                        NewFile {
                            id: &id,
                            actor_hash: &actor_hash,
                            intent_hash: &intent_hash,
                            config_identity: &shared.config_identity,
                            call: &call,
                            mutation,
                        },
                    )?;
                }
                if !mutation {
                    state
                        .journal
                        .file_page(&authority, invocation.max_pages(), now()?)?;
                }
            }
            let permit = |path: Option<&Path>| -> BridgeResult<()> {
                if Instant::now() >= deadline {
                    return Err(denied("authority_expired", "file authority expired"));
                }
                shared.lock().journal.check_grant(&authority.binding)?;
                if let Some(path) = path {
                    let shown = path_string(path)?;
                    if !call.paths.contains(&shown) {
                        return Err(denied(
                            "authority_mismatch",
                            "resolved file path differs from its prepared targets",
                        ));
                    }
                    let mut actual = GrantCall::new(invocation.action().capability(), call.level);
                    actual.paths.push(shown);
                    check_call(&authority, &actual)?;
                }
                Ok(())
            };
            let checked = Scope {
                workspace: &shared.workspace,
                policy: shared.config.exec_policy,
                authority: Some(&permit),
            };
            let result = (|| {
                permit(None)?;
                let (_, digest) =
                    files.project(&invocation, call.level, &shared.config_identity)?;
                if digest != authority.binding.input_sha256 {
                    return Err(denied("authority_mismatch", "file paths changed before IO"));
                }
                super::super::files::answer(&checked, operation.request()).1
            })();
            let outcome = match result {
                Ok(response) => FileOutcome::Ok(response),
                Err(error) => FileOutcome::Error(error),
            };
            if mutation {
                let mut state = shared.lock();
                if let Err(error) = state
                    .journal
                    .finish_file(&authority.binding, Some(&outcome))
                {
                    state.fault(error.clone());
                    return Err(error);
                }
            }
            if !mutation && let Some(code) = outcome.error_code() {
                shared
                    .lock()
                    .journal
                    .note_file_error(&authority.binding, code)?;
            }
            // Never disclose read bytes through an authority that ended during IO. A
            // mutation's outcome is already journaled and holds no file content: refusing
            // its reply now would report a change that happened as one that did not.
            if !mutation {
                permit(None)?;
            }
            Ok(RemoteReply::FileExecuted {
                operation_id: id,
                outcome,
                replayed: false,
            })
        })
        .await
        .map_err(|e| denied("internal_error", &e.to_string()))?
    }

    pub(super) fn finish_files(&self, binding: GrantBinding) -> BridgeResult<RemoteReply> {
        let mut state = self.shared.lock();
        state.journal.check_grant(&binding)?;
        let record = state
            .journal
            .file_record(&binding.operation_key)?
            .ok_or_else(not_found)?;
        if record.input_hash != binding.input_sha256 {
            return Err(denied(
                "authority_mismatch",
                "file completion differs from admission",
            ));
        }
        // FinishFiles is only a read-completion signal, never a claim of mutation success.
        if record.state == FilePhase::Reading {
            state.journal.finish_file(&binding, None)?;
        }
        Ok(RemoteReply::FileFinished {
            operation_id: record.id,
            error_code: record
                .outcome
                .as_ref()
                .and_then(FileOutcome::error_code)
                .map(str::to_owned),
        })
    }
}

fn check_call(authority: &GrantAuthority, call: &GrantCall) -> BridgeResult<()> {
    let item = &authority.binding.effective;
    item.selector
        .permits(item.capability, item.sandbox_level, call)
}
