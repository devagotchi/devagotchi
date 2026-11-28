import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Card.svelte?raw";
import Card from "./Card.svelte";
import { meta } from "./Card.variants.ts";

const text = (words: string) => createRawSnippet(() => ({ render: () => `<p>${words}</p>` }));

/** An action with a real button, so its click can be observed. */
const action = (words: string, onclick: () => void) =>
  createRawSnippet(() => ({
    render: () => `<button type="button">${words}</button>`,
    setup: (button) => {
      button.addEventListener("click", onclick);
    },
  }));

describe("Card", () => {
  it("titles itself with a level-2 heading by default, with the default variant", () => {
    const { container } = render(Card, { title: "texas", description: "Windows 11, online", children: text("Two agents running.") });
    expect(screen.getByRole("heading", { level: 2, name: "texas" })).toBeTruthy();
    expect(screen.getByText("Windows 11, online")).toBeTruthy();
    expect(screen.getByText("Two agents running.")).toBeTruthy();
    expect(container.querySelector<HTMLElement>(".card")?.dataset.emphasis).toBe(meta.variants.emphasis.default);
  });

  it.each([2, 3, 4, 5, 6] as const)("uses heading level %i when asked", (headingLevel) => {
    render(Card, { title: "Machines", headingLevel });
    expect(screen.getByRole("heading", { level: headingLevel, name: "Machines" })).toBeTruthy();
  });

  it("has no heading and no header without a title, description or actions", () => {
    const { container } = render(Card, { children: text("Only content.") });
    expect(screen.queryByRole("heading")).toBeNull();
    expect(container.querySelector(".header")).toBeNull();
  });

  it("offers its actions after the title, and they work", async () => {
    const onclick = vi.fn();
    render(Card, { title: "texas", actions: action("Open", onclick) });
    const heading = screen.getByRole("heading", { name: "texas" });
    const button = screen.getByRole("button", { name: "Open" });
    expect(heading.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await userEvent.click(button);
    expect(onclick).toHaveBeenCalledOnce();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
