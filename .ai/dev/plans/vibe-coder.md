# Vibe coder quality of life: what makes Devagotchi the place a solo builder's AI work lives

- Contract: DG-DELIVERY-1; stage: read [project.json](../../project.json) (alpha = pre-release)
- Issue / request: operator, 2026-11-15: "take a moment to figure out really really vibe coder
  quality of life features we can offer with devagotchi", for the free solo and small-business
  edition first, then managed and the commercial cloud.
- PR: per feature, once the operator picks it
- Branch / baseline: none yet
- Objective: the person who builds with Lovable, Claude, Codex or Grok, often without a
  developer's habits, gets the habits for free: nothing is lost, nothing leaks, nothing ships
  unchecked, and they always know what their AI did, in plain words, on their phone.
- Non-goals: an editor or an app builder of our own (Lovable, Bolt, Replit build the app; we are
  the hub their AI and their app work through); anything that needs a developer to set it up.
- Canonical architecture / related plan: [free edition](free-edition.md),
  [factory](agenthub-factory.md), [connections gateway](connections-gateway.md),
  [plugins](plugins.md), [AI sessions](ai-sessions.md), [mobile](mobile-experience.md).

## Proposed (operator to pick; each builds on a milestone that exists)

| # | Feature | What the person gets | Built on |
|---|---|---|---|
| V1 | **Bring your Lovable app home** | One tap: the app's code on their own machine or devspace, a board started from its README and TODOs, a list of the secrets it needs to fill in Vaultix | connections X8, factory F5 |
| V2 | **Keys never go in chat** | The AI is handed a connection, never a key; a key pasted into a prompt or a commit is caught by the machine's hook, the hub says which and how to rotate it | connections X2 to X5, factory F2 hooks |
| V3 | **A checkpoint before every AI task** | Each claimed ticket works in its own git worktree with a snapshot first; "put it back the way it was this morning" is one tap | factory F15, machine engine |
| V4 | **Approve the scary things from the phone** | Deploys, database migrations, pushes to `main`, a payment change and spend over a limit wait for a tap, with what, why and how to undo it in plain words | approvals (exist), mobile P2a |
| V5 | **"Your AI is stuck"** | When an agent fails the same check three times, loops on one file or lets a lease expire, the phone says so with the last error and three choices: retry, hand to another AI, stop | factory F11, F14, AI sessions S6 |
| V6 | **What happened today, in plain words** | A daily digest per project on the phone: what was done, what is waiting for them, what broke, with links to each ticket | board events, mobile P2a |
| V7 | **A preview link on every finished ticket** | The ticket shows the deploy preview (Vercel, Lovable) beside the diff and the test result, so they check the thing, not the code | factory F8, connections X7 |
| V8 | **Spend limits** | A monthly cap per connection (OpenAI, Anthropic, Stripe test or live), shown as a meter; over the cap, calls wait for a tap | connections X2, approvals |
| V9 | **Machine doctor** | "Node is missing on nas" becomes one approval that installs it; the AI sees what each machine has before it tries | machines, Desktop Commander parity D3 |
| V10 | **Several AIs, one plan** | Claude builds, Codex reviews, Grok tests: each is a seat on the same board, hand-offs carry the context, the person sees who did what | factory F12 to F14, plugins G5 |
| V11 | **Start a product in one go** | A workflow that sets up the repo, Supabase, Stripe test keys in Vaultix, a Resend domain with its DNS records checked, and the board with the first tickets | workflows (exist), connections X4, X7 |
| V12 | **Learn as you go** | Each finished ticket can explain its diff in plain words and what the tests prove, so a vibe coder picks up the practices (rrflow's aim for vibe coders, its connectome) | factory F8, rrflow |

## Recommended first, because they are what a vibe coder loses sleep over

V2 (keys), V3 (checkpoints), V4 (approvals), V5 (stuck), V1 (Lovable home). Each lands inside
an existing milestone, so they cost a PR each once that milestone is built.

## Risks and approved decisions

- **Approved by the operator, 2026-11-15:** V2 (keys never in chat), V3 (a checkpoint before every
  AI task), V4 (phone approvals for deploys, migrations, pushes to `main` and spend over a
  limit), V5 ("your AI is stuck", with retry, hand to another AI, or stop), V1 (bring your
  Lovable app home). The others stay proposals. Each approved row gets its milestone and
  evidence here as it is built, inside the plan it builds on.
- V2's commit hook runs on the person's machine and reads only staged changes; it never sends
  their code to the hub.

## Delivery

V1 to V5 approved; V6 to V12 proposed. Implementation: not started. PR: per feature.
