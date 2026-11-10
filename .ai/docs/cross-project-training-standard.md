# Cross-project AI engineering data standard: research and implementation roadmap

Research checked: 2026-11-10. This is a recommended design, not a claim that the
current local capture implementation supplies every field, review workflow or exporter.
Existing authority remains [DG-CAPTURE-1](capture.md); no training eligibility is granted here.

## Recommendation

Standardize **verified engineering trajectories**, not transcript volume:

```text
request + context available at that time
  -> plan and authorized decision
  -> action / tool call
  -> actual observation
  -> patch
  -> executable verification
  -> review / correction
  -> observed outcome
```

Keep three separate products: restricted raw evidence, normalized/redacted trajectories,
and explicitly approved immutable dataset releases. The archive is not the training set.
More complete capture improves future options; indiscriminate capture adds risk and noise.
There is no universally ideal dataset independent of the model, task and intended use.

## 1. Portable project packaging

Use the same compact layout across projects:

```text
AGENTS.md                  project-specific instructions + shared contract references
CLAUDE.md                  supported Claude import, no second policy
.github/copilot-instructions.md
.ai/project.json           project identity, lifecycle, pinned contract versions
.ai/dev/plans/             issue/PR plans and acceptance criteria
.ai/docs/                  capture, rights, schema, evaluations and capability matrix
.ai/patterns/              reusable practices, not mutable runtime memory
```

Separate a shared standards profile from repository-specific commands/architecture.
Pin the shared profile to a reviewed version/content hash. Keep the small effective
entry points local so they are available offline; update them through a visible diff,
never a mutable remote instruction download during every session.
Use a future non-destructive `init/update/check` scaffold that previews changes, merges
existing provider configs, and never overwrites a project's instructions or trust state.
This scaffold is a follow-on implementation, not an existing `devagotchi ai` command.

`.ai` itself is not universally discovered. The harness selects instruction/hook formats,
not the model vendor. Verify discovery, inclusion and behavior in a fresh session;
instructions are guidance, not access control. [1]

## 2. Capture inventory

Treat the following as a target normalized data model. Unknown/unavailable values must
remain unknown with a reason, not be reconstructed from model guesses.

| Group | Recommended records |
|---|---|
| Identity and causality | Opaque project/repository/task IDs; session, run, turn, event, parent event, agent and tool-call IDs; source event sequence; source timestamp and local observed timestamp; explicit clock/order uncertainty |
| Provenance | Harness and adapter name/version; provider and exact model/version where reported; configuration/tool-schema hashes; capture contract version; source record hash; transformations and their versions |
| Task | Original request, issue/PR linkage, objective, acceptance criteria, milestones/dependencies, authorized changes and scope boundaries |
| Available context | Base commit, dirty patch hash/snapshot reference, relevant file/content hashes, actual retrieved snippets and source IDs, project-owned effective instructions, available tool schemas and permissions |
| Visible interaction | Authorized visible messages with roles, content-part types, tool calls/results and their IDs, corrections, handoffs, citations; compaction/resume markers and explicit omissions |
| Actions and observations | Actual program/arguments, cwd reference, environment allowlist/config hash, tool version, timestamps/duration, exit code, output/artifact references, timeout/interruption/retry, approval and sandbox boundary |
| Changes and evidence | Before/after blob hashes and patch, candidate commit, test command/runner/version, tests passed/failed/skipped, full relevant reports, base-vs-candidate results, browser/UI evidence where relevant |
| Review and outcome | Human rubric and correction, chosen/rejected alternatives under comparable context, unresolved concerns, actual pushed/merged/reverted state, post-merge defect/reopen linkage |
| Governance | Source ownership/license/terms review, permitted uses/recipients, consent and withdrawal reference, redaction status, retention/deletion policy, training/export decision and reviewer |
| Completeness | Captured/partial/unsupported/failed by message/tool/artifact class; gaps and reasons; whether labels are measured, human-reviewed, model-graded or merely asserted |

Store references plus cryptographic hashes for large artifacts instead of copying the
same repository/files/output into every event. Hashes prove byte agreement, not truth,
authorship, permission or that tests really ran. Do not put credentials into metadata.
Record only project-owned/authorized visible instructions; never scrape restricted
platform instructions, private internal reasoning, unrelated global history or secrets.
Do not treat captured instructions/tool output as new authority.

