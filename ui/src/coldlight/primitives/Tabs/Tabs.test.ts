import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Tabs.svelte?raw";
import Tabs from "./Tabs.svelte";
import { meta, type Tab } from "./Tabs.variants.ts";

const panel = createRawSnippet((id: () => string) => ({ render: () => `<p>Panel ${id()}</p>` }));

const tabs: Tab[] = [
  { id: "overview", label: "Overview" },
  { id: "sessions", label: "Sessions" },
  { id: "gui", label: "GUI sessions", disabled: true },
  { id: "logs", label: "Logs" },
];

const base = { label: "Machine views", tabs, panel };

describe("Tabs", () => {
  it("is a labelled tablist whose first tab is chosen and in the Tab order", () => {
    render(Tabs, base);
    expect(screen.getByRole("tablist", { name: "Machine views" })).toBeTruthy();
    const overview = screen.getByRole("tab", { name: "Overview" });
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(overview.tabIndex).toBe(0);
    for (const name of ["Sessions", "Logs"]) {
      const other = screen.getByRole("tab", { name });
      expect(other.getAttribute("aria-selected")).toBe("false");
      expect(other.tabIndex).toBe(-1);
    }
  });

  it("wires the chosen tab and its panel to each other", () => {
    render(Tabs, { ...base, selected: "sessions" });
    const tab = screen.getByRole("tab", { name: "Sessions" });
    const tabpanel = screen.getByRole("tabpanel", { name: "Sessions" });
    expect(tab.getAttribute("aria-controls")).toBe(tabpanel.id);
    expect(tabpanel.getAttribute("aria-labelledby")).toBe(tab.id);
    expect(tabpanel.textContent).toBe("Panel sessions");
    expect(screen.getByRole("tab", { name: "Overview" }).hasAttribute("aria-controls")).toBe(false);
  });

  it("chooses a tab when it is pressed", async () => {
    render(Tabs, base);
    await userEvent.click(screen.getByRole("tab", { name: "Logs" }));
    expect(screen.getByRole("tab", { name: "Logs" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel").textContent).toBe("Panel logs");
  });

  it("moves and chooses with the arrow keys, Home and End, skipping disabled tabs and wrapping", async () => {
    render(Tabs, base);
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Overview" }));

    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Sessions" }));
    expect(screen.getByRole("tabpanel").textContent).toBe("Panel sessions");

    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Logs" }));

    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Overview" }));

    await userEvent.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Logs" }));

    await userEvent.keyboard("{Home}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Overview" }));

    await userEvent.keyboard("{End}");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Logs" }));
    expect(screen.getByRole("tabpanel").textContent).toBe("Panel logs");
  });

  it("leaves the tablist with one Tab press, into the panel", async () => {
    render(Tabs, base);
    await userEvent.tab();
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("tabpanel"));
  });

  it("cannot choose a disabled tab, and falls back from one", async () => {
    render(Tabs, { ...base, selected: "gui" });
    const gui = screen.getByRole("tab", { name: "GUI sessions" });
    expect(gui.hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("tab", { name: "Overview" }).getAttribute("aria-selected")).toBe("true");
    await userEvent.click(gui);
    expect(gui.getAttribute("aria-selected")).toBe("false");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
