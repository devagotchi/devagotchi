# ui/: the UI boundary

The program's §8 card for this boundary ([program.md](../../.ai/dev/plans/program.md)).

- **Owns:** every screen of Devagotchi (the hub's pages, the website and the desktop inspector as
  each converts) and ColdLight, its design system: the tokens, primitives, blocks, templates and
  patterns, their stories, their tests and the catalog.
- **Serves:** people, through the hub, which serves the static build (U1); and the people and AIs
  who build screens, through [CATALOG.md](../CATALOG.md) and [catalog.json](../catalog.json).
- **Settings:** none of its own. Words come from content files, and limits, options and choices
  from the engine's configuration (U8). Versions come from the hub's build (U12).
- **Depends on:** the engine's JSON interfaces only, through `src/api/` (U2). Nothing in the Rust
  workspace depends on it.
- **Pattern:** [ui.md](../../.ai/patterns/ui.md). **Checks:** stylelint (tokens only, known tokens),
  ESLint (level boundaries, no `fetch` outside `src/api/`), svelte-check (accessibility warnings
  fail), `expectVariantsStyled`, the token build (contrast, name collisions) and the catalog check,
  run by [ui.yml](../../.github/workflows/ui.yml).
- **Intake:** components from other projects (`platform_devpulse/packages/coldlight` among them)
  come in only by being rewritten here to ui.md; nothing is shared until its reuse is confirmed.
