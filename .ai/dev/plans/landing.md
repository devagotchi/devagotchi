# The landing page: layman interactives drawn from the real product

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-16: "the landing page needs to be properly rewritten to
  properly grab the real views the real examples ... we should have layman interactives properly
  planned out ... clear layman engaging visuals well optimized and the time taken to make the
  art and have it canonically consistent colocated hypermodular". Earlier: the setup video was
  to become interactive events a visitor triggers on click or that move on after 3 to 4 seconds;
  devagotchi.com should make the win over Desktop Commander easy to understand, along with owning
  your instance and the planned hosted tiers.
- PR: this plan, then one PR per scene (M3 onward). PR #221 (a first walkthrough) was merged on
  2026-11-17 as the interim hero, replacing the 1.7 MB video, when the operator asked for every
  open branch to be absorbed into `main`; it is the prototype this plan replaces scene by scene,
  and M13 retires it.
- Branch / baseline: `increment-172/landing-plan`, from `main` at `5104f35`
- Objective: someone who has never used a terminal understands, in a minute and by trying it
  themselves, what Devagotchi does for them, why it is safer and more capable than a computer in
  the chat, and how to start; every picture on the page is the product as it really looks, with
  real examples, drawn by the same code the product uses.
- Non-goals: a new visual identity (the site's tokens stay); video; images of screens (PNG/JPEG
  screenshots go stale); claims about features that are not built (they stay under "coming").
- Canonical architecture / related plan: [layout pattern](../../patterns/layout.md),
  [vibe coder](vibe-coder.md), [browser experience](browser-experience.md),
  [mobile experience](mobile-experience.md), the site model (`engine/hub/src/site/`).

## What went wrong in the first attempt (PR #221), so this plan avoids it

- **Screens drawn from memory.** Its scenes are hand-made HTML that resemble the product. The
  next change to the consent page or the phone app would make them wrong, and nothing would say
  so.
- **Examples invented.** Command output, sizes and messages were written, not recorded.
- **Everything in one sweep.** Walkthrough, comparison, video removal and tests in one PR, so no
  part got the time it needed and the operator could not review one thing at a time.
- **One generic carousel for every message.** A step list explains setup; it does not let a
  person feel what approving from a phone, or a job outliving the chat, is like.

## The principles every scene follows

1. **Real views, by the real code.** A scene that shows a Devagotchi screen renders it with the
   function that renders that screen in the product (the phone app's Overview, machine page,
   approvals, AgentHub board; the consent page; the add-a-machine page), given example data, and
   styled by the product's own stylesheet. When that screen changes, the landing page changes
   with it, and a test compares the two. Only screens that are not ours (Claude's connector
   dialog, a terminal) are drawn, from a captured reference (M0), and say whose they are.
2. **Real examples, recorded.** Every command, its output, file and figure is captured from a
   run on a real test machine (M1) and stored with the scene, with the date it was captured.
   One cast across the whole page: one person (Alex), one hosted hub address, machines `server`
   (a Linux VPS) and `home-pc` (Windows), one project (`shop`). The same machine never has two
   different stories.
3. **One idea per scene, and the visitor does it.** Each scene has one sentence it makes true
   (below), one thing the visitor presses, and an outcome they see. It also plays by itself:
   a beat every 3 to 4 seconds, and any press takes over.
4. **Plain words.** No `MCP`, `broker`, `OAuth`, `PTY` in a scene's words; the glossary keeps
   them for the sections that need them. Captions are a short sentence a person would say.
5. **Colocated and hypermodular.** One folder per scene, everything about it in it (below). A
   scene can be added, removed, reordered or switched off in `site.json` without touching
   another.
6. **Fast and light.** Budgets in M2: no images of screens, CSS and script per scene counted,
   animation on `transform` and `opacity` only, nothing loads until its scene is near the
   viewport, and every scene is complete and readable with motion off and without script.
7. **Honest.** A comparison claim cites a line of the other product's source or page; a planned
   feature is labelled as planned and shows no screen we do not have.

## A scene's home (hypermodular, colocated)

```
engine/hub/src/web/landing/scenes/
  mod.rs                  the scene contract, the registry site.json names scenes from, the
                          shared player (beats, pause, reduced motion, no-script fallback)
  player.js               the one script every scene shares (one file, its budget below)
  <scene>/
    mod.rs                the scene: its sentence, its beats, which real renderers it calls
    example.json          its recorded example data (M1), with where and when it was captured
    scene.css             its own styles, scoped under `.scene-<scene>`, built into hub.css
    tests.rs              it renders, its screens match the product's renderers, its budget holds
```

The words a site may change (captions, button labels) stay in `content/landing.json` under
`scenes.<scene>`, as the site model requires; the scene's structure and data stay in its folder.

## The scenes (storyboards for operator review before any is built)

Each row is one PR. "Real view" names the product code the scene renders, or the reference a
drawn screen is made from.

