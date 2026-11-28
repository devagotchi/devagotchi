// A line diff by Myers' O(ND) algorithm ("An O(ND) Difference Algorithm and Its Variations",
// 1986): the shortest edit script from `before` to `after`, as lines kept, removed and added.
// It runs in time proportional to the size of the change, so a small edit to a large file is
// cheap. It is presentation only: what a change means is the engine's to decide (ui.md U2).

export type LineKind = "same" | "removed" | "added";

export interface DiffLine {
  kind: LineKind;
  text: string;
  /** The line's number in `before` (1-based), when it is there. */
  before?: number;
  /** The line's number in `after` (1-based), when it is there. */
  after?: number;
}

export function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  if (lines.at(-1) === "") lines.pop();
  return lines;
}

export function lineDiff(before: string, after: string): DiffLine[] {
  const a = splitLines(before);
  const b = splitLines(after);
  const n = a.length;
  const m = b.length;
  const max = n + m;
  const offset = max + 1;
  const v = new Array<number>(2 * max + 3).fill(0);
  const trace: number[][] = [];

  search: for (let d = 0; d <= max; d += 1) {
    trace.push(v.slice());
    for (let k = -d; k <= d; k += 2) {
      const down = k === -d || (k !== d && (v[offset + k - 1] ?? 0) < (v[offset + k + 1] ?? 0));
      let x = down ? (v[offset + k + 1] ?? 0) : (v[offset + k - 1] ?? 0) + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x += 1;
        y += 1;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) break search;
    }
  }

  // Walk the trace back from the end, then read the moves forwards.
  const lines: DiffLine[] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 1; d >= 0; d -= 1) {
    const row = trace[d] ?? [];
    const k = x - y;
    const down = k === -d || (k !== d && (row[offset + k - 1] ?? 0) < (row[offset + k + 1] ?? 0));
    const previousK = down ? k + 1 : k - 1;
    const previousX = row[offset + previousK] ?? 0;
    const previousY = previousX - previousK;
    while (x > previousX && y > previousY) {
      x -= 1;
      y -= 1;
      lines.push({ kind: "same", text: a[x] ?? "", before: x + 1, after: y + 1 });
    }
    if (d > 0) {
      if (down) {
        y -= 1;
        lines.push({ kind: "added", text: b[y] ?? "", after: y + 1 });
      } else {
        x -= 1;
        lines.push({ kind: "removed", text: a[x] ?? "", before: x + 1 });
      }
    }
  }
  return lines.reverse();
}

/** How many lines a diff adds and removes. */
export function diffCounts(lines: DiffLine[]): { added: number; removed: number } {
  return {
    added: lines.filter((line) => line.kind === "added").length,
    removed: lines.filter((line) => line.kind === "removed").length,
  };
}
