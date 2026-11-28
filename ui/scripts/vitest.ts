// Runs Vitest with one spelling of the Windows drive letter. A shell may start in `d:\` while
// Node resolves modules under `D:\`; Vitest then loads itself twice and cannot find its runner.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const upper = (path: string) => path.replace(/^[a-z]:/, (drive) => drive.toUpperCase());
const bin = upper(fileURLToPath(new URL("../node_modules/vitest/vitest.mjs", import.meta.url)));
const run = spawnSync(process.execPath, [bin, ...process.argv.slice(2)], {
  cwd: upper(process.cwd()),
  stdio: "inherit",
});
process.exit(run.status ?? 1);
