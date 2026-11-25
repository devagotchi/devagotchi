# Catalog execution detail

Part of Batch 2 in implementation.md. Source objects read at S1
`46419c697964016eb547c60416a8f2f5e854a95f`:
`server/internal/catalog/{kinds,crud,values,seed}.go`,
`server/internal/devapi/{catalog_routes,mcp_access}.go`,
`server/internal/db/migrations/003_devagotchi_catalog.sql`,
`server/internal/schemacheck/check.go`, `server/internal/presets/devagotchi.go`, and
capability/grant/router/chat preset data. Read the remaining preset files and catalog
browser definitions before completing the corresponding implementation.

## Concrete owner and files

- Add `engine/hub/src/catalog/` and register it in `lib.rs`. No table access from browser
  or MCP handlers. Closed `Kind` enum maps eight URL kinds to fixed `catalog_` tables and
  fixed columns. Use typed record validation, not arbitrary table or column payloads.
- Add `migrations/0039_catalogs.sql`: typed source columns, positive CAS version,
  `(tenant_id,id)` primary keys, same-environment reference keys, RLS and restricted-role
  grants. Add per-file seed marks. Source `tenants` catalog is intentionally replaced by
  the accepted environment/users edition design; do not add a tenants management page.
- `types/{mod,capabilities,presets,connections}.rs`: exact supported fields, defaults and
  unknown-field rejection; merged PATCH validation; source IDs permit dots and colons (e.g. fs.read:sees)
  and must not be passed through the board-slug validator. Version is metadata, never
  an editable arbitrary field. Default grant selection is a dedicated atomic operation.
- `stored.rs`, `mutations.rs`, `references.rs`: scope and locking, bounded lists, CAS,
  complete merged-record validation, reference guards and current authorization before
  receipt replay. Compose existing `devagotchi_store::receipts` directly in the same
  transaction. Record nonsecret catalog mutation audit through the existing hub audit
  owner; do not invent board events for environment configuration.
- Extend the identity account owner with a narrow current-account/current-owner check
  usable inside another domain's transaction. Do not query identity tables from catalog.
  All writes require the environment owner as a person plus the existing policy catalog;
  map catalog write action names explicitly into the current `admin.catalog.*` namespace.
  Authenticated reads remain scoped to the account's environment. Keys/seats cannot gain
  human write authority. Live identity/policy checks precede replay and reference reads.
- `validate.rs`/`schema.rs`: validate full merged rows, field bounds, enum values, typed
  argv, capability selector schema and instance, supported sandbox levels and referenced
  capabilities/risks. Updating a capability cannot invalidate existing preset selectors.
  A full-access preset always requires unsandboxed policy, including partial edits that
  clear only that field. No automatic grant or runtime effect results from editing data.
- `seed.rs` plus `catalog/presets/*.json`: import the eight pinned public files, once per
  file/environment in source dependency order. Preserve edited/deleted seeded rows.
  All source rows must pass the same validator; a failed row rolls back the file mark.
  Provider reference checks are completed with A06, not a fabricated provider registry.
- `agenthub/http/catalog.rs`: source GET/POST collection and PUT/PATCH/DELETE record routes for the eight
  explicit kinds; body/path/version agreement, exact origin for sessions, no-store,
  bounded reads and original request keys. No `/tenants` route. Source registered
  `capabilities_list` in access MCP; there was no generic catalog mutation MCP registration.
  Implement the actual read tool with its grants packet, not speculative admin tools.
- `web/app/catalog/` and localized `site/catalog_copy.rs`: reachable owner catalog pages,
  typed fields, stable retry keys, edited-draft retention, separate default selection and
  reference-aware deletion. Link from owner settings; ordinary users see only permitted
  capability descriptions through the access flow. Do not add raw implementation details
  to ordinary task flows.

## Schema dependency decision

Use the established Rust jsonschema validator, with default HTTP/file resolver features
disabled and `options().offline()`; unknown external references must refuse, not fetch.
Bound schema/record size and nesting before compile. Keep built-in meta-schema support;
do not implement a partial handwritten JSON Schema interpreter or regex-only validator.
Official library docs checked 2026-11-21: selected version 0.61.2, `offline` refuses
references absent from its registry. Verify selected crate/MSRV/features in Cargo before
authoring its wrapper. See [ValidationOptions](https://docs.rs/jsonschema/0.61.2/jsonschema/struct.ValidationOptions.html)
and [crate metadata](https://docs.rs/crate/jsonschema/0.61.2).

## Acceptance before local checkpoint

Real PostgreSQL: all eight types create/read/patch/CAS/replay/delete, concurrent writers,
same ID isolated environments with actual restricted-role RLS, unknown fields/kinds,
invalid references, duplicate default selection, seed reopen/edit/delete preservation,
invalid merged full-access patch, invalid/remote schema, capability narrowing versus
existing presets, references preserved on failure, audit/receipt atomicity. Real HTTP:
browser owner/user/agent/origin/version boundary and per-kind route reachability. Browser:
draft preservation, escaped values, navigation and default/refusal behavior. Source row
closure still depends on consumers and eventual main delivery; catalog CRUD alone does
not prove enforced machine access or sandboxing.

## Authored catalog unit and open consumers

The typed PostgreSQL catalog owner, migration, eight pinned preset files, HTTP routes,
owner browser editor/navigation, localized copy, scoped read key permission and focused
tests are authored. IDs preserve source dots and colons; the first database run caught
and corrected rejection of the source capability-risk IDs. All 77 preset rows pass the
same complete-record validation as edits. Initial focused run: 11 passed, including
seven new catalog scenarios and four existing tests matched by the catalog filter;
zero ignored. The follow-up guard/site/store/key checks are recorded below when complete.

Versioned mutations, nonsecret audit events and permanent request receipts commit in
one transaction. Browser lists round-trip as JSON so commas inside an element are not
split; refused edits retain their request key and escaped values. Current policy
families preserve source grouping (capability risks use catalog.risks, grant presets
use preset) under the existing admin.* namespace. Default selection uses admin.preset.default.

Remaining A05/A04 work: compose actual access request/grant references and policy checks,
expose the actual capabilities_list MCP in the access packet and finish request/grant
consumers. The default-selection policy/confirmation integration, including escalation
of an already selected default, is authored in A04 with validation recorded there. Provider references remain
A06. No generic catalog administration MCP existed in source. The tenant catalog remains
replaced by the accepted one-environment/users boundary. No visual browser acceptance,
full suite, CI, push, merge, source closure or branch deletion is claimed here.


## Local validation receipt

Final focused command: `cargo test --locked -p devagotchi-hub --lib -- catalog site::tests store::tests identity::accounts::api_keys:: --test-threads=2` with required isolated PostgreSQL.
36 passed, zero failed/ignored; build 1m24s, execution 16.24s. This includes catalog
CRUD/references/default guards, live authority/replay, browser draft/list round trips,
actual restricted-role RLS, seed preservation, audit atomicity, site loading/localization,
API-key scopes and hub migration/bootstrap. Log: `target/absorption-catalog.log`.
Eight layout checks passed. The inventory checker verifies all 100 commits and 907
file-change records; all remain open. All eight embedded preset files match their pinned
source bytes. Formatting and diff whitespace checks passed. No CI or remote mutation.
