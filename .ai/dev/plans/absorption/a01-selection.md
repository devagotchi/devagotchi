# Task selection and reaper source comparison

Pinned S1 `46419c697964016eb547c60416a8f2f5e854a95f`, read Git objects for
`server/internal/store/{tasks,claim,sort,reaper,seed}.go`.

## Selection corrections authored

Source `Tasks` orders priority, creation time, id. Source `ClaimNext` prefers an
assignment to the caller, then priority/age/id, and applies all requested labels and
states in SQL before its 25-candidate limit. Current Rust previously used assignment
preference for ordinary lists as well, and applied states after taking 25 candidates.
It could report no eligible task while a later matching queue state was available.

`hosted/reads.rs::list_in` now applies the complete next-task selector before limiting;
assignment preference is only for actor-specific ready/claim selection. Labels are
normalized and intersected as the source does. Post-expiry state and live policy checks
remain in `hosted/claims.rs`. The source's policy candidate cap of 25 is retained; this
change does not claim an exhaustive search through arbitrary denied-policy candidates.

`NextFilter.labels` is passed through typed MCP/HTTP claim inputs and both CLI faces.
Singular `label` still combines with the list. Repeated CLI `--label` selects all values;
its last value remains in the original singular field and only preceding values go in
the new array. Empty arrays are omitted from canonical typed receipt serialization so
existing single-label requests do not acquire a different hash solely from a new field.

Authored regression scenarios: more than 25 earlier wrong states/partial label matches,
ordinary priority ordering versus assigned ready/claim ordering, normalized labels,
repeated local CLI labels with replay, and actual HTTP/MCP selection plus empty-field
retry equivalence. Focused validation passed with required PostgreSQL:

`cargo test --locked -p devagotchi-agenthub -p devagotchi-hub --lib -- selection search tests::lanes tests::lease_receipts tests::policy_enforcement cli::tests --test-threads=2`

44 AgentHub and 20 hub checks passed, zero failed/ignored; build 1m23s, execution
16.31s + 10.57s. Log: `target/absorption-task-selection.log`.

The resolver/reaper/policy/action follow-up checked paths affected by the preceding
lifecycle work without another rebuild:
`cargo test --locked -p devagotchi-agenthub -p devagotchi-hub --lib -- tests::hosted tests::actions policy::catalog::tests a_lapsed_lease_is_reaped --test-threads=2`

13 AgentHub and five hub checks passed in 3.26s + 3.17s, including persisted restart
reaping, listener reconnect, real deadline expiry, cross-instance state, default seed
preservation, atomic history and vocabulary. Log: `target/absorption-coordination-owners.log`.

## Deliberate current contracts retained

- Source query text combines PostgreSQL simple-dictionary web search, title substring,
  exact ID and notes. Existing `tests/search.rs` and hub search adapters cover these;
  Rust also searches literal description substrings and treats `%`, `_`, `\` literally
  rather than turning user input into SQL wildcard operators. Searches remain scoped
  to the resolved account/environment board, including note correlation by tenant/board.
- Actor-specific ready results allow lapsed leases so claim can recover them lazily.
  This extends the source's bare `ReadyOnly` list but follows its `ClaimNext` behavior.
  Assignment, blockers and distinct review constraints remain enforced. Board REST
  lists default to 100 and cap at 500; internal list defaults remain 200 rather than
  reproducing the source's implicit 500/1000-limit coercion.
- Reaping is AgentHub's existing transaction: database time, locked board, expire each
  lapsed holder, increment expiry count, release/attention transition, event/outbox.
  Hub scheduling uses persisted lease deadlines and notifications, not a second timer
  scheduler. Errors log and retry after five seconds. `Registry::reap_due` discovers
  persisted leases without previously opening a board. Existing acceptance lives in
  `tests/lanes.rs`, hub `agenthub/tests/hosted.rs` and `agenthub/mcp/tests/mod.rs`.
- The source reaper deletes old request identities by TTL. The implemented receipt
  owner intentionally retains permanent retired identities (A03); it never recreates
  an old mutation after pruning its response. A08 owns explicit retention/export.
- Source `Seed` inserts absent rows and seeds policies only when empty. Runtime catalog
  seed parity across all domains remains A05/A07; their later batches must not overwrite
  edited definitions. No source person's name or human bearer token becomes a public
  self-hosted default. Browser human authentication uses existing account login.

This ledger supplements A01 and A03; it is not closure for every behavior in these files.
