# Main builds again (B2a)

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json)
- Issue / request: operator, 2026-11-19: test B2 on the dev deploy. Its `/healthz` reports
  `main-18a4096`, five merges behind main, because no newer build exists.
- PR: #262 (merged as `3e2e5b9`)
- Branch / baseline: `increment-272/main-builds` from main `86b9d8b`
- Objective: every commit on main is built and attested again, and its hub image is pushed
  as `:main` and `:sha-<short>`, as `release.yml`'s push trigger and `dist-workspace.toml`
  already say (operator, 2026-11-18). Only a tag publishes a release. Pull requests still do
  not run the release build.
- Non-goals: deploying anything (the operator deploys the dev host); changing what a tag
  publishes; running the release build on pull requests.
- Canonical architecture / related plan: [the rewrite](rewrite.md),
  [self-hosted and provenance](self-hosted-and-provenance.md), `scripts/fetch-main-build.sh`.

## What went wrong

#258 (`2b3feec`) set `pr-run-mode = "skip"` so pull requests stop running the release build.
dist's `build-local-artifacts` runs only when `publishing` (a tag) or when `pr-run-mode` is
`"upload"`, so main stopped building too. `build-global-artifacts`, the install gate and
`custom-hub-image` need it and were skipped with it. Every release run from `2b3feec` to
`86b9d8b` passed with nothing built, so no check failed. Separately, since `e94174a` the
Attest step ran only for a tag, so `scripts/fetch-main-build.sh`, which refuses a main build
without its attestation, could never have accepted one.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M1 | `plan` gains a `building` output (main or a tag); `build-local-artifacts` and its Attest step use it | none | `runtime/devagotchi/tests/release_builds.rs` passes on the new file and fails on the old condition and five other ways main could stop building | done |
| M2 | Plans, status and changelog say what happened | M1 | this file, [rewrite](rewrite.md) status, CHANGELOG | done |
| M3 | Validate and publish | M2 | `cargo test -p devagotchi --test release_builds`, fmt, PR checks green | done: 18 of 18 |
| M4 | Merge, then the push to main builds | M3 | the release run on the merge commit shows `build-local-artifacts`, `custom-hub-image` and `custom-install-check` succeeded, and `ghcr.io/devagotchi/devagotchi:sha-<short>` exists | done: run 44657553081 |

## Risks and approved decisions

- A main push now takes the full release build again (about an hour of runners), as before
  #258. That is the 2026-11-18 decision: every main commit built, attested and kept.
- The install gate (`custom-install-check`) runs on main again. If it fails, main's build
  shows red: that is the gate working, and it is fixed, not skipped.

## Verification receipts

2026-11-19, Windows 11: `cargo test -p devagotchi --test release_builds`, 2 passed. The YAML
parses (PyYAML), and `plan.outputs.building` reads as written.

2026-11-19, the push of `3e2e5b9` to main: release run 44657553081 succeeded. Built: all
four `build-local-artifacts` targets (Linux and Windows, x86_64 and ARM64) with their Attest
step, `build-global-artifacts`, the hub image on both architectures, and `custom-install-check`
on seven runners. `custom-hub-image / publish` pushed `ghcr.io/devagotchi/devagotchi:sha-3e2e5b9`
(`docker manifest inspect` finds it). `host` and `announce` skipped: no tag, nothing published.

## Delivery

Implementation: done. Validation: done. Push: done. PR: #262, squash-merged.
Main: `3e2e5b9`; its own release run built and pushed the image (above).
Not yet: the dev deploy runs it (the operator's step).
