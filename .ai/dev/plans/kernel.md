# The kernel: Devagotchi's AI code in one crate, with a working lexicon and dictionary

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-16, shippin channel: all of Devagotchi's AI-specific code
  under one kernel; the lexicon and dictionary "full researched and implemented"; when the person
  has Clyffy and Devagotchi, Clyffy takes over the lexicon and dictionary, the two are merged into
  one clear source of truth, and Devagotchi restores them when the person leaves Clyffy. Not small
  PRs: complete, working pieces, pushed at least hourly.
- PR: none (the operator, Nov 16: no PR ceremony; land on main as each stage works)
- Branch / baseline: `increment-178/kernel` from `main` at `5104f35`; K1 carried onto
  `increment-179/desktop-commander-parity` and main after Desktop Commander parity P9
- Objective: `engine/kernel` is the one home for Devagotchi's AI code, one domain per folder; the
  lexicon and dictionary work end to end (resolve, propose, confirm, dismiss, retire, revisions,
  events), stored in the hub; the Devagotchi half of the Clyffy handoff (export, merge, authority
  with fencing, restore) is built and tested against a bundle Clyffy can produce.
- Non-goals: changes in Clyffy, RRFlow or AgentHub (`agenthub` is owned by another
  session); running a model; copying LFG's question bank.
- Canonical architecture / related plan: [layout pattern](../../patterns/layout.md),
  [AI development capture](ai-development-capture.md), [structure cleanup](structure-cleanup.md)
  S10 (superseded by K1), [AgentHub factory](agenthub-factory.md).

## Layout

```
engine/kernel/src/
  lib.rs          wiring only
  capture/        `devagotchi ai`: policy, harness hooks, capture journal, delivery status
  dictionary/     concepts: canonical label, aliases, informal and retired names, terms,
                  stack markers, disambiguation rules; the shipped seed (seed.json)
  lexicon/        phrases a person or project uses, their concepts, scopes, states,
                  revisions, events and receipts; the `Store` trait; the resolver and scanner
  words.rs        normalization and word splitting both share
  handoff/        the Clyffy bundle, merge, and the authority state with its epoch fence
```

