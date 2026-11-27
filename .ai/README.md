# ALPHA = PRE-RELEASE. NOTHING IS IN PRODUCTION.

**The toggle is `"stage"` in [project.json](project.json). `alpha` means pre-release.** Read it
before anything else. It stays `alpha` until the operator promotes it ([how](docs/lifecycle.md)).

While it says `alpha`, every agent on every task works like this:

- **Nothing is live.** There are no production users, deployments or data to protect.
- **When something fails, leave it failed and work it all the way through.** No rollback, no
  retreat, no stopping to report it. Find the cause and fix forward until it works.
- **No production thinking.** No rollback plans, staged rollouts, release gates, compatibility
  layers, migration paths or "not safe to deploy" verdicts. They arrive with promotion.
- **Do not stop to ask whether to continue.** Keep going until the work is done.
- **Do not burn CI** on work you know is incomplete. Test the smallest relevant slice.

Alpha never excuses: broken authorization, tenant boundaries or secret handling, lost user data,
hidden failures, or claiming something works when it was not run.

---

# AI development contracts

Root [AGENTS.md](../AGENTS.md) is the instruction entry point; this directory supplies
its versioned contracts, not another agent or task engine.

| Canonical identifier | Owner |
|---|---|
| DG-LIFECYCLE-1 | [Lifecycle and explicit production promotion](docs/lifecycle.md); [machine state](project.json) |
| DG-DELIVERY-1 | [Planning, validation, autonomy and delivery](dev/workflow.md) |
| DG-CAPTURE-1 | [Consent, local journal, evidence and dataset eligibility](docs/capture.md) |
| DG-ADAPTER-1 | [Harness-specific adapters and capability limits](docs/adapters.md) |
| DG-RETRY-1 | [The broker's retry lifecycle: permanent retired keys, retry identity, refusal and history size](docs/retry-lifecycle.md) |

- **[Operator rules](operator-rules.md): the operator's own instructions. They hold over
  every other file here, this index and `AGENTS.md` included. Read them first.**
- [What is being built](../docs/product.md): the one product description; every other
  document agrees with it. [Documentation map](../docs/documentation-map.md): every
  document, and whether it is kept, fixed, folded or retired.
- [Plans](dev/plans/README.md): maintained task/PR plans, with [template](dev/plans/TEMPLATE.md).
- [Editions](docs/editions.md): what is core, what is cloud, and the license.
- [Patterns](patterns/README.md): reusable guidance, not duplicated architecture decisions.
- [Connector pattern](patterns/connectors.md): how an AI client is defined, proven and added.
- [Layout pattern](patterns/layout.md): where Rust code lives, and the file-size ratchet.
- [Rules](rules/): the workspace's rules as data, one folder per rule, checked by
  [bebop](../devtool/bebop/README.md) (`cargo run -p devagotchi-bebop -- check --all`).
- [Capture schema](docs/capture-event.schema.json): generated from canonical Rust types.
- [Cross-project training-data standard](docs/cross-project-training-standard.md):
  researched recommendations, data inventory, curation/evaluation gates and explicit
  implementation gaps; raw capture is not a training-ready dataset.
- [Evaluation definitions](docs/evaluations.json): measurable checks, not assertions that they passed.

Raw chats/journals/evidence live outside the checkout in the operator's private state
directory. Repository hook configs capture nothing until the operator opts in.
`devagotchi ai --help` exposes policy, hook capture, incremental transcript import,
paged export and actual Git delivery status. No command transmits chats to a third party.
