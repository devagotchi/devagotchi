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
- Objective: `crates/kernel` is the one home for Devagotchi's AI code, one domain per folder; the
  lexicon and dictionary work end to end (resolve, propose, confirm, dismiss, retire, revisions,
  events), stored in the hub; the Devagotchi half of the Clyffy handoff (export, merge, authority
  with fencing, restore) is built and tested against a bundle Clyffy can produce.
- Non-goals: changes in Clyffy, RRFlow or AgentHub (`crates/agenthub` is owned by another
  session); running a model; copying LFG's question bank.
- Canonical architecture / related plan: [layout pattern](../../patterns/layout.md),
  [AI development capture](ai-development-capture.md), [structure cleanup](structure-cleanup.md)
  S10 (superseded by K1), [AgentHub factory](agenthub-factory.md).

## Layout

```
crates/kernel/src/
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
| K1 | Move machine-engine `src/ai/` to `crates/kernel/src/capture/`; `devagotchi ai` unchanged; ratchet entries move with their files | none | `git diff -M` renames; kernel 18 tests, `ai_workflow`, `one_binary`, `layout` green | done |
| K2 | `dictionary` and `lexicon` domains: model, scopes, states (proposed, confirmed, dismissed, retired, inconclusive), CAS revisions, events, resolver, shipped seed; in-memory `Store` | K1 | resolver tests on the seed and the disambiguation rows | done |
| K3 | Hub hosting: `kernel_` tables, Postgres `Store`, MCP tools `lexicon_resolve`, `lexicon_propose`, `lexicon_review`, `dictionary_get` | K2 | store tests on Postgres; tool tests | not started |
| K4 | `handoff`: `LexiconBundleV1` in contracts, merge engine, authority (devagotchi, handing_over, clyffy, restoring) with an epoch fence, restore from the mirror | K2 | property tests: merge order-independent for non-conflicts; confirmed-vs-confirmed differences become conflicts; an old epoch's write is refused | not started |

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
  ratchet numbers are unchanged, so S10 becomes a split inside `crates/kernel/src/capture/`.
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

## Delivery

Implementation: K1 and K2 done, K3 next. Validation: local receipts above (no CI by request).
Push: increment-179/desktop-commander-parity. PR: none. Main: K1 landed.
**DELIVERY INCOMPLETE: K3 and K4 remain**
