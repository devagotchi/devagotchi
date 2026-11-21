# A03 HTTP source mapping and authoring

Pinned S1: 46419c697964016eb547c60416a8f2f5e854a95f. Read httpapi/server.go,
tasks.go, admin.go, events.go, protocol.go and export.go, including dynamic config routes.
This packet is local authoring only; no CI, push or source closure.

## Task and lease adapter

Keep source route names: GET/POST /api/v1/tasks, GET/PATCH /api/v1/tasks/{id},
POST /api/v1/claim-next and POST /api/v1/tasks/{id}/{op}. Expand op to claim,
heartbeat, release, complete, handoff, force-release, notes and deps. Unknown operations
refuse. Existing /api/v1/me stays with identity. Rust IDs are the existing string ticket
IDs; never reinterpret them as source integers. Require an explicit project, including
reads: a global source task list cannot bypass the current board access boundary.

Own HTTP adapters under crates/hub/src/agenthub/http/. Mount behind the existing
bearer_or_session middleware (live bearer verification or browser session plus origin
check). Capture Subject and KeyLimits from request extensions, never JSON. Check key scope
and project with the equivalent MCP operation, waiting state and valid seat. Reuse the
existing scoped board call for grant/role/profile/WIP identity, then invoke Board methods;
no new SQL, request-context forgery or loopback MCP connection. Record real completion.

Use typed mutation schemas already shared by MCP, including optional progress with explicit
null distinct from omission. REST has no remembered MCP fence or session; a held write must
supply its fence. Map source ttl_seconds to ttl_ms with checked conversion; reject conflicting
fields. Resolve path/query identifiers and If-Match before hashing. A supplied version must
agree with If-Match; malformed/weak tags refuse rather than silently selecting version zero.
Reject disagreeing Idempotency-Key and request_id rather than silently ignoring either.
Bound JSON to 1 MiB and reject unknown fields through the typed schema.

Receipt input is typed and canonical before implicit actor defaults, with the same
operation names as MCP. The authenticated principal owns it. Replay returns the saved
result, request_id/replayed metadata and Idempotent-Replayed: true, with no fresh mutation
or wake. Domain policy still runs on every attempt. Mark responses no-store. HTTP errors
use the source error envelope, stable codes and useful status: invalid 400, access 403,
missing 404, conflict 409, retired 410, precondition mismatch 412, oversized 413, unavailable
503. Never expose database failure details.

List retains tasks and board-scoped head_seq/server_now; detail retains task, notes,
depends_on and events while using the current ticket types. Reads have bounded limits.
Claim-next returns claimed plus its optional lease. Other result shapes use current typed
domain records; document this pre-release migration instead of claiming wire identity.

## Administrative work and transport status

Principal/role/policy/settings HTTP adapters and the environment agent roster are authored
locally; exact source mapping and validation are in [a03-principals.md](a03-principals.md).
The remaining aggregate/project/workflow administration below is still open.

- GET /api/v1/config; principals list/create/put/token metadata/issue/revoke. Map to account
  ownership, live manager policy and the authored principal/catalog APIs. Never replay
  token issuance or expose global identity lists to ordinary agents.
- Expanded configuration kinds: roles, policies, workflows, projects and settings, each
  POST and PUT; DELETE for all except settings. Project delete, arbitrary settings and
  cross-owner catalogs need their actual domain authoring before advertising the routes.
- Authored on 2026-11-21: board-scoped SSE resumes after Last-Event-ID, subscribes before
  catch-up, uses bounded pages/backpressure and rechecks live authority. Five new transport
  tests and the six existing HTTP tests passed; see a03-review.md for remaining acceptance.
- Authored protocol/connect output uses current CLI/MCP names, board heartbeat settings
  and the existing connector catalog, never a stored secret. Dynamic Devagotchi Extra
  routes are separately owned by A04-A08.

## Focused evidence required

Real HTTP create/replay, changed-body/cross-operation conflict, duplicate concurrent
requests, stale If-Match and fences, null-vs-omitted progress, old release after newer claim,
one event per mutation, malformed/oversized input. Current key scope/project restriction,
revocation, foreign-account reads/writes and browser cross-origin refusals must hold on
replay too. Compare REST and MCP persisted state and cross-transport receipt identity.
No source row closes until all mapped behavior is verified and delivered to main.
