# AI development contracts

Root [AGENTS.md](../AGENTS.md) is the instruction entry point; this directory supplies
its versioned contracts, not another agent or task engine.

| Canonical identifier | Owner |
|---|---|
| DG-LIFECYCLE-1 | [Lifecycle and explicit production promotion](docs/lifecycle.md); [machine state](project.json) |
| DG-DELIVERY-1 | [Planning, validation, autonomy and delivery](dev/workflow.md) |
| DG-CAPTURE-1 | [Consent, local journal, evidence and dataset eligibility](docs/capture.md) |
| DG-ADAPTER-1 | [Harness-specific adapters and capability limits](docs/adapters.md) |

- [Plans](dev/plans/README.md): maintained task/PR plans, with [template](dev/plans/TEMPLATE.md).
- [Patterns](patterns/README.md): reusable guidance, not duplicated architecture decisions.
- [Capture schema](docs/capture-event.schema.json): generated from canonical Rust types.
- [Evaluation definitions](docs/evaluations.json): measurable checks, not assertions that they passed.

Raw chats/journals/evidence live outside the checkout in the operator's private state
directory. Repository hook configs capture nothing until the operator opts in.
`devagotchi ai --help` exposes policy, hook capture, incremental transcript import,
paged export and actual Git delivery status. No command transmits chats to a third party.
