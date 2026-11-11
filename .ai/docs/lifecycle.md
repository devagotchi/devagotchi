# DG-LIFECYCLE-1: lifecycle contract

Canonical current stage: **alpha**, recorded once in [project.json](../project.json).
"Nearly alpha", "pre-release" and a package version are descriptions, not stage switches.
This project is not represented as a production service.
`release_mode: pre_release` and `alpha_readiness: unconfirmed` distinguish the
development stage from a tested alpha release. Merging this packet is not alpha
acceptance or permission to implement production rollout practices. Confirm alpha
readiness only through a reviewed acceptance receipt covering the actual supported
clients, integration paths and release gates; do not infer it from local tests.

## Mandatory at every stage

Authorization, privacy/secret handling, tenant boundaries, safe execution, reproducible
verification, license compliance, explicit errors and truthful completion reports.
Alpha does not permit shortcuts that corrupt data or conceal failures.
Preserve user data; an operator-approved reset of disposable pre-release fixtures is
different from silently deleting somebody's deployment.

## Alpha development

Prefer completing the intended architecture over speculative compatibility layers.
Breaking pre-release interfaces are allowed when explicitly planned, documented and
tested; never invent a production migration burden for nonexistent production users.
Keep existing supported behavior unless the task deliberately changes it. Verify the
smallest relevant slice while iterating; do not burn CI on known-incomplete work.

When something fails in alpha, it stays failed and gets worked all the way through: no
rollback, no retreat, and no stopping to report it instead of fixing it. Do not add rollout
gates, release lanes or recovery machinery for deployments that do not exist.

## Explicit production promotion

Only a reviewed PR may change `stage` to `production`. Its `production_promotion` object
must identify the approval, candidate commit/image, acceptance evidence and date.
The same PR must explicitly change `release_mode` to `production`.
The operator must explicitly authorize production promotion. A model cannot infer it.
The PR must reconcile the [release gates](../../docs/implementation-and-release-plan.md)
and record unresolved limitations; a JSON edit alone does not establish readiness.

Production activates: published compatibility/deprecation guarantees; tested migration,
backup/restore and rollback; staged rollout/change approval; operational ownership,
monitoring/SLOs, retention/deletion, incident and vulnerability-response procedures.
The promotion record must link the actual policies, not merely promise them.
Demotion does not erase obligations to existing production users.
