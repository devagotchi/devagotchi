# AgentHub issue lifecycle and WardenMesh product plan

**Status, 2026-11-09 UTC:** this is the initial strategy, not an implementation receipt.
Use [the implementation and release plan](implementation-and-release-plan.md) for the
verified starting point, canonical board integration, client capability limits, dependencies,
acceptance gates and validation cadence. The user selected self-hosted completion before
commercial hosting; the private commercial repository does not exist yet.

## Executive summary

The repo already points in the right strategic direction: Devagotchi is intended to improve on Desktop Commander with a local broker, a native inspector, and a future task-graph / mesh architecture. The concrete gap is that the project still has no real GitHub issue-triggered agent lifecycle, no shared state layer that works across AI vendors, and no market fork strategy for a commercial hosted version.

The minimum viable version is not a special-case "AI for GitHub issues" feature. It is a general issue-and-task lifecycle hub that:

- accepts GitHub issue webhooks and issue events as a trusted source of truth,
- creates or resumes a shared AgentHub state record at issue open,
- gives every AI client a consistent view of that state for the life of the issue,
- persists advice, ownership, decisions, and artifacts in a shared graph,
- closes the issue with a consistent summary, evidence ledger, and handoff state,
- works across frontier and subscription AI clients through a provider-neutral hub contract,
- and can be hosted in a self-hosted open-source Docker image while the commercial product uses the same core with SaaS auth and cloud hosting.

This design matches the real-world GitHub webhook model and the repo's own graph-first direction.

## What the repo already tells us

The docs already establish the core product intent and constraints:

- [docs/product-and-platform-plan.md](product-and-platform-plan.md) says Devagotchi should grow from a local MCP bridge into an internal mesh with every admitted task represented as a graph.
- [docs/desktop-commander-user-evidence.md](desktop-commander-user-evidence.md) documents the actual pain points that Desktop Commander users experience: startup friction, ownership and diagnostic gaps, session churn, and host-environment differences.
- [docs/performance-and-experience-spec.md](performance-and-experience-spec.md) explicitly says there is no comparative performance measurement yet and proposes benchmarking before claiming superiority.

This is the right starting point. The missing work is operationalizing the issue lifecycle and state-sharing layer into an actual product flow.

## Real-world research: how issue lifecycle triggers should work

GitHub's official webhook docs establish the correct source-of-truth pattern for issue lifecycle tracking. The `issues` event fires when a repository issue is created, edited, opened, closed, reopened, assigned, labeled, reassigned, or otherwise changed. The payload includes:

- `action`
- `issue`
- `repository`
- `sender`
- and the standard GitHub event headers such as `X-GitHub-Event`, `X-GitHub-Delivery`, and `X-Hub-Signature-256`.

The important design implication is simple: the issue start and issue end are not guessed from UI state; they are authoritative GitHub events. We should treat `issues`/`issue_comment` and optionally `pull_request` events as the canonical lifecycle source, then route them into AgentHub.

The correct hook model is:

1. GitHub receives issue lifecycle event.
2. GitHub sends signed webhook to the AgentHub ingress.
3. AgentHub validates the signature and idempotency key.
4. AgentHub creates or updates a canonical task record for the issue.
5. AgentHub resolves the right AI session, context, or backlog record.
6. AgentHub writes a state snapshot that all supported AI clients can read.
7. AgentHub emits a start-of-issue or end-of-issue event to providers, even when they are different vendors.

This is materially stronger than a UI-only trigger. It works no matter whether the AI is a frontier model, a cheaper subscription tier, or a self-hosted local agent.

## Product requirement: AgentHub must be provider-neutral

The requirement is not "Claude can access AgentHub" or "GPT can access AgentHub." The requirement is: every supported AI connection, regardless of model vendor, can read and write the same shared state.

That means AgentHub must have three layers:

- Identity/auth layer: repo, user, machine, and session identity.
- State/graph layer: issue, task, milestone, artifact, suggestion, recommendation, ownership, and memory records.
- Provider adapter layer: OpenAI-compatible, Anthropic-compatible, local model runner, or custom MCP bridge.

The adapter layer is where the product becomes competitive with Desktop Commander. A Local Composer, Desktop Commander, or VS Code-style agent can all share the same normalized state object while model-specific prompts and tool usage remain behind a provider adapter.

