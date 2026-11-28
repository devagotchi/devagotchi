import { render, screen, within } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./List.svelte?raw";
import List from "./List.svelte";
import { meta } from "./List.variants.ts";

interface Machine {
  id: string;
  name: string;
}

const machines: Machine[] = [
  { id: "m1", name: "texas" },
  { id: "m2", name: "platinum" },
  { id: "m3", name: "phone" },
];

const row = createRawSnippet<[Machine]>((machine) => ({ render: () => `<span>${machine().name}</span>` }));
const nothing = createRawSnippet(() => ({ render: () => `<p>No machines yet. Connect one to start.</p>` }));
const key = (machine: Machine) => machine.id;

describe("List", () => {
  it("is a list named by its label, one item per entry in data order, with the default variant", () => {
    render(List<Machine>, { items: machines, item: row, key, label: "Machines" });
    const list = screen.getByRole("list", { name: "Machines" });
    expect(list.tagName).toBe("UL");
    expect(within(list).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["texas", "platinum", "phone"]);
    expect(list.dataset.density).toBe(meta.variants.density.default);
  });

  it("follows its data, keeping rows by key", async () => {
    const { rerender } = render(List<Machine>, { items: machines, item: row, key, label: "Machines" });
    const texas = screen.getByText("texas").closest("li");
    await rerender({ items: [machines[2]!, machines[0]!] });
    const items = screen.getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual(["phone", "texas"]);
    expect(items[1]).toBe(texas);
  });

  it("shows its empty state, and no empty list, when there are no items", () => {
    render(List<Machine>, { items: [], item: row, key, label: "Machines", empty: nothing });
    expect(screen.getByText("No machines yet. Connect one to start.")).toBeTruthy();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("carries the density it is given", () => {
    render(List<Machine>, { items: machines, item: row, key, label: "Activity", density: "compact" });
    expect(screen.getByRole("list", { name: "Activity" }).dataset.density).toBe("compact");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