| # | Scene and its one sentence | What the visitor does | Beats (about 3.5 s each) | Real view | Examples recorded (M1) |
|---|---|---|---|---|---|
| S1 | **Hero, "Ask, and it happens on your computer."** | Picks one of three real asks | The ask in the chat; the AI runs it on `server`; the answer, with the real output | The AI's chat (drawn, reference M0); the phone Overview showing `server` busy (`web/app/home.rs`) | Three runs on a real VPS: why is the site slow (disk), restart the web server, back up the database |
| S2 | **Set up, "Three taps, no sign-up."** | Taps each step | Add the address to Claude; the consent page; one setup link opened on the machine; `server` online | Consent page (`web/oauth.rs`); the setup link page (`web/workflow`); phone Overview | A real connect and join on the test hub; the join line as the hub prints it |
| S3 | **Approvals, "Nothing risky happens without your yes."** | Taps Approve or Deny on the phone | The AI asks; the request on the phone with what, why and undo; approve: it is done; deny: it stops and says so | Phone Approvals (`web/app`, approvals card); the push notification | A real approval round trip: the request, its wording, the outcome |
| S4 | **Keeps running, "Close the chat. The work keeps going."** | Closes the chat window | A long job starts; the chat closes; the phone shows it still running; reopen: the AI picks up the result | Phone machine page, recent jobs (`web/app/machine.rs`) | A real long build that outlives a closed AI session |
| S5 | **All your machines, "Every computer you have, in one place."** | Taps a machine | Overview with `server` and `home-pc`; one machine's page: online, its build, its jobs | Phone Overview and machine page | Two real linked machines, Linux and Windows |
| S6 | **AgentHub, "Your AIs share one to-do list."** | Moves a ticket | Claude claims a ticket; Codex reviews it; done, with what each did | Phone AgentHub board (`web/app/agenthub`) | A real board with tickets worked by two AI apps |
| S7 | **Compared, "More than a computer in the chat."** | Picks a situation | For each situation (the laptop sleeps mid-job; the AI wants to delete files; a program asks for a password): what happens with Desktop Commander, cited, and with Devagotchi, shown by the real view | The evidence table (`landing/compare.rs`), its rows each linked to the scene that shows ours | Desktop Commander's cited lines (release v0.3.3); our side from S3, S4 |
| S8 | **Your way, "Run it yourself, or let us."** | Picks one | Self-host on your server (one command); the hosted hub, free in the alpha; managed for a team; the cloud tiers (planned, labelled) | The self-host terminal (drawn); the hosted sign-in | The real install line and its output |
| S9 | **Coming, labelled** | none | Keys never in the chat (Vaultix), devspaces (shippin.cloud), checkpoints before each AI task: a sentence each, no screens until they exist | none | none |

Order on the page: S1, S2, S3, S4, S5, S6, S7, S8, S9, then the FAQ and the closing call. The
sections the page has today (how, features, network, connect, self-host, next) are folded into
these scenes or kept as plain sections where a scene adds nothing; each fold is named in its PR.

## Milestones and tasks

| Milestone | Task | Dependencies | Acceptance evidence | State |
|---|---|---|---|---|
| M0 | Capture the real views: a script that runs a hosted hub, links a Linux and a Windows machine, connects an AI, signs in on an emulated phone, and screenshots every screen a scene shows, at phone and desktop sizes, light and dark | none | The capture set and the script in the repo (`apps/landing-capture/`) | pending |
| M1 | Record the examples: the same run executes each scene's asks for real and stores command, output and timing as `example.json` per scene | M0 | Example files with their capture date; a test that they parse | pending |
| M2 | The scene contract and player: `scenes/mod.rs`, `player.js`, CSS assembly, the budgets (HTML, CSS, script per scene and per page), no-script and reduced-motion behaviour, lazy start near the viewport | M0 | Contract tests; budget test; a scene with no screens passes them | pending |
| M3 | Real renderers callable with example data: each product screen a scene shows gets a render function that takes data, not a database (consent page, setup link page, Overview, machine page, approvals card, AgentHub board) | M2 | Each product page still renders the same (its tests); each renders from example data | pending |
| M4 to M12 | One scene per PR, S1 to S9, each: storyboard approved by the operator, built, its screens compared with the product's renderers, desktop and phone screenshots and a short recording attached to the PR, operator review before merge | M1 to M3 | Per scene: tests, budget, screenshots, operator approval | pending |
| M13 | Retire the old hero (PR #221's walkthrough or today's video, whichever is live) and the sections the scenes replace | the scenes that replace them | Page order test; nothing left unreachable | pending |

## Risks and approved decisions

- **Approved (operator, 2026-11-16):** plan first; real views and real examples; colocated,
  hypermodular scenes; one scene per PR with review. PR #221 was held on 2026-11-16 and merged
  as the interim hero on 2026-11-17 (operator: absorb every open branch into `main`).
- **To approve with this plan:** the scene list and order (S1 to S9); the cast (Alex, `server`,
  `home-pc`, `shop`); that drawn third-party screens say whose they are; that the landing page
  runs one small script (`script-src 'self'`, nothing inline), as PR #221 introduced.
- **Risk:** rendering product screens from example data needs their render functions separated
  from their data loading (M3). Where a screen cannot be separated cleanly, the scene waits
  rather than drawing an imitation.
- **Risk:** recorded examples age. Each carries its capture date; M1's script re-records them,
  and a test fails when a recorded tool name or output shape no longer matches the product.
- **Comparison:** naming Desktop Commander is allowed in the comparison scene only (PR #221's
  narrowed guard), every claim cited to its source.

## Verification receipts

None yet: this is the plan.

## Delivery

Implementation: pending. Validation: pending. Push: not_verified.
PR: not_created. Main: not_verified.
**DELIVERY INCOMPLETE: NOT PUSHED / NOT MERGED TO MAIN**