## Core architecture proposal

### 1. Inbound lifecycle ingress

Create a dedicated GitHub connector with these event handlers:

- `issues` → `opened`, `reopened`, `edited`, `closed`, `locked`, `unlocked`
- `issue_comment` → `created`, `edited`, `deleted`
- `pull_request` → `opened`, `closed`, `ready_for_review`, `converted_to_draft`
- `workflow_run` or custom repository automation as optional telemetry sources

Use a single ingress service behind a signed webhook URL. The connector should validate:

- `X-Hub-Signature-256`
- repository allowlist
- installation / app auth
- replay protection via `X-GitHub-Delivery`
- idempotent processing by event ID and action

The event should map into a canonical task record like:

```json
{
  "task_id": "issue-123",
  "source": "github",
  "source_id": "repo/owner#123",
  "status": "opened",
  "title": "...",
  "body": "...",
  "labels": ["bug", "needs-repro"],
  "assignees": ["user-a"],
  "project": "devagotchi",
  "created_at": "...",
  "updated_at": "...",
  "state": "active"
}
```

### 2. AgentHub session bootstrap at issue start

On issue open or issue reopened, AgentHub should automatically create or resume a session context.

The session should include:

- issue metadata
- repo metadata and default branch
- relevant prior issue history
- current ownership and active AI agent assignment
- state snapshot from earlier sessions if any
- a working state object for the agent

This is the correct "start-of-issue" hook. It should be mandatory and system-level, not optional in the provider UI.

### 3. State-sharing model

This is the part most people get wrong: generative AI does not produce robust shared execution unless the system persists structured state, not just chat transcripts.

AgentHub should persist a shared ledger with:

- `task graph`
- `decision log`
- `recommendations`
- `artifact references`
- `handoff notes`
- `ownership / permissions`
- `readiness status`
- `timeline events`

This is exactly the same conceptual direction the repo explores in its task-graph and mesh docs.

### 4. End-of-issue closure and handoff summary

At issue close, resolve and summarize the issue into a durable record:

- root cause summary
- changed files and patch references
- validation evidence
- open follow-up tasks
- user-facing summary
- AI advice or rationale retained for future work

This should be emitted to the issue comment or to a structured internal state channel, depending on repo policy and user consent.

The end-of-issue event is not just a closing note. It is the system memory preservation step.

## AI collaboration model: share advice and state, not just prompts

The real winning pattern in AI collaboration is not allowing every model to "chat with the same prompt." It is letting models share a structured memory that all agents understand.

Use a model-neutral collaboration schema such as:

- `agent_id`
- `agent_role`
- `model_provider`
- `model_name`
- `task_id`
- `context_hash`
- `recommendation`
- `confidence`
- `evidence_refs`
- `decision_accepted`
- `shared_state_version`

This lets a frontier model and a subscription model both contribute without locking the system to one vendor or one UI.

The UI should expose this as a live shared board: task health, top recommendations, unresolved questions, and state version. That is the main user benefit and the clear difference from a basic Desktop Commander clone.

## Desktop Commander parity and competitive gap

To compete with Desktop Commander, Devagotchi must not only match file execution. It must exceed it in three ways:

1. Shared state that survives sessions and reconnects.
2. Multi-agent collaboration and review rather than one isolated executor.
3. Structured issue lifecycle coupling with GitHub and repo workflows.

Desktop Commander is strong around local command execution and UI visibility. Devagotchi must be stronger in:

- durable task state
- agent-to-agent handoff
- issue lifecycle automation
- workflow memory across models and sessions
- hosted/cloud capability without losing self-hosted control

This is consistent with the repo's explicit goal of becoming a trusted mesh and graph-based platform.

## WardenMesh commercial fork strategy

The right strategy is not an unrelated commercial product. It is a clean fork and productization plan with shared core.

### Open-source path

- `devagotchi` remains the open-source self-hosted core.
- It provides the docker image, local broker, MCP adapters, issue webhook connector, and self-hosted deployment.
- It is the equivalent of a local or private desktop + docker installation that can compete with Desktop Commander, but with better state sharing and issue lifecycle workflows.

### Commercial cloud path

- `WardenMesh.com` is the commercial SaaS fork of the same product.
- It adds login, billing, multi-tenant orchestration, hosted AgentHub state, cloud backups, enterprise policies, and premium support.
- It reuses the same contract and state model while layering a cloud UX on top.

