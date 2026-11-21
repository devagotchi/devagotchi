//! Durable process admission and independent execution ownership.
use super::*;
impl Broker {
    // No await occurs from committed admission through spawning the independent job.
    pub(super) fn start(&self, input: StartInput) -> BridgeResult<StartOutput> {
        self.start_bound(input, None)
    }
    pub(super) fn start_bound(
        &self,
        input: StartInput,
        authority: Option<GrantAuthority>,
    ) -> BridgeResult<StartOutput> {
        let (mut spec, request_digest) = self.normalize(&input)?;
        if let Some(authority) = &authority {
            remote::check_start(&input, &spec, &request_digest, authority)?;
            spec.account_environment = false;
        }
        let mut state = self.shared.lock();
        if let Some(authority) = &authority {
            state.journal.check_grant(&authority.binding)?;
        }
        if let Some(id) = state.keys.get(&input.idempotency_key).cloned() {
            let operation = state.operations.get_mut(&id).expect("registered operation");
            if operation.record.request_digest != request_digest
                || operation.record.config_identity != self.shared.config_identity
                || operation.record.grant.as_ref() != authority.as_ref().map(|a| &a.binding)
                || !visible(
                    operation.record.owner.as_ref(),
                    input.caller.as_ref(),
                    ListScope::Account,
                )
            {
                return Err(BridgeError::new(
                    "idempotency_conflict",
                    "The retry key is already bound to different canonical inputs or broker configuration",
                ));
            }
            if operation.active
                && let Some(authority) = &authority
            {
                if operation
                    .control
                    .as_ref()
                    .is_none_or(|c| c.borrow().is_some())
                {
                    return Err(BridgeError::new(
                        "authority_expired",
                        "interrupted work cannot be resumed",
                    ));
                }
                operation
                    .authority
                    .as_mut()
                    .ok_or_else(|| {
                        BridgeError::new("authority_expired", "admission authority ended")
                    })?
                    .refresh(authority)?;
            }
            return Ok(StartOutput {
                operation: operation.record.view.clone(),
                replayed: true,
            });
        }
        if state.journal.was_retired(&input.idempotency_key)? {
            return Err(BridgeError::new(
                "idempotency_key_retired",
                "This retry key belonged to an operation that finished and was retired from the broker's history. It is never run again; use a new key for new work.",
            ));
        }
        if let Some(error) = &state.persistence_fault {
            return Err(error.clone());
        }
        if state.closing {
            return Err(BridgeError::new(
                "shutting_down",
                "Broker admission is closed",
            ));
        }
        // Every refusal comes before anything is retired: a refused start evicts no history.
        state.room_for_one()?;
        if state.active_count >= state.limits.max_concurrent {
            return Err(BridgeError::new(
                "concurrency_limit_reached",
                "The active operation limit has been reached",
            ));
        }
        let authority_clock = authority
            .as_ref()
            .map(remote::AuthorityClock::new)
            .transpose()?;
        let expiry = authority_clock
            .as_ref()
            .map(|clock| clock.deadline.subscribe());
        state.make_room()?;
        let timestamp = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        let view = OperationView {
            operation_id: id.clone(),
            executable: path_string(&spec.executable)?,
            args: spec.args.clone(),
            cwd: path_string(&spec.cwd)?,
            deadline_ms: spec.deadline_ms,
            state: OperationState::Accepted,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
            exit_code: None,
            detail: Some(with_launch(
                spec.launch.as_deref(),
                if spec.account_environment {
                    "Admission committed; the independent broker job owns execution. It runs as the broker's own account with that account's environment; this is not a sandbox."
                } else {
                    "Admission committed; the independent broker job owns execution. Executable grants and cwd checks do not sandbox the executable."
                },
            )),
            ownership_scope: match &spec.interactive {
                Some(InteractiveInput {
                    mode: InputMode::Terminal,
                    ..
                }) => SCOPE_TERMINAL,
                _ => SCOPE,
            }
            .into(),
            cancellation_requested: false,
            output_complete: false,
            interactive: spec
                .interactive
                .as_ref()
                .map(|interactive| InteractiveView {
                    mode: interactive.mode,
                    cols: interactive.cols,
                    rows: interactive.rows,
                    input_open: true,
                    writer_until_ms: None,
                }),
        };
        let record = JournalOperation {
            key: input.idempotency_key.clone(),
            request_digest,
            config_identity: self.shared.config_identity.clone(),
            owner: input.caller.clone(),
            grant: authority.map(|a| a.binding),
            view: view.clone(),
            latest_sequence: 0,
            frames: VecDeque::new(),
        };
        if let Err(error) = state.journal.insert(&record) {
            state.fault(error.clone());
            return Err(error);
        }
        let (control, receiver) = watch::channel(None);
        let (input_sender, input_receiver) = match spec.interactive {
            Some(_) => {
                let (sender, receiver) = mpsc::channel(INPUT_QUEUE);
                (Some(sender), Some(receiver))
            }
            None => (None, None),
        };
        state.keys.insert(input.idempotency_key, id.clone());
        state.operations.insert(
            id.clone(),
            Operation {
                record,
                retained_bytes: 0,
                active: true,
                control: Some(control),
                persistence_fault: None,
                input: input_sender,
                writer: None,
                launch: spec.launch.clone(),
                pid: None,
                authority: authority_clock,
            },
        );
        state.active_count += 1;
        drop(state);
        if let Some(expiry) = expiry {
            remote::watch_expiry(&self.shared, id.clone(), expiry);
        }
        // Admitted (and perhaps another retired to make room): the list changed.
        self.shared.changed();
        let shared = self.shared.clone();
        let deadline = Instant::now() + Duration::from_millis(spec.deadline_ms);
        let job_id = id.clone();
        let job_shared = shared.clone();
        let job = tokio::spawn(async move {
            run_job(job_shared, job_id, spec, receiver, deadline, input_receiver).await;
        });
        // A panic in a job cannot silently strand an active registry entry.
        tokio::spawn(async move {
            if let Err(error) = job.await {
                shared.finish(&id, OperationState::Unknown, None, false,
                    format!("Owned job did not settle normally: {error}. Process effects and resource status are unknown."));
            }
        });
        Ok(StartOutput {
            operation: view,
            replayed: false,
        })
    }
}
