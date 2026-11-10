# DG-CAPTURE-1: private capture and evidence

## What actually runs

`devagotchi ai` is a native command in the existing hub library; it needs neither an
account, email, initialized board, network service nor model subscription. Repository
[adapters](adapters.md) invoke it with native JSON stdin. The canonical event type lives
in `crates/contracts/src/lib.rs`; `devagotchi ai schema` generates
[capture-event.schema.json](capture-event.schema.json). `ai check` detects schema,
stage and default-policy drift. Runtime validation enforces sizes/consent beyond schema.

Capture starts **off**. Merely pulling this repository never opts an operator into
recording content. From this checkout:

```text
devagotchi ai policy show
devagotchi ai policy set --operator local-owner --capture metadata
devagotchi ai capture status
```

Metadata allowlist: schema, event/session identity, harness, kind, optional model and
issue. Hook bodies, prompt/tool text, transcript paths, command arguments, working
directory, environment and arbitrary extra fields are NOT stored in metadata mode.
IDs/model/issue may themselves be identifying; do not put secrets in them.
Hook input is bounded to 1 MiB. No global provider history is scraped.

Content is a **separate informed consent**:

```text
devagotchi ai policy set --operator local-owner --capture content --acknowledge content-risk
```

This records raw hook bodies and explicitly supplied transcript records. They may contain
credentials, personal/confidential material, code and paths. No automatic redaction is
claimed. Use trusted OS storage and never redirect an export into the repository.
Opt out immediately with `--capture off`; this stops future writes, not past retention.
Returning to metadata revokes content acknowledgment. Re-enabling content requires it again.

## Transcript boundaries and incremental import

Hook lifecycle events are **partial** coverage, not a complete conversation. Hooks can be
skipped, denied, interrupted, unsupported or absent; subscription vs API model access
does not determine harness support. Every archive/export stays `coverage: partial`.

For Claude Code/Codex only, content-mode Stop/SessionEnd hooks can follow the native
`transcript_path` **only after** the operator separately authorizes an existing absolute
directory using `policy set --operator NAME --transcript-root ABSOLUTE_DIR`.
Add roots separately if multiple directories are needed; inspect `policy show`.
`--clear-transcript-roots` revokes automatic path access. Prefer the narrow project/session
directory, not the home directory or a global history root. Paths outside authorized roots
and final-component symlinks are refused explicitly; canonicalization prevents parent
symlink escapes. No shell is used to parse input or open transcripts.

Alternatively, explicitly import a selected local JSONL file:

```text
devagotchi ai capture import --harness codex --session SESSION --file ABSOLUTE_JSONL
```

This explicit command authorizes that file for that invocation. Provider transcript
formats are not stable contracts: each complete JSONL record is preserved opaquely with
its source harness/session, byte offset and SHA-256 of the exact source line, never
misrepresented as a normalized visible chat message. Imported `content.record` contains
the native JSON value; `content.source_sha256` binds its original source bytes.
Only newline-terminated records are admitted, so a live partial tail is retried later.
Limits: 64 MiB per file/import, 1 MiB per record/event. Imports re-read records to detect
changed-content replay; original path hash + byte offset gives stable record identity.
Prior valid records survive a malformed later record and retry without duplication.
Rewrites of already imported records conflict; use a distinct source/session for a new
trajectory, never silently replace evidence. Large imports should use the explicit command,
not assume they fit a provider's short shutdown budget. No inference of transcript
completeness, provider version, message roles, token counts or hidden reasoning.

For harnesses with no usable transcript path (including this Copilot adapter), authorized
hook payloads are captured and an explicit exported JSONL trajectory can be imported.
Do not claim automatic capture of assistant messages not present in the event.

## Durability, privacy and recovery

- Private journal: `<state>/ai/<sha256(canonical-checkout-path)>/journal.sqlite3`.
  `<state>` is the existing per-user platform state directory or absolute
  `DEVAGOTCHI_STATE_DIR`. Archive paths inside source checkouts are rejected.
  Symlink redirection of the archive directory/database is refused.
- SQLite here is a **local evidence journal**, not a hosted board database, synchronization
  service or authority fallback. No schema/migration is added to hosted Postgres.
- Immediate transactions, FULL synchronous writes and a bounded two-second lock wait
  serialize consent/capture and concurrent writers. Errors are explicit. A successfully
  acknowledged event survives process restart. No buffered shutdown-only archive.
- Canonical `capture append` requires a caller's stable event ID. Identical replay returns
  its original sequence; changed replay fails with no mutation. Native hooks have no
  universal invocation ID, so receive random IDs: repeated hook deliveries can duplicate.
- Entries chain the previous hash, capture time and event bytes. `capture verify` detects
  accidental mutation/reordering/interior deletion. This is not a signature, external
  witness, proof of authorship, or protection against same-user rewrites/tail deletion.
- Unix archive directories/files use 0700/0600. Windows uses the existing per-user state
  location and inherited ACLs; a custom state directory must have private ACLs. This
  implementation does not claim encryption or repair arbitrary Windows parent ACLs.
- Local policy changes are transactionally audited. Their operator label is self-asserted,
  not authenticated identity or an assessment of competence.
  Harness/model/session labels are also asserted provenance, not cryptographic proof
  that a particular provider or person authored a record.
- `capture export --after SEQUENCE --limit 100` emits ordered JSONL to stdout (max 1000 per
  page). Retain the last sequence and chain hash; protect the exported file yourself.
  `capture status` identifies the exact private database, mode and coverage.
- No automatic upload, cloud retention, production tenancy or scheduled deletion is implied.
  Retention defaults to explicit operator custody: use off for future writes; close active
  clients before an operator removes the exact archive shown by status and its own exports.
  Backups/exports remain the operator's responsibility. Do not delete shared state roots.

## Useful training data, without automatic training permission

Maintain three distinct layers:

1. **Restricted raw archive:** provenance-bearing visible chats, tool observations and
   operator-authorized provider records. Not a dataset and not committed.
2. **Reviewed engineering trajectory:** objective/issue, repository/base commit, plan and
   approved decisions, visible messages, action/observation IDs, patch/commit hashes,
   commands and exit codes, test reports/artifact hashes, review corrections, outcome and
   limitations. Use `capture append` checkpoint content to link these; prefer concise
   observed rationale over speculative narratives or requests for private chain-of-thought.
3. **Curated release:** explicit rights/consent review, secret/PII redaction with provenance,
   reviewer decisions, license/provider-term checks, acceptance evidence and dataset
   version/hash. Split by task/repository/time to avoid evaluation leakage.

Every runtime export has `training_eligible: false` and `redaction: not_performed`.
Nothing in this implementation automatically converts records into training examples.
No provider-specific fine-tuning service or universal license to reuse code/chats is assumed.
Use [evaluation definitions](evaluations.json) before making any data-quality claims.
