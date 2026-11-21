# AgentHub task HTTP API

This pre-release API reads and changes the same PostgreSQL boards used by AgentHub's pages,
MCP tools and CLI. Each request needs a current bearer token, or a signed-in browser session.
Browser writes must send this hub's exact `Origin`. API-key scopes and project restrictions
apply to every request, including retries. A person's authority and an agent's authority
are checked by the existing board resolver and domain policy.

Every operation names a `project` the caller can reach. For reads, supply it in the query.
For writes, supply it in the JSON body or query; both must agree if both are present. Task
IDs are the existing string IDs returned by the board, not the old Go service's integers.

| Method and route | Behavior |
| --- | --- |
| GET `/api/v1/tasks?project=demo` | List tasks; optional `state`, `holder`, `label`, `q`, `ready=1`, `limit` (1–500) |
| POST `/api/v1/tasks` | Create a task; `project` and `title` required |
| GET `/api/v1/tasks/{id}?project=demo` | Task, notes, dependencies, children and latest 200 events |
| PATCH `/api/v1/tasks/{id}?project=demo` | Update metadata at the supplied `version` |
| POST `/api/v1/claim-next` | Claim a ready task; `project`, optional `label`, `states`, `parent`, `ttl_ms` |
| POST `/api/v1/tasks/{id}/claim?project=demo` | Claim this task; optional `ttl_ms` |
| POST `/api/v1/tasks/{id}/heartbeat?project=demo` | Renew the supplied `fence`; optional `progress` |
| POST `/api/v1/tasks/{id}/release?project=demo` | Release the supplied `fence`; optional `reason` |
| POST `/api/v1/tasks/{id}/complete?project=demo` | Complete the supplied `fence` with `summary` |
| POST `/api/v1/tasks/{id}/handoff?project=demo` | Hand off the supplied `fence` with `note` and optional `to` |
| POST `/api/v1/tasks/{id}/force-release?project=demo` | A person takes away a lease, with `reason` |
| POST `/api/v1/tasks/{id}/notes?project=demo` | Append `body`, optional `kind`/`to`; held work needs `fence` |
| POST `/api/v1/tasks/{id}/deps?project=demo` | Set `depends_on`, optional `kind`; `remove: true` removes it |
| GET `/api/v1/events?project=demo` | Page the selected board's committed history |
| GET `/api/v1/events/verify?project=demo` | Verify that board's event chain |

Creation and updates use the same typed fields as `board_create` and `board_update` in
the MCP tool schema. Creation supports `depends_on` as an array of string task IDs.
Updates use `clear_assignee: true` to remove an assignment. `progress` omitted keeps its
current value; `progress: null` clears it. REST never substitutes a remembered MCP fence
or connection session. Claim routes also accept the old `ttl_seconds` field, converted
with overflow checking; it must agree with `ttl_ms` if both are sent.

`If-Match: "VERSION"` can supply an update's version. A body version must agree with it.
A fresh stale `If-Match` returns 412. A stale body-only version returns 409. Weak tags and
lists of tags are rejected. Unknown fields, malformed JSON and conflicting path/body
identifiers are rejected. JSON bodies are limited to 1 MiB.

## Retry a change

Send `Idempotency-Key: KEY` or JSON `request_id`. If both are present, they must agree.
Keep the key and identical input when the result is uncertain. The key belongs to the
authenticated principal across operations and boards; changed input with the same key
returns `request_conflict`. Successful retries return the original result with
`replayed: true` and `Idempotent-Replayed: true`, without applying the change again.
Every attempt still needs current authority. Issuing credentials is not part of this API.

Typed task and lease inputs share receipt identity with the corresponding MCP mutations.
For example, retrying an HTTP creation through `board_create` with the same key and input
returns the original task. A replayed claim has its original fence and expiry; it does
not renew a lease or replace a later claim. A replayed empty claim-next remains empty.

Results keep their domain fields and add `request_id` when supplied and `replayed` for
mutations. Claim-next returns `claimed` and an optional `lease`; individual claims return
the lease directly. Dependencies are under `deps`. Errors use `{ "error": { "code": ...,
"message": ... } }`. Task-route responses are marked `Cache-Control: no-store`.

