# AI sessions on your machines, from your phone

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json); capture rules:
  [DG-CAPTURE-1](../../docs/capture.md)
- Issue / request: operator, 2026-11-12: "the sessions active on a machine so they can easily see
  their Claude Code, their Claude duet if in the beta, the Codex dot, the Codex chats on that
  machine, in a layman easier way to resume sessions from their phone, see the push
  notifications and email backup, tailored mobile UI, fluid clamp, accessibility".
- PR: not_created
- Branch / baseline: one `increment-N/<topic>` branch per milestone, from `main`
- Objective: on the phone, each machine lists the AI coding sessions on it in plain words; a
  person opens one, reads where it got to, sends it the next instruction, and is told (push,
  then email) when a session finishes or waits for them.
- Non-goals: reading any session without the machine owner's opt-in; a chat client of our own
  for the AI (the session keeps running in the tool that made it).
- Canonical architecture / related plan: [roadmap](roadmap.md), [mobile experience](mobile-experience.md),
  [factory](agenthub-factory.md) F11 (live activity). Code: the machine side in `crates/link`
  and `crates/broker`, the hub in `crates/hub/src/web/app/`.

## Where the sessions are (verified 2026-11-12 on the clients' own docs before each milestone)

| Client | Where its sessions live on the machine | Resume headless | State |
|---|---|---|---|
| Claude Code | `~/.claude/projects/<folder>/<session>.jsonl` | `claude --resume <id> -p "<text>"` | to verify per version |
| Codex CLI | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `codex exec resume <id> "<text>"` | to verify per version |
| "Claude duet", "Codex dot" | not yet known: the operator names the product and version | unknown | needs the operator |

Each reader is a small, versioned adapter (like `crates/connectors`), so a format change breaks
one adapter's tests, not the app.

## Consent (DG-CAPTURE-1 holds)

Capture is off by default and global provider history is never scraped. This feature asks, per
machine, on its page, in two steps:
1. **Show sessions**: titles, folders, when, which tool, running or waiting. No message text.
2. **Show what they said**: the last messages of a session, read only when the person opens it,
   never stored by the hub beyond the page view. Separate, explicit, revocable.
Both are recorded on the account's chain and can be turned off from the phone.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| S0 | Name "Claude duet" and "Codex dot" (product, version, where they keep sessions) | operator | this table updated | needs the operator |
| S1 | Per-machine consent on the machine page (both steps), on the chain | none | app tests | not started |
| S2 | The machine lists its sessions (step 1 metadata) through the broker, read-only, bounded; adapters for Claude Code and Codex with fixtures | S1 | adapter tests on recorded fixtures; Linux and Windows CI | not started |
| S3 | Phone: a Sessions section on each machine and in the Go to sheet: "Claude Code in `devagotchi`, 12 min ago, waiting for you" | S2 | browser test on an emulated iPhone | not started |
| S4 | Open a session (step 2): where it got to, in plain words, with the last messages | S3 | app tests; consent refused shows nothing | not started |
| S5 | Resume from the phone: type the next instruction; the hub starts the client's headless resume on that machine through the broker and shows the answer as it comes | S4 | operation tests; browser test | not started |
| S6 | Told when it matters: the clients' own hooks (Claude Code `Stop` and `Notification`, Codex `notify`) report "finished" or "waiting" to the hub; push first, email if the push is not opened in N minutes (N set by the person) | S3 | hook tests; push and mail tests | not started |
| S7 | Fluid type and spacing with `clamp()` across the app; accessibility scan in CI | none | CI job (mobile P2f) | not started |

## Risks and approved decisions

Session files can hold secrets, code and personal data: nothing is read before the opt-in, step
2 is read on demand only, and nothing of it is logged. Resume runs the client as the machine's
account, so it is an operation like any other: audited, cancellable, and subject to the
machine's grants and approvals. Formats change between client versions: adapters fail closed
(the session is listed as "unknown format", never misread).

## Verification receipts

Each milestone records its own.

## Delivery

Implementation: not started. Push: not_verified. PR: per milestone.
**DELIVERY INCOMPLETE: NOT STARTED**
