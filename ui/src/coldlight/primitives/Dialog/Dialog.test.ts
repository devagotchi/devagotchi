import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Dialog.svelte?raw";
import Dialog from "./Dialog.svelte";
import { meta } from "./Dialog.variants.ts";

const text = (html: string) => createRawSnippet(() => ({ render: () => html }));

// jsdom has no showModal(): open and close only set the attribute, as the browser does.
const original = {
  showModal: HTMLDialogElement.prototype.showModal,
  close: HTMLDialogElement.prototype.close,
};
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
});
afterAll(() => {
  HTMLDialogElement.prototype.showModal = original.showModal;
  HTMLDialogElement.prototype.close = original.close;
});

const base = {
  title: "Revoke this token?",
  description: "Agents using it lose access at once.",
  closeLabel: "Close",
  children: text("<p>Two agents use it.</p>"),
};

describe("Dialog", () => {
  it("opens as a dialog named by its title and described by its description", () => {
    render(Dialog, { ...base, open: true });
    const dialog = screen.getByRole("dialog", { name: base.title });
    expect(dialog.hasAttribute("open")).toBe(true);
    expect(dialog.dataset.tone).toBe(meta.variants.tone.default);
    const describedBy = dialog.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(describedBy)?.textContent).toBe(base.description);
    expect(screen.getByRole("heading", { level: 2, name: base.title })).toBeTruthy();
  });

  it("stays closed until open is set", () => {
    render(Dialog, { ...base });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("has no description when none is given", () => {
    render(Dialog, { ...base, description: undefined, open: true });
    expect(screen.getByRole("dialog").hasAttribute("aria-describedby")).toBe(false);
  });

  it("closes by its close button, named by closeLabel, and calls onclose", async () => {
    const onclose = vi.fn();
    render(Dialog, { ...base, open: true, onclose });
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("closes by Escape (the browser's cancel event) and calls onclose", async () => {
    const onclose = vi.fn();
    render(Dialog, { ...base, open: true, onclose });
    const dialog = screen.getByRole("dialog");
    const cancel = new Event("cancel", { cancelable: true });
    await fireEvent(dialog, cancel);
    expect(cancel.defaultPrevented).toBe(true);
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("follows a dialog the browser closed on its own (a form with method=dialog)", async () => {
    const onclose = vi.fn();
    render(Dialog, { ...base, open: true, onclose });
    const dialog = screen.getByRole("dialog");
    dialog.removeAttribute("open");
    await fireEvent(dialog, new Event("close"));
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("closes without calling onclose when the owner sets open to false", async () => {
    const onclose = vi.fn();
    const { rerender } = render(Dialog, { ...base, open: true, onclose });
    await rerender({ open: false });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onclose).not.toHaveBeenCalled();
  });

  it("renders its actions in the footer", () => {
    render(Dialog, { ...base, open: true, actions: text("<button>Revoke</button>") });
    expect(screen.getByRole("button", { name: "Revoke" })).toBeTruthy();
  });

  it("marks a destructive confirmation with the danger tone", () => {
    render(Dialog, { ...base, open: true, tone: "danger" });
    expect(screen.getByRole("dialog").dataset.tone).toBe("danger");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