## Page history

Events accept `after_seq`, exclusive `before_seq`, `actor`, `action` (a prefix), `task_id`,
`order=asc|desc`, and `limit` (1–1000). Advance `after_seq` to the last event received when
paging forward. `ticket` and `action_prefix` are accepted aliases for the MCP field names.
Task lists return `tasks`, `head_seq` and `server_now` (Unix milliseconds). Their head is
captured before reading tasks so a reconnect cannot skip a concurrent committed event;
clients may see that change again when catching up. Event lists return `events` and a
board-scoped `head_seq`. No endpoint aggregates another account's history.

Project creation/deletion and aggregate configuration remain in the absorption
plan. The routes below are local prerelease authoring, not a published release.

## Principals and policy

These administration routes require a live person. API keys, hub-issued agent OAuth
tokens, bound agents and caller-selected seats cannot administer people or credentials.
Human accounts keep their existing `/admin/users` and password/passkey/provider login;
the principal routes manage registered nonhuman profiles. Setting an agent's `kind` to
`human` never changes its credential into a human identity.

| Method and route | Behavior |
| --- | --- |
| GET `/api/v1/principals` | Owner's environment roster; ordinary people see their own agents |
| POST `/api/v1/principals` | Create a profile with version 0 |
| GET/PUT `/api/v1/principals/{id}` | Read/edit a profile; PUT requires its positive version |
| GET `/api/v1/principals/{id}/tokens` | Metadata only, including revoked keys |
| POST `/api/v1/principals/{id}/tokens` | Issue a key bound to this registered agent; secret returned once |
| DELETE `/api/v1/tokens/{id}` | Revoke a key; supports a replay key |
| GET `/api/v1/principals/configuration` | Owner reads principal kinds/WIP, roles and policy rules |
| POST `/api/v1/roles` or `/api/v1/policies` | Owner creates a version-0 role or rule |
| PUT `/api/v1/roles/{id}` or `/api/v1/policies/{id}` | Owner updates the current positive version |
| DELETE `/api/v1/roles/{id}` or `/api/v1/policies/{id}` | Owner deletes with `If-Match`; assigned roles refuse |
| PUT `/api/v1/settings/principals` | Owner updates versioned principal kinds and default WIP |

Profile/key routes accept `account` in the query, defaulting to the caller for individual
operations. Only the environment owner may name another account. Roster reads accept
`role` and the returned `next` cursor as `after`; token pages accept their own returned
cursor. Both return at most 100 rows. Roster cursors qualify the profile ID with its account.
The browser's `/app/agents` uses this same roster and links roles to their members.

Profile fields are `id`, `name`, `kind`, `roles`, `enabled`, `wip_limit` and `version`.
Role fields are `id`, `description`, `version`. Rules and principal settings use the
records returned by the configuration route. Mutations return `result` and `replayed`;
replays also set `Idempotent-Replayed: true`. Current authority is checked before replay.
Path/body IDs and header/body versions must agree; stale domain versions return 409.

Token issuance takes `name`, `scopes`, `hours`, optional `projects` and `machines`, using
the existing API-key scope vocabulary and permitted durations. It rejects request keys
because the secret cannot be cached for replay. The response contains `info` and `token`;
later reads contain only metadata. An uncertain issuance is resolved by inspecting and
revoking the resulting key before explicitly issuing another. Responses are `no-store`.

## Workflow definitions and board configuration

These routes require a person who owns the selected account board and its complete
workflow catalog. Configured shared boards and organization namespaces retain their
existing configuration pages; a grant on one board does not disclose a whole catalog.

| Method and route | Behavior |
| --- | --- |
| GET `/api/v1/workflows?project=demo` | Shared workflow definitions in this account's catalog |
| POST `/api/v1/workflows?project=demo` | Create `{version: 0, workflow: {...}}` without binding the board |
| PUT `/api/v1/workflows/{id}?project=demo` | Edit `{version, workflow}` and atomically update every bound board |
| DELETE `/api/v1/workflows/{id}?project=demo` | Delete an unused definition with `If-Match` |
| GET `/api/v1/projects/{project}/configuration` | Settings, workflow, their independent versions, and current shared binding |
| PUT `/api/v1/projects/{project}/settings` | Save `{version, settings}` using the editable settings fields |
| PUT `/api/v1/projects/{project}/workflow` | Save `{version, workflow}` for an unbound board |
| POST `/api/v1/projects/{project}/workflow/bind` | Select `{version, workflow_id, workflow_version}` |
| POST `/api/v1/projects/{project}/workflow/detach` | Save `{version}` to retain an independent copy of the current workflow |

