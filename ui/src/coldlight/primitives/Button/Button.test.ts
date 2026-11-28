import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Button.svelte?raw";
import Button from "./Button.svelte";
import { meta } from "./Button.variants.ts";

const label = (text: string) => createRawSnippet(() => ({ render: () => `<span>${text}</span>` }));

describe("Button", () => {
  it("is a button labelled by its text, with the default variants", () => {
    render(Button, { children: label("Save") });
    const button = screen.getByRole("button", { name: "Save" });
    expect(button.getAttribute("type")).toBe("button");
    expect(button.dataset.tone).toBe(meta.variants.tone.default);
    expect(button.dataset.emphasis).toBe(meta.variants.emphasis.default);
  });

  it("does its action when pressed, by pointer or keyboard", async () => {
    const onclick = vi.fn();
    render(Button, { children: label("Approve"), onclick });
    const button = screen.getByRole("button", { name: "Approve" });
    await userEvent.click(button);
    button.focus();
    await userEvent.keyboard("{Enter}");
    expect(onclick).toHaveBeenCalledTimes(2);
  });

  it("is disabled and says so while busy", async () => {
    const onclick = vi.fn();
    render(Button, { children: label("Deploy"), busy: true, onclick });
    const button = screen.getByRole("button", { name: "Deploy" });
    expect(button.hasAttribute("disabled")).toBe(true);
    expect(button.getAttribute("aria-busy")).toBe("true");
    await userEvent.click(button);
    expect(onclick).not.toHaveBeenCalled();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
