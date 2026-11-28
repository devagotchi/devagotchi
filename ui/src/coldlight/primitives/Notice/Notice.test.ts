import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Notice.svelte?raw";
import Notice from "./Notice.svelte";
import { meta, type NoticeTone } from "./Notice.variants.ts";

const text = (words: string) => createRawSnippet(() => ({ render: () => `<p>${words}</p>` }));

/** A next step with a real button, so its click can be observed. */
const fix = (words: string, onclick: () => void) =>
  createRawSnippet(() => ({
    render: () => `<button type="button">${words}</button>`,
    setup: (button) => {
      button.addEventListener("click", onclick);
    },
  }));

describe("Notice", () => {
  it("shows what happened and why, with the default tone, and is static by default", () => {
    const { container } = render(Notice, { title: "Deploy refused", children: text("Port 80 needs administrator rights.") });
    expect(screen.getByText("Deploy refused")).toBeTruthy();
    expect(screen.getByText("Port 80 needs administrator rights.")).toBeTruthy();
    const notice = container.querySelector<HTMLElement>(".notice");
    expect(notice?.dataset.tone).toBe(meta.variants.tone.default);
    expect(notice?.hasAttribute("role")).toBe(false);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it.each<[NoticeTone, string]>([
    ["danger", "alert"],
    ["warning", "alert"],
    ["success", "status"],
    ["info", "status"],
  ])("announces a live %s notice as %s", (tone, role) => {
    render(Notice, { title: "Something happened", children: text("Why it did."), tone, live: true });
    expect(screen.getByRole(role).textContent).toContain("Something happened");
  });

  it("offers the next step, and the step works", async () => {
    const onclick = vi.fn();
    render(Notice, {
      title: "Deploy refused",
      tone: "danger",
      children: text("Port 80 needs administrator rights."),
      next: fix("Use port 8080", onclick),
    });
    await userEvent.click(screen.getByRole("button", { name: "Use port 8080" }));
    expect(onclick).toHaveBeenCalledOnce();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
