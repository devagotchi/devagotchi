# AgentHub benchmark plan

**Status, 2026-11-09 UTC:** proposed executable acceptance specification; no comparative
results are claimed. Delivery is P07 in [the implementation and release plan](implementation-and-release-plan.md).

## Goal

Measure the real gap between a normal desktop-agent workflow and the AgentHub state-sharing model. The benchmark must prove whether the hub makes issue startup, state continuity, handoff quality, and issue close materially better than a plain desktop commander-style flow.

## Core benchmark dimensions

1. Startup readiness
   - Cold start of the hub and broker from a fresh state directory.
   - Time to register issue webhook handler, issue store, provider state, and route availability.

2. Issue lifecycle latency
   - Time from webhook receipt to issue record creation.
   - Time from issue open to readable AgentHub state record.
   - Time from issue close to summary and handoff state persistence.

3. State continuity across providers
   - Same issue state read by multiple provider models without rehydration drift.
   - Shared version increments and decision ledger are preserved across sequential provider sessions.

4. Handoff quality
   - A second agent can read the previous provider's recommendation and resume with a consistent context summary.
   - Evidence references remain valid and human-readable.

5. Workspace execution under load
   - Multiple issues open simultaneously while agent sessions keep continuity.
   - Recovery after cancellation or reconnect without losing the current issue state snapshot.

## Benchmark harness design

The harness should run in a temporary repo fixture with a seeded GitHub issue payload, then perform these steps:

- create issue via signed webhook payload
- verify the issue is stored and the route returns the canonical state
- create provider-neutral advice state for the issue
- read the state from a second provider session
- close the issue and confirm the final summary/handoff state is persisted
- record cold/warm latency, event version number, and decision ledger size

## Minimum acceptance thresholds

These are proposed thresholds for a real product comparison, not claimed current metrics.

- issue open to state available: under 500 ms on local loopback fixture
- issue close to state available: under 800 ms on local loopback fixture
- provider state round trip: under 150 ms for repeated reads
- state version monotonicity: never decreases across provider updates
- decision ledger retains at least the last 25 recommendations for a given issue

Measure the latency targets as p95 on the declared local fixture cohort, not as the
single fastest request. Keep source-lifecycle and board/shared revisions separate.
Correctness requires zero duplicate effects, zero silent advice loss, rejected stale
writes and no cross-account/project access. A fixture failure fails acceptance even if
successful-request latency meets its threshold.

## Required harness output and modes

Extend the existing board/e2e probe infrastructure rather than introducing a second
test framework. The harness must offer distinct correctness, performance and comparator
profiles so routine code changes do not launch long soaks or real paid-model sessions.

Each run emits a JSON manifest, raw per-attempt observations, correctness outcomes and
a summary with source/binary/image identity, OS/CPU/RAM/storage, client/runtime versions,
seed/configuration, sample counts, failures/timeouts, p50/p95 and threshold pass/fail.
Report p99 only with at least 1000 successful observations, following the
[performance specification](performance-and-experience-spec.md). Missing or unavailable
comparators/providers are `not_run`/`blocked`, not zeros or passing comparisons.

Required scenarios:

- duplicate/redelivered open and close events produce one effect;
- delayed/ambiguous source updates cannot silently roll current state backward;
- two authenticated seats retain at least 25 decisions across handoff and reconnect;
- simultaneous summary writes reject the stale expected version;
- a crash between inbox receipt, board mutation and completion marking recovers safely;
- unauthorized account/project reads and writes are denied;
- a source close preserves evidence and explicitly reports incomplete finalization;
- backup/restore retains mappings, source history, board evidence and chain validity.

Scripted subjects measure protocol continuity only. A separate real-client cohort must
verify hook execution, subscription-plan capability, recovered evidence and handoff outcome;
it cannot infer model behavior from provider names attached to synthetic records.

Desktop Commander comparisons time only matched execution/file/search/terminal/control
workloads. If it lacks issue lifecycle orchestration, report a feature gap rather than
assigning artificial latency. Pin both source and the installed stable artifact; a current
alpha source checkout is not evidence about the stable release.

## Comparison target

This should be run against both:

- the source-available Devagotchi self-hosted path
- a hosted WardenMesh cloud variant using the same contract

The benchmark report must call out differences in startup cost, tenant isolation, and multi-agent continuity. Only then can the product claim parity or superiority over Desktop Commander.