The repo should be organized so that the OSS core is the foundation and the commercial fork is a thin operational overlay.

## Recommended architecture split

### Core repo (Devagotchi OSS)

- local broker
- MCP server/connector
- GitHub webhook ingress
- issue lifecycle event processor
- task graph state store
- shared state API
- provider adapters
- docker deployment for self-hosting

### Commercial repo (WardenMesh)

- hosted AgentHub control plane
- cloud task execution
- auth / tenancy / RBAC
- billing and usage analytics
- hosted connectors / premium integrations
- enterprise policy and audit trail
- SaaS UI

This is a healthy product split: open-source trust + commercial platform value.

## Benchmark design: the benchmark that actually matters

The repo already makes the right point: there is no real comparative benchmark yet. We need a benchmark that measures the actual product value, not a generic shell latency score.

### Benchmark categories

1. Startup and readiness
   - cold start time from disconnected state to first usable AI session
   - warm reconnect time
   - ability to recover when optional services are unavailable

2. Issue lifecycle performance
   - time from GitHub issue opened to AgentHub session active
   - time from close event to agent memory persisted
   - end-of-issue summary latency

3. State-sharing quality
   - percentage of relevant state correctly carried across agents
   - number of missing or stale state transitions
   - conflict resolution correctness

4. Delegation and handoff quality
   - issues resolved when handed from one AI to another
   - whether advice is preserved across model/vendor boundaries

5. Workspace execution and control
   - command execution latency
   - cancellation correctness
   - file operation and output traceability
   - permission recovery

### Benchmark outputs

Report:

- p50, p95, p99 for startup and issue events
- success rate under normal and degraded network conditions
- stale-state rate
- context-loss rate when switching AI providers
- time to handoff
- action completion correctness

This is much more relevant than only benchmarking raw command launches.

## Implementation roadmap

### Phase 1: issue lifecycle foundation

- implement GitHub webhook validation
- persist issue records and event stream
- create issue start and issue close actions
- create canonical task state record
- build event idempotency and replay protection

### Phase 2: AgentHub state and multi-provider access

- add shared state store
- add provider adapter abstraction
- implement read/write session contract
- optimize shared memory and handoff state
- expose agent recommendations and decision ledger

### Phase 3: parity with Desktop Commander

- live operation logs
- session ownership clarity
- command output persistence
- cancellation and recovery UX
- explicit host diagnostics

### Phase 4: WardenMesh commercialization

- fork cloud hosting controls
- platform auth and tenant model
- billing and quota enforcement
- audit, backups, and scaling

### Phase 5: validation and benchmark publication

- run the benchmark against Desktop Commander in controlled conditions
- publish methodology and results
- state the exact hardware, OS, and load conditions
- keep benchmark honest and reproducible

## Recommended acceptance gates

The product is good to move forward when all of the following are true:

- Every issue open and issue close produces a trusted AgentHub state record.
- Every supported provider can read/write the same state object.
- At least one real multi-provider schedule works without state loss.
- A self-hosted Docker install can run without cloud hosting.
- The hosted commercial fork adds value without altering the open-source contract.
- A benchmark exists that compares start, issue flow, state sharing, and handoff against Desktop Commander under realistic loads.

## Immediate next move

The next step should be a focused implementation sprint that does not wait for full product perfection:

1. Build the GitHub issue webhook ingress and signed validation.
2. Add the canonical task record and start/close triggers.
3. Add the shared state service and provider adapter abstraction.
4. Write a benchmark harness for issue-open / issue-close / handoff / output persistence.
5. Fork the production host plan for WardenMesh.com after the core is stable.

That sequence keeps the product honest and moves toward a real competitor to Desktop Commander rather than a speculative concept.

## Final recommendation

The core product should be framed as:

- Devagotchi = open-source self-hosted workspace bridge + AgentHub + issue lifecycle + local desktop experience
- WardenMesh = commercial cloud-hosted fork for enterprise SaaS and managed deployment

The crucial architectural rule is: the AI must not rely on vendor-specific chat memory. It must rely on a durable shared task graph and shared state service that survives issue opening, issue closure, and provider switching.

That is the real path to competing with Desktop Commander and building a credible commercial platform.
