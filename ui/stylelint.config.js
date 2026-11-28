// ui.md U4: Style Dictionary makes every value; a component's CSS only arranges tokens.
// Each refusal says what to write instead, so a person or an AI can fix it from the message.

const USE_A_TOKEN =
  "write a ColdLight token, for example var(--cl-space-s) or var(--cl-color-danger); every token, with what it is for, is in ui/catalog.json";

/** Properties whose value must come from a token (or be a CSS keyword, or zero). */
const TOKEN_ONLY = /^(font|font-family|font-size|font-weight|line-height|letter-spacing|opacity|z-index|box-shadow|border-radius|transition-duration|animation-duration|transition-timing-function|outline-width|outline-offset)$/;

export default {
  extends: ["stylelint-config-standard"],
  plugins: ["./scripts/stylelint/known-tokens.js"],
  overrides: [{ files: ["**/*.svelte"], customSyntax: "postcss-html" }],
  ignoreFiles: ["src/coldlight/tokens/build/**"],
  rules: {
    "coldlight/known-tokens": true,
    "color-no-hex": [true, { message: `A hex colour is a literal: ${USE_A_TOKEN}.` }],
    "color-named": ["never", { message: `A named colour is a literal: ${USE_A_TOKEN}.` }],
    "function-disallowed-list": [
      ["rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color", "color-mix"],
      { message: (/** @type {string} */ fn) => `${fn}() makes a literal colour: ${USE_A_TOKEN}.` },
    ],
    "unit-allowed-list": [
      ["%", "fr", "cqi", "ch"],
      {
        message: (/** @type {string} */ unit) =>
          `${unit} is a literal size: ${USE_A_TOKEN}. Only %, fr, cqi and ch, which are relative, may be written by hand.`,
      },
    ],
    "declaration-property-value-allowed-list": [
      { [`/${TOKEN_ONLY.source}/`]: [/^var\(--cl-/, /^(0|none|inherit|initial|unset|normal|auto|currentcolor)$/] },
      { message: (/** @type {string} */ property) => `${property} takes its value from a token: ${USE_A_TOKEN}.` },
    ],
    "declaration-no-important": [true, { message: "!important fights the cascade; give the rule the right place instead." }],
    "selector-class-pattern": null,
    "custom-property-pattern": null,
  },
};
