# DG-RETRY-1: the broker's retry lifecycle

What a machine's broker promises about `operation_start` and its `idempotency_key`, whether
the start comes from `devagotchi mcp`, a hub, or the inspector. It is not an exactly-once
promise for every tool, command or outside service. The code is
`crates/machine-engine/src/broker.rs` (`normalize`, `start`), `broker/history.rs` and
`journal.rs`; the tests are `broker/tests/retry.rs`, `broker/tests/lifecycle.rs` and
`journal/tests.rs`.

## A key is one operation, for good

A caller picks the key before it sends the work and keeps it until it knows the outcome.
Once admitted, the key belongs to one operation in that broker's journal:

- The same key with the same request returns that operation (`replayed: true`); nothing runs
  again.
- The same key with a different request is `idempotency_conflict`. So is the same key from
  another account (an operation is its account's alone).
- Once the operation is retired from history, the key is `idempotency_key_retired`, and it
  is never admitted as new work.

## A retired key is remembered permanently

`max_operations` bounds the operation records and their output that the broker holds, not
its memory of keys. Retiring an operation deletes its record and output and writes a compact
tombstone (key, request digest, time) to `retired_keys`. Tombstones never expire: the
protocol has no validity window that would make forgetting one safe, and forgetting one
lets a late retry run its work a second time. The journal grows by one row per finished
operation.

The journal's triggers make a tombstone immutable on every connection: a `DELETE`, an
`UPDATE`, and an `INSERT` or `INSERT OR REPLACE` of a key that is already there all fail,
and the transaction that tried rolls back whole. (`INSERT OR REPLACE` skips delete triggers
while `recursive_triggers` is off, SQLite's default, so the insert has its own guard.) This
guards against a mistaken writer, not against the machine's own account deleting the
journal or the triggers on purpose. A broker build without these triggers prunes tombstones
when it retires, so on a journal this build has opened its retirements fail.

## What a key is bound to

The request digest covers:

- **The broker:** the canonical workspace and the exec policy (`granted` or `account`).
- **The request:** the canonical executable, the arguments, the canonical cwd, `deadline_ms`
  and `interactive`.

The limits (`output_limit_bytes`, `max_operations`, `max_concurrent`) and the executable
grant list are not part of it. Tuning a limit, or adding or removing an unrelated grant,
keeps every key's retry. The grant and the cwd are still checked live on every start,
replays included, so a revoked grant refuses even an exact retry (`executable_not_allowed`).
If the executable or cwd no longer exists, an exact retry is refused too; reconcile with
`operation_get` when the operation ID is known.

An operation admitted by a build from before this contract was bound to another identity
(it included the grant list), so retrying its key against this build is
`idempotency_conflict`. Its tombstone, once it is retired, still refuses the key: that check
is by key alone. This is alpha: there is no compatibility path for the old identity.

## Refusal, history size and restart

- A refused start changes nothing. `operation_limit_reached` (the history is full and every
  operation in it is still running) and `concurrency_limit_reached` are both decided before
  anything is retired, so a refused start evicts no finished history.
- An admitted start retires the oldest finished operations until one more fits. When
  `max_operations` was lowered in broker.json across a restart, that is several at once.
  Every retired key stays retired.
- `set_config_value` saves a new limit to broker.json first and applies it only once it is
  saved; one that cannot be saved is refused (`config_not_saved`) and changes nothing.
  A lower `max_operations` then retires finished operations down to it at once; running ones
  stay until they finish.
- A journal error while retiring is returned to the caller as it is. The retirement it hit
  rolled back, so the journal and the broker's memory still agree, and the broker is not
  faulted: running jobs keep running.
- On restart, an operation that had not settled comes back as `unknown`. It is never run
  again and no remembered PID is signalled. `unknown` is not proof that its effects did not
  happen.

## Boundaries still open

- `run` makes a new key for every call, so repeating a `run` can repeat its effects. Retry
  with `operation_start` and a key the caller keeps.
- A hub writes its audit row after it dispatches; an audit failure after admission can
  leave the caller unsure. This contract does not change that order.
- Keys are scoped to one broker's journal and are not authenticated per caller beyond the
  account check. Grants are not a sandbox between clients of the same OS account.
- File writes, edits and moves are not retry-keyed operations.