`version` on board routes is the matching `settings_version` or `workflow_version` from
the configuration response. On catalog routes it is the definition's `version`. Binding
checks both board and catalog versions. A shared definition cannot be removed while a
board uses it. Editing a definition validates its effect on all using boards before any
change commits. All writes support request keys and current-authority replay.

Editable settings are lease TTL/min/max, maximum lease expiries, default WIP/priority,
heartbeat hint, note kinds, priority labels, repo and environments. Standing approvals
have their own operations and cannot be inserted into the settings editor.

## Account project lifecycle

People manage their own projects through `/app/projects` or these API routes. Agent
credentials and seats cannot administer projects. One user's ownership does not grant
access to another user's project or archived history, including on a self-hosted hub.

| Method and route | Behavior |
| --- | --- |
| GET `/api/v1/projects` | Active projects; `retired=true` selects removed projects; `after` takes the returned cursor |
| POST `/api/v1/projects` | Create an empty project with `{id, name, version: 0}` |
| GET `/api/v1/projects/{project}` | Current project record and metadata version |
| PUT `/api/v1/projects/{project}` | Save `{id, name, version}`; the immutable address must agree with the path |
| DELETE `/api/v1/projects/{project}` | Remove the empty project with its positive version in `If-Match` |
| GET `/api/v1/projects/archive/{generation}/events` | Owner-only retained events, paged by `after_seq` |

Creation and editing accept paired `workflow_id` and `workflow_version` to select a
shared definition. Omit both to use a private workflow; detaching retains the effective
definition. Creation, selection, defaults, metadata, audit and retry receipt commit
together. Workflow changes also advance project metadata versions so a stale project
editor cannot undo a newer binding. Mutations return `{result, replayed}` and support
the same request identity across browser and HTTP.

Removal refuses any tasks (including completed ones), live project-scoped API keys,
or shared definitions whose audit history originates in that project. Revoke those keys
and remove unused definitions before removing the empty project. Account-wide keys
continue to cover future projects by their existing contract. Removed boards retain
history and receipts but cannot be used for ordinary reads or writes. Reusing an address
creates a different `board_id`; old handles and mutation keys cannot act on that new
generation. Archived records are addressed by their original `board_id`.

## Reconnecting events and protocol discovery

`GET /api/v1/stream?project=PROJECT` streams the same committed board events as the
events endpoint. Each frame has `event: hub`, its sequence as `id`, and the JSON event
as `data`. The initial retry hint is 3000 ms; idle connections receive a keepalive.
Use `Last-Event-ID` or `after_seq` to resume after the last received event. If both are
supplied they must agree. `from=now` starts at the current board head and cannot be
combined with a replay cursor. Negative, malformed, duplicate-header or future cursors
are refused before the stream opens. A project is always required.

The stream subscribes before its first history read, catches up in pages of at most
100 events, and periodically reads history even when no notification arrived. Its
credential, API-key limits, registered agent and board access are checked before
delivery, including delivery from a buffered page. Idle authority is checked every
five seconds. Revocation, lost board access, changed identity or a failed store/auth
check ends the stream; reconnect with current authority and the last received cursor.
Already delivered bytes cannot be withdrawn. Dropping a client drops its reader; there
is no detached producer or unbounded event queue. Responses are `no-store`, with proxy
buffering disabled. A mutation replay produces no new event.

`GET /api/v1/protocol?project=PROJECT` returns `agents_md`, the board's effective default
`heartbeat_hint_ms`, and `connect` with the existing connector catalog rendered for the
configured hub address. Actual claim responses determine the lease's heartbeat/expiry.
The catalog format is shared with `/api/v1/connectors`; this prerelease does not reproduce
the Go source's handwritten per-client snippets. No caller credential is returned.