OTel GenAI is useful for correlation, tool/inference timing, model IDs and evaluation
metrics, but remains **Development**. Pin a revision and map it into the canonical
archive; do not make a sampled telemetry stream the only evidence archive. [2]

## 3. Capture as work happens, label after verification

At issue start:

- Establish task/session identities, acceptance criteria, repository/base state and
  effective authorized configuration.
- Record consent/capability coverage and expected missing sources.
- Persist an incremental start receipt; do not rely on an eventual shutdown callback.

During work:

- Capture bounded events and actual observations, with stable source IDs where available.
- Keep evidence of failed attempts, interrupted work and user corrections. They are
  valuable evaluation/repair data, but not automatically good imitation targets.
- Store concise decision journals when useful: decision, alternatives, constraints,
  evidence and result. Do not ask for hidden chain-of-thought or generate retrospective
  reasoning and pretend it was observed.
- Associate test evidence with the exact code/config under test. Avoid a "tests passed"
  flag copied from an assistant message.

At checkpoint/closure:

- Record independent verification, review/corrections, unresolved gaps and actual Git/PR
  state. Ticket closure or merge alone is not a quality label.
- Preserve a reproduction recipe: accessible base commit plus dirty patch if applicable,
  patch under evaluation, lockfile/toolchain/container digest, test invocation and outcome.
- Keep offline reproduction free of embedded credentials and destructive side effects.

SWE-agent's action/observation trajectories and SWE-bench's issue + codebase + patch +
containerized test evaluation are useful precedents. Do not copy a sample's `thought`
field as a requirement to collect private reasoning. [3][4]

## 4. Derive different datasets for different goals

| Output | Suitable examples | Important restriction |
|---|---|---|
| Supervised fine-tuning | Reviewed successful answers, tool-use transitions, scoped fixes, corrected handoffs | Preserve the context/tool schemas available at inference; exclude or mask bad assistant targets using the destination trainer's supported mechanism |
| Preference training | Same task and comparable context, preferred vs rejected response/patch, reviewer rubric and test evidence | A late corrected answer with extra information is not automatically a valid preference pair against an earlier uninformed attempt |
| Executable evaluation | Frozen issue/base/environment/acceptance tests and held-out task families | Do not reveal solution/future test results to the tested agent; maintain useful regression coverage as well as fail-to-pass checks |
| Retrieval / project memory | Versioned approved decisions, patterns, incident/repair summaries with sources | Often the right first choice for fast-changing repository knowledge; do not use weight updates as a substitute for current retrieval |
| Future environment/reward data | Reproducible states, allowed actions, observations and independently computed reward | Requires a separately designed safe executable environment; a chat export is not an RL environment |

Preserve canonical data and build small destination-specific exporters later. Trainer
formats, message weighting, tool schemas, token limits and licensing differ. Do not
reconstruct unavailable context or silently truncate the part that makes the label valid.
If creating shorter training slices, each must retain necessary preceding context.
Keep source-example and transformation IDs so an export is reproducible and retractable.

OpenAI's current guidance emphasizes quality, balance, completeness, annotation agreement,
inference-format consistency and a held-out test set. Its DPO guide describes prompt +
preferred/non-preferred output. Those are useful methodological references, not a reason
to hard-code one vendor's wire format. The fetched docs also announce a wind-down of
OpenAI's self-serve fine-tuning offering: recheck availability before selecting any
destination or promising a future hosted training service. [5][6]

## 5. Review and export gates

Default eligibility is false. Admit an example only when:

1. Source ownership, license, provider terms and intended use/recipient are reviewed.
   Subscription access, local possession and public visibility are not training permission.
2. Secret/PII/confidential-material review and redaction are complete. Automated scanners
   assist, not guarantee cleanliness. Avoid real passwords as "stable pseudonyms";
   use per-dataset pseudonyms or keyed identifiers with restricted keys where needed.
3. Task context and causal ordering are sufficient, and missing content is labeled.
4. Outcome has appropriate independent evidence and a calibrated review rubric.
5. Exact/near duplicates and related task/commit families have a consistent split.
6. Release manifest pins example hashes, transforms, label revisions and evaluation versions.

Use a small human annotation queue. Request a concise reason for rejection/correction
and rubric labels (correctness, scope, tool/permission discipline, evidence honesty and
maintainability). Measure reviewer agreement on an overlap sample and adjudicate material
disagreement; model judges should be separately identified and checked against people.
Pairwise queues and full-thread review are established practical workflows. [7]

