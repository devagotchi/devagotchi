// stylelint plugin `coldlight/known-tokens`: every `var(--cl-*)` names a token the build emits.
// A typo or an invented token is reported with the closest real names, so the fix is in the
// message. Custom properties without the `cl-` prefix are a component's own and are not checked.

import { readFileSync } from "node:fs";
import stylelint from "stylelint";

const RULE = "coldlight/known-tokens";
const TOKENS = new URL("../../src/coldlight/tokens/build/tokens.json", import.meta.url);

/** @type {string[] | undefined} */
let known;
function knownTokens() {
  if (!known) {
    try {
      known = JSON.parse(readFileSync(TOKENS, "utf8")).map((/** @type {{css: string}} */ t) => t.css);
    } catch {
      throw new Error("coldlight/known-tokens: run `npm run tokens` first; tokens/build/tokens.json is missing");
    }
  }
  return /** @type {string[]} */ (known);
}

/** Levenshtein distance, for the suggestions. */
function distance(/** @type {string} */ a, /** @type {string} */ b) {
  /** @type {number[]} */
  let row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i += 1) {
    /** @type {number[]} */
    const next = [i];
    for (let j = 1; j <= b.length; j += 1) {
      const same = a[i - 1] === b[j - 1] ? 0 : 1;
      next[j] = Math.min((row[j] ?? 0) + 1, (next[j - 1] ?? 0) + 1, (row[j - 1] ?? 0) + same);
    }
    row = next;
  }
  return row[b.length] ?? 0;
}

const messages = stylelint.utils.ruleMessages(RULE, {
  unknown: (/** @type {string} */ name, /** @type {string[]} */ near) =>
    `${name} is not a ColdLight token. Did you mean ${near.join(", ")}? Every token, with what it is for, is in ui/catalog.json.`,
});

/** @type {import("stylelint").Rule} */
const rule = (enabled) => (root, result) => {
  if (!enabled) return;
  const names = knownTokens();
  root.walkDecls((decl) => {
    for (const [, name = ""] of decl.value.matchAll(/var\(\s*(--cl-[a-z0-9-]+)/g)) {
      if (names.includes(name)) continue;
      const near = [...names].sort((x, y) => distance(name, x) - distance(name, y)).slice(0, 3);
      stylelint.utils.report({ ruleName: RULE, result, node: decl, message: messages.unknown(name, near), word: name });
    }
  });
};
rule.ruleName = RULE;
rule.messages = messages;

export default stylelint.createPlugin(RULE, rule);
