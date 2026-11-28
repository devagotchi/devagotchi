import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./ConflictResolver.svelte?raw";
import ConflictResolver from "./ConflictResolver.svelte";
import { meta, type ConflictLabels } from "./ConflictResolver.variants.ts";

// jsdom has no modal dialogs; these stand in for the browser's, in this file only.
const native = { show: HTMLDialogElement.prototype.showModal, close: HTMLDialogElement.prototype.close };
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterAll(() => {
  HTMLDialogElement.prototype.showModal = native.show;
  HTMLDialogElement.prototype.close = native.close;
});

const labels: ConflictLabels = {
  legend: "Which version should be written?",
  existing: "Keep what is there",
  existingHint: "Your change is dropped.",
  incoming: "Use your change",
  incomingHint: "The newer version is replaced.",
  merged: "Merge both",
  mergedHint: "Both changes are kept.",
  preview: "What will be written",
  apply: "Write it",
  cancel: "Cancel",
  close: "Close",
  added: "Added",
  removed: "Removed",
  noChange: "Nothing changes.",
  counts: (added, removed) => `${added} added, ${removed} removed`,
};

const existing = "port = 8080\nmode = dev\n";
const incoming = "port = 8443\nmode = dev\n";
const merged = "port = 8443\nmode = dev\ninvites = true\n";

function open(props: Partial<{ merged: string }> = { merged }) {
  const onresolve = vi.fn();
  const oncancel = vi.fn();
  render(ConflictResolver, {
    open: true,
    title: "hub.toml changed while you were editing",
    name: "hub.toml",
    existing,
    incoming,
    ...props,
    labels,
    onresolve,
    oncancel,
  });
  return { onresolve, oncancel, dialog: screen.getByRole("dialog", { name: "hub.toml changed while you were editing" }) };
}

describe("ConflictResolver", () => {
  it("offers the three choices with their counts, the incoming change selected", () => {
    const { dialog } = open();
    const group = within(dialog).getByRole("group", { name: labels.legend });
    const radios = within(group).getAllByRole("radio");
    expect(radios.map((radio) => (radio as HTMLInputElement).value)).toEqual(["existing", "incoming", "merged"]);
    expect(within(group).getByRole("radio", { name: labels.incoming })).toHaveProperty("checked", true);
    expect(within(group).getByText("2 added, 1 removed")).toBeTruthy();
    expect(within(group).getByText("0 added, 0 removed")).toBeTruthy();
  });

  it("previews the selected choice against what is there, in red and green", async () => {
    const { dialog } = open();
    const preview = within(dialog).getByRole("region", { name: "hub.toml" });
    expect(within(preview).getAllByRole("listitem").map((item) => item.dataset.kind)).toEqual([
      "removed",
      "added",
      "same",
    ]);
    await userEvent.click(within(dialog).getByRole("radio", { name: labels.existing }));
    expect(within(dialog).getByText(labels.noChange)).toBeTruthy();
  });

  it("writes the chosen text when applied", async () => {
    const { dialog, onresolve } = open();
    await userEvent.click(within(dialog).getByRole("radio", { name: labels.merged }));
    await userEvent.click(within(dialog).getByRole("button", { name: labels.apply }));
    expect(onresolve).toHaveBeenCalledWith("merged", merged);
  });

  it("never offers a merge the engine did not make", () => {
    const { dialog } = open({});
    expect(within(dialog).queryByRole("radio", { name: labels.merged })).toBeNull();
  });

  it("cancels without writing", async () => {
    const { dialog, onresolve, oncancel } = open();
    await userEvent.click(within(dialog).getByRole("button", { name: labels.cancel }));
    expect(oncancel).toHaveBeenCalledOnce();
    expect(onresolve).not.toHaveBeenCalled();
  });

  it("declares no variants, and styles none", () => {
    expectVariantsStyled(meta, source);
  });
});
