# Pattern: the UI, Svelte on ColdLight

This page is how every screen of Devagotchi is built. The authority is the operator's:
- [operator rules](../operator-rules.md), "The UI";
- [the program](../dev/plans/program.md) §1;
- the operator's decision of 2026-11-25: **all existing UI converts to Svelte**.

The shared rules G1 to G14 in [README.md](README.md) hold here too. The checks named below
are built in program step 10, together with the first screens. Until each one exists, review
holds its rule.

## What converts

| Existing UI | Where it is | Becomes |
|---|---|---|
| The hub's server-rendered pages | `engine/*/web/`, HTML built in Rust strings | Svelte routes per domain in `ui/`. A Rust page is deleted once its Svelte route and its JSON (`http.rs`) ship |
| The original AgentHub UI | `agent-hub/web/src`: React 19, 20 routed screens, 3,770 lines, 414 lines of CSS | Its screens and the API calls each makes are the spec. No code is carried over |
| The website | `apps/website`: React, React Aria, Style Dictionary | SvelteKit, prerendered for search engines |
| The desktop inspector | `apps/desktop`: Tauri and React | Svelte in the same Tauri shell |

The original AgentHub screens, by the domain each goes to:
- **agenthub:** board, task, new task, notes;
- **identity:** agents and the agent editor, roles, tokens, login, connect;
- **settings:** settings and its tabs;
- **ops:** audit and audit retention, actions (the action feed);
- **machines:** machines, a machine, GUI sessions;
- **access** (under machines until the access domain is ported): requests, grants, role
  bindings;
- **policy:** policy, changes;
- **providers:** subscriptions, providers;
- **guides:** guides, a guide, guide updates.

## Rules

**U1. One UI app.**
- `ui/` is a SvelteKit app. The hub serves its build.
- The landing page is prerendered and SEO-friendly; the rest sits behind sign-in.

**U2. The UI is dumb.**
- It reads and writes only through the engine's JSON interfaces (`interfaces/http`, each
  domain's `http.rs`), and decides nothing the engine owns (G1).
- A screen shows what the API answers, including its refusals' `next`.
- Check: no `fetch` outside `ui/src/lib/api/`, and the API client is generated from the
  contracts.

**U3. Folders are the navigation.**
- Routes mirror the engine's domains: `ui/src/routes/(app)/<domain>/<tab>/+page.svelte`.
- Each domain folder has a `meta.ts` with its label, order and icon. The navigation and tabs
  are built from those files (`import.meta.glob`), never wired by hand.
- Check: every engine domain with an `http.rs` has a route folder, and every route folder has
  a `meta.ts`.

**U4. Style Dictionary makes all CSS.**
- Tokens live in `ui/src/lib/coldlight/tokens/`, in DTCG JSON, as three layers:
  - primitive (raw values);
  - semantic (roles: surface, text, accent, danger, space-m, step-0, …);
  - composite (typography, elevation, focus ring).
- Style Dictionary emits them as CSS custom properties and TypeScript.
- Type and space are fluid `clamp()` scales between a phone and a desktop.
- Layout uses flex and grid primitives. A component decides its own layout with a container
  query; pages have no viewport breakpoints of their own.
- Check: stylelint rejects a literal colour, size or font outside the token build.

**U5. ColdLight grows bottom up.**
- `ui/src/lib/coldlight/` holds `tokens/`, `primitives/`, `blocks/`, `templates/` and
  `patterns/`.
- Primitives come first: button, field, select, badge, notice, dialog, card, list, tabs,
  table, code, diff.
- Blocks are made only of primitives, templates only of blocks and primitives. Patterns are
  whole experiences, such as resolving a conflict.
- A level imports only from the levels below it. Check: an import-boundary lint rule.

**U6. Each component keeps its variants with it.**
- `Button/` holds `Button.svelte`, `Button.variants.ts`, `Button.stories.svelte` and
  `Button.test.ts`. Its variants are data, not copies.
- Check: every component folder has a story and a test.

**U7. Storybook shows every state.**
- Every ColdLight component, and every page in each of its states: loading, empty, error,
  refused, conflict and full.
- Stories use recorded API answers, not hand-written ones.

**U8. Nothing is hardcoded.**
- Words come from content files, ready for translation.
- Limits, options and choices come from the API's configuration.
- No number, colour, label or URL is written in a component.

**U9. Failures help.**
- Every error shows what happened, why, and what to do next, from the refusal's
  `{code, message, next}`.
- When the API offers a fix, the error offers it as a button.
- Nothing fails silently or shows a raw stack.

**U10. Conflicts are resolved in a friendly popup.**
- When a change meets a newer version (`changed_since_read`, an import that collides, a stale
  setting), the popup offers keeping the incoming change, keeping the existing one, or merging
  both.
- It previews the result as a clear red and green diff before anything is written.
- It is a ColdLight pattern: one implementation, used everywhere.

**U11. Logging and audit are visible.**
- Every write shows what was recorded: its audit line or receipt.
- Each domain has a friendly dashboard of its state and recent activity, with live updates
  over the hub's event stream.

**U12. Versions come from code.**
- The UI's version is the hub's build. Settings > About shows the build, its channel and the
  UI's version.
- No version is typed by hand anywhere in the UI.

**U13. Accessible by default.**
- Keyboard first, with labelled controls and 48 dp touch targets.
- Svelte's accessibility warnings fail the build.

## Anti-patterns

- CSS written by hand outside the token build. A page restyling another page's classes.
- New UI built as HTML in Rust strings. That is today's server-rendered pages; they are
  converted, not extended.
- A business rule in a component.
- A route added without its domain folder and `meta.ts`.
- A hand-written navigation list.
- A component without a story.

## What this replaces

- [hub-pages.md](hub-pages.md) still holds the existing server-rendered pages until each one
  is converted. No new page is written that way.
- The hand-written sheets under `engine/*/web/` and `web/style/` (#269) are replaced by the
  token build, not extended.
