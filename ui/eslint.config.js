import svelte from "eslint-plugin-svelte";
import { defineConfig } from "eslint/config";
import ts from "typescript-eslint";

/** ui.md U5: a ColdLight level imports only from the levels below it. */
const LEVELS = ["primitives", "blocks", "templates", "patterns"];

/**
 * @param {string} level
 * @returns {import("eslint").Linter.Config}
 */
function boundary(level) {
  const above = LEVELS.slice(LEVELS.indexOf(level) + 1);
  return {
    files: [`src/coldlight/${level}/**`],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: above.map((up) => ({
            group: [`**/${up}/**`, `#coldlight/${up}/**`],
            message: `A ColdLight ${level.slice(0, -1)} is made only of the levels below it (${LEVELS.slice(0, LEVELS.indexOf(level)).join(", ") || "tokens"}); ${up} sit above it (ui.md U5).`,
          })),
        },
      ],
    },
  };
}

export default defineConfig(
  { ignores: [".svelte-kit/", "build/", "storybook-static/", "node_modules/", "src/coldlight/tokens/build/"] },
  ts.configs.recommended,
  svelte.configs.recommended,
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parserOptions: { projectService: true, extraFileExtensions: [".svelte"], parser: ts.parser },
    },
  },
  {
    files: ["src/**"],
    ignores: ["src/api/**"],
    rules: {
      "no-restricted-globals": [
        "error",
        { name: "fetch", message: "The UI reads and writes only through src/api/, the client of the engine's JSON interfaces (ui.md U2)." },
      ],
    },
  },
  ...LEVELS.slice(0, -1).map(boundary),
);
