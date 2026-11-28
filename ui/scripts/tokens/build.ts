// The token build (ui.md U4): Style Dictionary resolves the DTCG sources in
// src/coldlight/tokens/ and this script writes, into tokens/build/:
//   tokens.css  every semantic and composite token as a `--cl-*` custom property, light on
//               :root, dark under prefers-color-scheme and [data-theme="dark"], and every
//               duration 0ms under prefers-reduced-motion;
//   tokens.ts   a map from each token's name to its `var(--cl-*)`, for TypeScript;
//   tokens.json each token's name, CSS property, layer, type, description and values, which the
//               catalog (scripts/catalog/build.ts) publishes for people and AIs.
// Primitive tokens are resolved but never emitted, so a component cannot reach a raw value.
// The build fails, saying how to fix it, on two names that collide and on a text colour that
// does not reach WCAG AA contrast on the background it is meant for, in either theme.

import { mkdir, writeFile } from "node:fs/promises";
import StyleDictionary from "style-dictionary";
import { contrast } from "./contrast.ts";

const SOURCES = "src/coldlight/tokens";
const OUT = `${SOURCES}/build`;
const PREFIX = "cl";

/** Text colours and the backgrounds each is meant for; each pair must reach 4.5:1. */
export const READABLE_PAIRS: ReadonlyArray<readonly [string, string]> = [
  ["color.text", "color.canvas"],
  ["color.text", "color.surface"],
  ["color.text", "color.surface-sunk"],
  ["color.text", "color.accent-subtle"],
  ["color.text", "color.success-subtle"],
  ["color.text", "color.warning-subtle"],
  ["color.text", "color.danger-subtle"],
  ["color.text-muted", "color.canvas"],
  ["color.text-muted", "color.surface"],
  ["color.accent", "color.surface"],
  ["color.accent", "color.canvas"],
  ["color.accent", "color.accent-subtle"],
  ["color.on-accent", "color.accent"],
  ["color.on-accent", "color.accent-hover"],
  ["color.danger", "color.surface"],
  ["color.danger", "color.danger-subtle"],
  ["color.on-danger", "color.danger"],
  ["color.on-danger", "color.danger-hover"],
  ["color.surface", "color.text-muted"],
  ["color.success", "color.surface"],
  ["color.success", "color.success-subtle"],
  ["color.warning", "color.surface"],
  ["color.warning", "color.warning-subtle"],
  ["color.pending", "color.surface"],
  ["color.pending", "color.pending-subtle"],
  ["color.text", "color.pending-subtle"],
  ["color.diff-added-text", "color.diff-added"],
  ["color.diff-removed-text", "color.diff-removed"],
];

type Mode = "light" | "dark";

export interface BuiltToken {
  name: string;
  css: string;
  layer: "semantic" | "composite";
  type: string;
  description: string;
  value: string;
  dark?: string;
}

/** `--cl-` and the token's path joined by `-`; a step's `-1` keeps its minus (`--cl-step--1`). */
StyleDictionary.registerTransform({
  name: "cl/name",
  type: "name",
  transform: (token) => [PREFIX, ...token.path].join("-"),
});

const TRANSFORMS = [
  "cl/name",
  "color/css",
  "fontFamily/css",
  "cubicBezier/css",
  "shadow/css/shorthand",
  "typography/css/shorthand",
];

async function resolve(mode: Mode): Promise<BuiltToken[]> {
  const sd = new StyleDictionary({
    source: [
      `${SOURCES}/primitive/**/*.tokens.json`,
      `${SOURCES}/semantic/size.tokens.json`,
      `${SOURCES}/semantic/color.${mode}.tokens.json`,
      `${SOURCES}/composite/**/*.tokens.json`,
    ],
    usesDtcg: true,
    log: { verbosity: "silent", warnings: "error", errors: { brokenReferences: "throw" } },
    platforms: { css: { transforms: TRANSFORMS } },
  });
  const { allTokens } = await sd.getPlatformTokens("css");
  return allTokens
    .map((token) => ({ token, layer: token.filePath.replaceAll("\\", "/").split("/").at(-2) }))
    .filter(({ layer }) => layer === "semantic" || layer === "composite")
    .map(({ token, layer }) => ({
      name: token.path.join("."),
      css: `--${token.name}`,
      layer: layer as BuiltToken["layer"],
      type: String(token.$type),
      description: String(token.$description ?? ""),
      value: String(token.$value),
    }));
}

