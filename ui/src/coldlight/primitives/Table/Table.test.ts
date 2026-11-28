import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Table.svelte?raw";
import Table from "./Table.svelte";
import { meta, type Column } from "./Table.variants.ts";

interface Machine {
  id: string;
  name: string;
  sessions: number;
}

const columns: Column[] = [
  { key: "name", label: "Machine" },
  { key: "sessions", label: "Sessions", align: "end" },
];

const rows: Machine[] = [
  { id: "m1", name: "texas-workstation", sessions: 3 },
  { id: "m2", name: "pixel-9", sessions: 1 },
];

const cell = createRawSnippet((row: () => Machine, key: () => string) => ({
  render: () => `<span>${String(row()[key() as keyof Machine])}</span>`,
}));

const empty = createRawSnippet(() => ({ render: () => "<span>No machines yet.</span>" }));

/** The table, its records typed as machines (render() cannot infer a generic component's T). */
const MachineTable = Table<Machine>;

const base = { caption: "Machines", columns, rows, rowKey: (row: Machine) => row.id, cell };

describe("Table", () => {
  it("is a table named by its caption, with column headers and one row per record", () => {
    render(MachineTable, base);
    const table = screen.getByRole("table", { name: "Machines" });
    const headers = within(table).getAllByRole("columnheader");
    expect(headers.map((th) => th.textContent)).toEqual(["Machine", "Sessions"]);
    expect(headers.every((th) => th.getAttribute("scope") === "col")).toBe(true);
    const bodyRows = within(table).getAllByRole("row").slice(1);
    expect(bodyRows.map((tr) => within(tr).getAllByRole("cell").map((td) => td.textContent))).toEqual([
      ["texas-workstation", "3"],
      ["pixel-9", "1"],
    ]);
  });

  it("sits in a focusable region named by the caption, so a keyboard can scroll it", async () => {
    render(MachineTable, base);
    const region = screen.getByRole("region", { name: "Machines" });
    expect(region.dataset.density).toBe(meta.variants.density.default);
    await userEvent.tab();
    expect(document.activeElement).toBe(region);
  });

  it("aligns a column to the end when asked", () => {
    render(MachineTable, base);
    expect(screen.getByRole("columnheader", { name: "Sessions" }).dataset.align).toBe("end");
    expect(screen.getByRole("cell", { name: "3" }).dataset.align).toBe("end");
    expect(screen.getByRole("columnheader", { name: "Machine" }).dataset.align).toBe("start");
  });

  it("can hide its caption from sight but keeps it as the name", () => {
    render(MachineTable, { ...base, captionHidden: true });
    const table = screen.getByRole("table", { name: "Machines" });
    expect(table.querySelector("caption")?.classList.contains("hidden")).toBe(true);
  });

  it("shows the empty state in one full-width row when there are no records", () => {
    render(MachineTable, { ...base, rows: [], empty });
    const only = screen.getByRole("cell", { name: "No machines yet." });
    expect(only.getAttribute("colspan")).toBe(String(columns.length));
  });

  it("shows no body rows and no empty row when there are no records and no empty state", () => {
    render(MachineTable, { ...base, rows: [] });
    expect(screen.getAllByRole("row")).toHaveLength(1);
  });

  it("takes the compact density", () => {
    render(MachineTable, { ...base, density: "compact" });
    expect(screen.getByRole("region").dataset.density).toBe("compact");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