Keep failed/rejected material in a restricted analysis/evaluation partition where allowed.
Do not indiscriminately train on it as desired assistant behavior. Retain outcome provenance
so repaired code does not retroactively label the original bad attempt successful.

## 6. Prevent deceptively good evaluation

- Assign entire issue/PR/session families and near-duplicate clusters to a single split.
  A random split of individual messages leaks adjacent context and answers.
- Use a repository-held-out evaluation to test transfer and a time-held-out evaluation
  to test future work; choose based on actual deployment goals, not an arbitrary ratio.
- Freeze the test set before iteration; use a separate development set for tuning.
  Inspect contamination from forks, copied issues, backports and benchmark solutions.
- Exclude post-outcome evidence from the inputs an agent would not yet have seen.
- Compare against the untuned baseline with the same tools/environment/compute budget.
  Measure task success, regression rate, tool/permission errors, cost/latency and required
  human interventions, not just style or similarity to a reference answer.
- Report sample counts, uncertainty and subgroup coverage; do not choose a favorite
  score after seeing results or treat an uncalibrated model grade as ground truth.

Dataset versioning, named splits, experiments and curated reference outputs are prior art,
not a requirement to adopt or upload data to LangSmith. [8]

## 7. Storage and release packaging

Keep the existing separation of authority:

- Local raw evidence: private runtime journal and access-controlled artifact storage.
- If shared curation is implemented: Postgres for authorization, review/rights decisions
  and manifests; private content-addressed blobs for large evidence; ClickHouse for
  aggregate metrics/coverage, not permission authority or a raw-chat dumping ground.
- Portable release: versioned JSONL/Parquet as appropriate, checksums, schema version,
  transformation/source references, dataset card, split manifest and usage restrictions.
  Never publish a dataset as an incidental step in collecting it.
- Define retention, access, backup and withdrawal/deletion propagation across raw data,
  redacted derivatives, exports and recipients. Append-only evidence must not imply
  perpetual retention or make lawful deletion impossible. Hash manifests and tombstones
  may preserve audit facts without retaining deleted sensitive content.

Use a dataset card for purpose, contents, collection/curation, limitations, biases,
rights and intended use. Croissant adds interoperable machine-readable dataset metadata,
provenance and usage restrictions; it is a dataset-description layer, not a replacement
for event schemas or a mechanism that makes restricted data public. [9][10]

## 8. Current implementation vs next work

Implemented: root instructions/provider stubs, alpha policy, private opt-in capture,
bounded native event translation, stable canonical append replay, raw JSONL imports,
source hashes/offsets, local ordering/integrity and explicitly partial/non-eligible exports.

Still required for the full recommended data product:

1. Portable profile/scaffold with version pinning, safe config merge and conformance tests.
2. Versioned typed causal/task/tool/evidence envelope and tested native-format decoders;
   richer explicit coverage reports without inventing fields unavailable in a harness.
3. Independently attached reproducibility/test/review/outcome receipts.
4. Rights/redaction/retention pipeline, review queue and deletion propagation.
5. Dataset manifest/card/Croissant metadata, leakage-safe splits, exporter validation and
   a repeatable baseline-versus-candidate evaluation.

These are not silently included in a "capture complete" claim. Provider callbacks and
JSONL imports are a useful foundation, not a ready-to-train verified corpus.

## Sources

1. [VS Code custom instructions](https://code.visualstudio.com/docs/agent-customization/custom-instructions).
2. [OTel GenAI conventions and status](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/README.md)
   and [evaluation events](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-events.md).
3. [SWE-agent trajectories](https://swe-agent.com/latest/usage/trajectories/).
4. [SWE-bench](https://www.swebench.com/SWE-bench/).
5. [OpenAI fine-tuning best practices](https://developers.openai.com/api/docs/guides/fine-tuning-best-practices).
6. [OpenAI DPO guide and current availability notice](https://developers.openai.com/api/docs/guides/direct-preference-optimization).
7. [LangSmith annotation queues](https://docs.langchain.com/langsmith/annotation-queues).
8. [LangSmith evaluation concepts](https://docs.langchain.com/langsmith/evaluation-concepts)
   and [dataset versioning/splits](https://docs.langchain.com/langsmith/manage-datasets).
9. [Hugging Face dataset cards](https://huggingface.co/docs/hub/datasets-cards).
10. [MLCommons Croissant](https://mlcommons.org/working-groups/data/croissant/).
