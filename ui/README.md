# Devagotchi UI

Devagotchi's screens, built in Svelte on **ColdLight**, its own design system. Every colour,
size and font comes from one set of design tokens, so a screen looks right because it cannot
look any other way. The rules are [ui.md](../.ai/patterns/ui.md); this page is how to work in it.

## Start

```sh
cd ui
npm ci
npm run storybook    # every component in every state, light and dark, at http://localhost:6006
npm run verify       # everything CI checks: types, lint, tests, the catalog, the build
```

Node 22.18 or newer. `npm run build-storybook` builds Storybook as CI does.

## Build a screen

1. Open [CATALOG.md](CATALOG.md) (or [catalog.json](catalog.json), for tools and AIs). It lists
   every component, when to use it and when not to, its variants, its props and an example, and
   every token with what it is for.
2. Put the screen together from those components. Give them their words through props: no text
   is written inside a component.
3. If you write CSS, write tokens: `var(--cl-space-s)`, `var(--cl-color-danger)`. Anything else
   is refused, and the message says what to write instead.

## Add a component

A component is a folder under `src/coldlight/<level>/<Name>/` with four files:

| File | What it holds |
|---|---|
| `<Name>.svelte` | The component. Its CSS uses tokens only, and selects variants with `data-<axis>` attributes. |
| `<Name>.variants.ts` | What it is: summary, when to use it, its variants as data, its props, accessibility, an example. The catalog is built from this. |
| `<Name>.stories.svelte` | A Storybook story for every variant and state. |
| `<Name>.test.ts` | Its behaviour, keyboard and labels, and `expectVariantsStyled` (the CSS styles every variant the data declares, and no other). |

[Button](src/coldlight/primitives/Button/) is the reference. The levels, lowest first: `primitives`,
`blocks` (made of primitives), `templates`, `patterns` (whole experiences, such as resolving a
conflict). A level imports only from the levels below it. Then run `npm run catalog` and commit
`catalog.json` and `CATALOG.md` with the component.

## Change a colour, size or font

Edit the tokens in [src/coldlight/tokens/](src/coldlight/tokens/) (DTCG JSON, built by
Style Dictionary), never the CSS:

- `primitive/`: raw values, the palette and scales. Components never see these.
- `semantic/`: roles (`color.surface`, `color.danger`, `space.m`, `step.0`), with the dark theme in
  `color.dark.tokens.json`. This is what components use.
- `composite/`: roles made of several values (`type.body`, `elevation.overlay`, `focus.width`).

`npm run tokens` builds them into `src/coldlight/tokens/build/` (CSS custom properties, a
TypeScript map and the catalog's data). The build refuses two tokens whose CSS names collide, and
any text colour that falls below WCAG AA contrast (4.5:1) on its background in either theme.

## When a check fails

| Message | What to do |
|---|---|
| `… is a literal: write a ColdLight token …` | Replace the value with the token the message names, or find one in CATALOG.md. |
| `--cl-… is not a ColdLight token. Did you mean …` | A typo or an invented token: use one of the names it suggests. |
| `A ColdLight primitive is made only of the levels below it` | Move the import, or move your component up a level. |
| `Unexpected use of 'fetch'` | Data comes only through `src/api/`, the engine's client (U2). |
| `no CSS rule selects [data-…]` | A variant in `.variants.ts` has no CSS, or the CSS styles a variant the data does not declare. |
| `ui/catalog.json … differ from the sources` | Run `npm run catalog` and commit the two files. |
| `tokens: the build refused these tokens` | Each line says which token, why, and which file to change. |

## Notes

- **`overrides.glob`** in `package.json`: Style Dictionary 5.8.0 pins `glob` 13, which carries
  advisories GHSA-3g3m-2fjr-6pw4 and, through `minimatch`, GHSA-g3hv-75wg-m249. The override moves
  only that dependency to the patched `glob` 14, whose `globSync` Style Dictionary calls
  unchanged. Remove it when Style Dictionary depends on a patched `glob` itself.
- **Windows:** `npm run test` goes through `scripts/vitest.ts`, which gives Vitest one spelling of
  the drive letter (`D:` and `d:` otherwise load it twice).
- The hub serves the static build (U1); that arrives with the first domain's screens.