function fail(lines: string[]): never {
  console.error(["tokens: the build refused these tokens.", ...lines.map((l) => `  - ${l}`)].join("\n"));
  process.exit(1);
}

function checkNames(tokens: BuiltToken[]): void {
  const seen = new Map<string, string>();
  const clashes: string[] = [];
  for (const token of tokens) {
    const other = seen.get(token.css);
    if (other) {
      clashes.push(
        `${other} and ${token.name} both become ${token.css}. Rename one key in its .tokens.json so the CSS names differ.`,
      );
    }
    seen.set(token.css, token.name);
  }
  if (clashes.length) fail(clashes);
}

function checkContrast(byMode: Record<Mode, Map<string, BuiltToken>>): void {
  const problems: string[] = [];
  for (const mode of ["light", "dark"] as const) {
    for (const [fg, bg] of READABLE_PAIRS) {
      const a = byMode[mode].get(fg);
      const b = byMode[mode].get(bg);
      if (!a || !b) {
        problems.push(`${mode}: ${!a ? fg : bg} is listed in READABLE_PAIRS but is not a token. Add it, or take the pair out.`);
        continue;
      }
      const ratio = contrast(a.value, b.value);
      if (ratio < 4.5) {
        problems.push(
          `${mode}: ${fg} (${a.value}) on ${bg} (${b.value}) is ${ratio.toFixed(2)}:1; text needs 4.5:1 (WCAG AA). ` +
            `Point ${fg} or ${bg} at a darker or lighter step in primitive/color.tokens.json.`,
        );
      }
    }
  }
  if (problems.length) fail(problems);
}

function block(selector: string, lines: string[], indent = ""): string {
  return `${indent}${selector} {\n${lines.map((l) => `${indent}  ${l}`).join("\n")}\n${indent}}`;
}

function css(tokens: BuiltToken[]): string {
  const light = tokens.map((t) => `${t.css}: ${t.value};`);
  const dark = tokens.filter((t) => t.dark !== undefined).map((t) => `${t.css}: ${t.dark};`);
  const still = tokens.filter((t) => t.type === "duration").map((t) => `${t.css}: 0ms;`);
  return [
    "/* Generated by ui/scripts/tokens/build.ts from the DTCG sources beside this folder. Do not edit. */",
    "@layer tokens {",
    block(":root", ["color-scheme: light;", ...light], "  "),
    block("@media (prefers-color-scheme: dark)", [], "  ").replace(
      "{\n\n  }",
      `{\n${block(':root:not([data-theme="light"])', ["color-scheme: dark;", ...dark], "    ")}\n  }`,
    ),
    block(':root[data-theme="dark"]', ["color-scheme: dark;", ...dark], "  "),
    block("@media (prefers-reduced-motion: reduce)", [], "  ").replace(
      "{\n\n  }",
      `{\n${block(":root", still, "    ")}\n  }`,
    ),
    "}",
    "",
  ].join("\n");
}

function ts(tokens: BuiltToken[]): string {
  const entries = tokens.map((t) => `  ${JSON.stringify(t.name)}: ${JSON.stringify(`var(${t.css})`)},`);
  return [
    "// Generated by ui/scripts/tokens/build.ts. Do not edit.",
    "/** Each ColdLight token's name and the CSS that reads it. */",
    "export const tokens = {",
    ...entries,
    "} as const;",
    "",
    "export type TokenName = keyof typeof tokens;",
    "",
  ].join("\n");
}

const [light, dark] = await Promise.all([resolve("light"), resolve("dark")]);
const byMode = {
  light: new Map(light.map((t) => [t.name, t])),
  dark: new Map(dark.map((t) => [t.name, t])),
};
const tokens = light.map((t) => {
  const other = byMode.dark.get(t.name);
  return other && other.value !== t.value ? { ...t, dark: other.value } : t;
});
checkNames(tokens);
checkContrast(byMode);

await mkdir(OUT, { recursive: true });
await Promise.all([
  writeFile(`${OUT}/tokens.css`, css(tokens)),
  writeFile(`${OUT}/tokens.ts`, ts(tokens)),
  writeFile(`${OUT}/tokens.json`, `${JSON.stringify(tokens, null, 2)}\n`),
]);
console.log(`tokens: ${tokens.length} tokens (${tokens.filter((t) => t.dark).length} with a dark value) in ${OUT}`);