The kernel opens no network connection and owns no database driver for the hub. Storage is a
trait the kernel defines (`lexicon::Store`); the hub implements it on its Postgres schema with
`kernel_` tables, and any other host (shippin.ai's hub, a standalone AgentHub) implements the
same trait on its own database. That keeps the kernel public and host-neutral.

## Milestones

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| K1 | Move machine-engine `src/ai/` to `engine/kernel/src/capture/`; `devagotchi ai` unchanged; ratchet entries move with their files | none | `git diff -M` renames; kernel 18 tests, `ai_workflow`, `one_binary`, `layout` green | done |
| K2 | `dictionary` and `lexicon` domains: model, scopes, states (proposed, confirmed, dismissed, retired, inconclusive), CAS revisions, events, resolver, shipped seed; in-memory `Store` | K1 | resolver tests on the seed and the disambiguation rows | done |
| K3 | Hub hosting: `kernel_` tables, Postgres `Store`, MCP tools `lexicon_resolve`, `lexicon_propose`, `lexicon_review`, `dictionary_get` | K2 | store tests on Postgres; tool tests | done |
| K4 | `handoff`: `LexiconBundleV1`, merge engine, authority (devagotchi, handing_over, clyffy, restoring) with an epoch fence, restore from the mirror | K2 | property tests: merge order-independent for non-conflicts; confirmed-vs-confirmed differences become conflicts; an old epoch's write is refused | done |
| K5 | Hub keeps each owner's authority (`kernel_lexicon_authority`), fences the lexicon tools' writes by it, and serves the handover, mirror and restore steps (`lexicon_handoff`) | K3, K4 | Postgres tests: a write after `begin` is refused; a conflict is previewed and blocks the export; the mirror follows Clyffy; a restore writes Clyffy's last bundle back | done |

## K2 decisions

- States: `proposed`, `confirmed`, `inconclusive` (parked, meaning may be empty until revised),
  `dismissed`, `retired` (still resolves, marked, after everything else), `deleted` (words
  erased from the entry, its events and its receipts). One live entry (proposed, confirmed,
  inconclusive) per normalized phrase per scope.
- Operations: propose, define (confirmed at once: the person's own words), park, revise,
  confirm, dismiss, retire, delete. Clyffy's create_proposal and create_confirmed map to
  propose and define in the handoff.
- Resolution order: project confirmed, personal confirmed, dictionary rule (cued, then the
  phrase's default), dictionary names by strength, retired entry, unknown. Ties between
  concepts go to the one the surrounding words name most, else ambiguous.
- Owner ids are any 1 to 128 of `[A-Za-z0-9_-.:@/]`, so a hub subject and a Clyffy UUID both fit.
- `Store` uses `impl Future` methods (no async-trait dependency); `MemoryStore` is the
  reference the Postgres store will be tested against.

## K3 decisions

- Tables `kernel_lexicon_entries`, `kernel_lexicon_events`, `kernel_lexicon_receipts`
  (migration 0032), domain prefix `kernel_` added to layout rule 8 and its test. Hub-wide and
  keyed by owner like `machine_devices`: a subject may have no tenant, so every query names the
  owner rather than relying on a tenant's row-level security.
- The database also holds the two rules a race could break: `SELECT … FOR UPDATE` on the
  entry for its revision, and a partial unique index for one live phrase per scope (a lost
  race retries and comes back as a conflict).
- Owner is the signed-in subject's id; an id the lexicon cannot key is keyed by its SHA-256.
- Tools: `dictionary_get`, `lexicon_resolve` (phrase or text), `lexicon_propose` (`start`:
  proposed, inconclusive, or confirmed only on the person's own words), `lexicon_review`
  (list, history, confirm, dismiss, retire, delete, revise). Writes are recorded in the audit
  chain. API keys get two new scopes, `words.read` and `words.write`.

## K4 decisions

- The bundle lives in `devagotchi_kernel::handoff`, not `devagotchi-contracts`: the kernel
  already depends on contracts, and the bundle carries the kernel's own `Entry` types.
- A bundle entry is Clyffy's entry field for field (`owner_id`, `indexed_revision` carried),
  plus `concepts` and the states `inconclusive` and `retired`, both optional on read.
- Merge: one copy per id (highest revision, then latest change; the same revision and time
  with other content is a conflict); one live entry per normalized phrase per scope; two
  confirmed meanings (meaning normalized plus concept set) are never chosen between: all go
  to the person as a conflict. Equal confirmed meanings keep the first confirmed and union
  the examples. Copies are ordered by their whole content, so the result does not depend on
  which side or order they came in.
- Authority: every move but a completion raises the epoch; Clyffy's bundle is taken in only
  while Clyffy holds the lexicon (that epoch) or while it is restored (that epoch or the one
  before the restore began).

## K5 decisions

- One tool, `lexicon_handoff`, with steps status, preview, begin, export, complete, abort,
  mirror and restore, so Clyffy (signed in through this hub as the person) needs nothing
  else. `status` gives the owner id a bundle must carry.
- The hub does the merge, once, at `export`: Clyffy sends its own lexicon if it had one, the
  hub keeps the merged result and returns it for Clyffy to adopt. Entries another replaced
  travel as dismissed (one revision on), so both sides keep every id. A conflict (two
  confirmed meanings) refuses the export: `preview` lists them before `begin`, and `abort`
  gives the lexicon back so the person can settle them here.
- While Clyffy holds it, Clyffy is the one source of truth: `mirror` replaces this hub's
  read-only copy with Clyffy's bundle (no merge). `restore` does the same with Clyffy's last
  bundle, or keeps the copy when Clyffy cannot send one, and this hub writes again.
- Fence: every lexicon write takes the owner's authority row `FOR SHARE` in its transaction;
  every move takes it `FOR UPDATE` and checks it against what it read. An entry that leaves
  the hub's copy, or arrives deleted, has its words erased from its events and receipts.

## Research behind the model

- Concept scheme: W3C SKOS Reference (2009): one `prefLabel` per concept, any number of
  `altLabel` and `hiddenLabel`, `broader`, `narrower`, `related`, `exactMatch`.
- Entry, sense, concept: W3C OntoLex-Lemon (2016): a lexical entry has senses, each sense
  references one concept; a phrase with two senses is ambiguous until context picks one.
- Thesaurus practice: ISO 25964-1:2011: preferred term, non-preferred terms pointing to it,
  retired terms kept so old text still resolves.
- Clyffy's lexicon (clyffy_alpha `4e578c9`, `interfaces/clyffyd/src/app/store/lexicon.rs`):
  personal and project scope, project overriding personal; proposed, confirmed, dismissed,
  deleted; revision compare-and-swap; receipts by request id; event history. The kernel keeps
  every one of those so a bundle round-trips, and adds a concept id per sense and the
  inconclusive state, which Clyffy lacks.
- Seed: Drive "Boundary Question Bank", Domain Boundaries v0.1 (canonical labels, seed
  aliases, disambiguation) and Stack Category Routing v0.2.

## Risks and approved decisions

- Single writer for the Clyffy handoff (one owner at a time, epoch-fenced, Devagotchi keeps a
  read-only mirror while Clyffy owns it): the operator asked for "one clear source of truth".
- Open PR #227 plans to split machine-engine's `ai/mod.rs` in S10; K1 moves it instead. The
  ratchet numbers are unchanged, so S10 becomes a split inside `engine/kernel/src/capture/`.
- The lexicon holds a person's own words: entries are theirs to edit and delete, and capture
  stays under DG-CAPTURE-1.

## Verification receipts

- K1, 2026-11-16, local: `cargo check -p devagotchi-kernel -p devagotchi-machine-engine -p
  devagotchi` clean; `cargo test -p devagotchi-kernel` 18 passed; `cargo test -p devagotchi
  --test layout --test ai_workflow --test one_binary` 7, 3 and 11 passed.
- K2, 2026-11-16, local: kernel 40 passed (18 capture, 22 dictionary, lexicon, words):
  seed load and refusals, stack markers with yields, state machine and refusals, CAS,
  receipts, owner isolation, delete erasing words from events and receipts, project over
  personal, every seed disambiguation row, context picking, retired fallback, text scanning
  with exact-case names. `layout` 7 passed.
- K1 on main after P9, 2026-11-16, local: kernel 18 passed, machine-engine 107 (its 18 `ai`
  tests now live in the kernel), `layout`, `ai_workflow`, `one_binary` 7, 3 and 11 passed.
- K3, 2026-11-16, local, Postgres 55432: `cargo test -p devagotchi-hub` 438 passed, among
  them the Postgres store telling the same story as `MemoryStore`, one live phrase holding
  when two writes race, and the tools keeping each person's words to themselves; `layout` 7;
  `generate-contracts --check` current; `tests/e2e.py` E2E PASSED with the four tools served.
- K4, 2026-11-16, local: kernel 55 passed, 15 of them the handoff's: merge cases, 2,000
  generated pairs of sides (symmetric, order-free, one live phrase, nothing dropped, a
  confirmed meaning only giving way to the same meaning; the property test fails when copies
  are ordered by less than their whole content), the authority's moves and fences, a
  Clyffy-written bundle read as it is, and a whole handover and restore.
- K5, 2026-11-16, local, Postgres 55432: kernel 57 passed; `cargo test -p devagotchi-hub`
  439 passed, among them a person's words going to Clyffy and back through
  `lexicon_handoff` (preview conflict, writes refused after `begin`, export refused on a
  conflict, merged export, mirror with a deletion erased from history, old-epoch bundles
  refused, another person untouched, restore); `layout` 7; `generate-contracts --check`
  current; `tests/e2e.py` E2E PASSED with `lexicon_handoff` served.

## Delivery

Implementation: K1 to K5 done. Validation: local receipts above (no CI by request).
Push: increment-179/desktop-commander-parity. PR: none. Main: K1 to K5 landed with this push.
**DELIVERY INCOMPLETE: Clyffy's side (calling `lexicon_handoff`) is Clyffy's repo, out of scope here**
