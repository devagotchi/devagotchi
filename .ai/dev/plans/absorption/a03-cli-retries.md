# A03 CLI retry implementation handoff

Source: S1 46419c697964016eb547c60416a8f2f5e854a95f, server/cmd/hub/{main,client,run,state}.go.
Author after the configuration/browser checkpoint; no CI, push or source closure.

The source generates cli-<random> for writes and accepts --request-id. Its run.go sends a
cached fence, then drops that cache on release/done/handoff. Replaying the same invocation
can therefore send fence 0 or a newer fence, or put an old claim response back into the
current cache. Port the request identity while correcting those behaviors.

## Exact implementation boundaries

1. cli/args.rs: add --request-id; distinguish single mutations from reads and batch
   commands. Single mutations are new, claim (including claim-next), beat ID, note, done,
   handoff, release, force, set, dep and settings with a patch. Reject a supplied key for
   reads, hooks, beat-all, scan/import and lifecycle operations whose single-result retry
   contract is not authored. Do not apply one key to multiple mutations.
2. cli/requests/ owns local retry preparation only, not mutation truth. PostgreSQL remains
   authoritative. Persist a bounded, versioned record of the user's mutation intent and
   the resolved CLI arguments (not credentials) before sending a mutation. Freeze implicit
   session and cached fence on first submission. A repeat restores the same arguments even
   after claim-cache deletion or a later claim. Changed user intent with the same key
   returns request_conflict; do not silently reuse old content or silently generate a key.
   Keep intent comparison separate from display flags and directory/account selection.
3. Bind the local record to hub/project/authenticated principal/acting seat, or the actual
   standalone store/project/OS actor. For a bound session use whoami.subject and optional
   whoami.agent/tenant, not caller-supplied account names. Hash namespaces/key for filenames.
   Never persist tokens, account.json, passwords or the full whoami response. Local records
   are ignored state under .devagotchi, never a second board database.
4. Lock each request record during preparation/dispatch; use atomic durable file creation.
   A malformed, partial, mismatched or unwritable record refuses before a remote mutation.
   Keep a request's original record after ambiguous transport failure and successful replay.
   Do not prune or overwrite records in a way that silently changes implicit defaults.
   Bound body/file size; use the repository Rust version's standard file-lock APIs, without
   unsafe code. Keep the lock's owning handle alive until dispatch finishes.
5. cli/mod.rs: capture raw intent before applying session defaults. Resolve the actor and
   board as today; read cache, prepare/recover the request, then dispatch the recovered
   arguments. The request owns the complete local Board::requested input and operation.
   For a bound Remote, add request_id to its one mutation's typed MCP body. Read helpers
   must not receive the key. Human output shows generated keys before dispatch. JSON keeps its one-result contract
   and includes the key in success/error output; the immutable local record also retains it
   if the process terminates. Include request_id and replayed in success/error
   output without dropping existing object result fields. Arrays need an explicit result
   member when metadata is present. Document this output contract.
6. face.rs: expose replay status for successful mutations (Board and Remote
   implement it). cli/commands/leases.rs must never replace/forget current claim cache on
   replay. Remote decodes the MCP replay flag and original result. A replay does not extend
   a lease, take another task, issue a wake or clear a newer fence. Keep explicit null
   progress distinct from omitted progress.
7. Keep existing unkeyed/internal helpers usable, and do not put retry state on a cached
   server Board. Browser/MCP request copies are already local to one call. CLI durable
   preparation is a transport journal only; current server authorization still runs on
   every attempt.

## Required focused evidence

- A standalone PostgreSQL CLI create invoked twice with one key has one task/event and
  original response; changed input or operation refuses. Explicit null and omission differ.
- Claim, release, claim again, retry the old release: current lease/cache survive. Retry
  the old claim: original fence is reported as replay and does not replace the new cache.
- Retry after restart with a different DEVAGOTCHI_SESSION or missing claim cache sends the
  original resolved inputs. A changed explicit user option conflicts, not silent replay.
- Two concurrent identical CLI invocations use one immutable prepared request and one
  server mutation. Torn/unwritable preparation refuses before mutation.
- Bound CLI over the real hub repeats the same cases across MCP sessions; live revocation
  refuses replay. Request records contain no key/OAuth secrets and cannot cross principals.
- Read/batch misuse is a usage error; normal JSON consumers retain existing object fields.
- Record exact paths/tests in a03-review.md; all source rows stay open pending full S1-S5
  authoring, later validation and verified main delivery.

## Review findings during authoring

`note`, `set` and the required-lease commands must not fall back to today's cache after
preparation. A missing original fence is part of the saved input, just as a present one
is. `commands::optional_fence` therefore only uses an explicitly prepared fence on keyed
invocations. A later claim cannot turn a previously unfenced request into a different one.

Capability negotiation is explicit: `whoami.board_request_receipts == true` is required
before a bound write, and its response must echo the key and a boolean replay indicator.
Otherwise an older hub could ignore an unknown field and repeat an unprotected mutation.
