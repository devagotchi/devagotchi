import { render, screen, within } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Diff.svelte?raw";
import Diff from "./Diff.svelte";
import { meta } from "./Diff.variants.ts";
import { diffCounts, lineDiff, splitLines } from "./lineDiff.ts";

const words = { addedLabel: "Added", removedLabel: "Removed" };

describe("lineDiff", () => {
  it("keeps equal texts as unchanged lines", () => {
    expect(lineDiff("a\nb\n", "a\nb\n")).toEqual([
      { kind: "same", text: "a", before: 1, after: 1 },
      { kind: "same", text: "b", before: 2, after: 2 },
    ]);
  });

  it("finds the smallest change, with line numbers on each side", () => {
    const lines = lineDiff("port = 80\nhost = a\nmode = dev\n", "port = 80\nhost = b\nmode = dev\n");
    expect(lines.map((l) => [l.kind, l.text, l.before, l.after])).toEqual([
      ["same", "port = 80", 1, 1],
      ["removed", "host = a", 2, undefined],
      ["added", "host = b", undefined, 2],
      ["same", "mode = dev", 3, 3],
    ]);
    expect(diffCounts(lines)).toEqual({ added: 1, removed: 1 });
  });

  it("handles empty sides and Windows line endings", () => {
    expect(lineDiff("", "x\r\ny")).toEqual([
      { kind: "added", text: "x", after: 1 },
      { kind: "added", text: "y", after: 2 },
    ]);
    expect(lineDiff("x\n", "")).toEqual([{ kind: "removed", text: "x", before: 1 }]);
    expect(splitLines("a\r\nb\r\n")).toEqual(["a", "b"]);
  });

  it("rebuilds both sides exactly from the diff", () => {
    const before = Array.from({ length: 400 }, (_, i) => `line ${i}`).join("\n");
    const after = before.replace("line 17", "line seventeen").replace("line 300\n", "") + "\nline 400";
    const lines = lineDiff(before, after);
    expect(lines.filter((l) => l.kind !== "added").map((l) => l.text)).toEqual(splitLines(before));
    expect(lines.filter((l) => l.kind !== "removed").map((l) => l.text)).toEqual(splitLines(after));
    expect(diffCounts(lines)).toEqual({ added: 2, removed: 2 });
  });
});

describe("Diff", () => {
  it("is a labelled region whose changed lines say what they are", () => {
    render(Diff, { label: "config.toml", before: "a\nb", after: "a\nc", ...words });
    const region = screen.getByRole("region", { name: "config.toml" });
    const items = within(region).getAllByRole("listitem");
    expect(items.map((item) => item.dataset.kind)).toEqual(["same", "removed", "added"]);
    expect(items[1]?.textContent).toContain("Removed: ");
    expect(items[2]?.textContent).toContain("Added: ");
  });

  it("shows its unchanged message when nothing differs", () => {
    const unchanged = createRawSnippet(() => ({ render: () => "<p>No changes</p>" }));
    render(Diff, { label: "notes", before: "same", after: "same", unchanged, ...words });
    expect(screen.getByText("No changes")).toBeTruthy();
    expect(screen.queryByRole("listitem")).toBeNull();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
