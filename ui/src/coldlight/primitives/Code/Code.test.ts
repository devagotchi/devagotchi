import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Code.svelte?raw";
import Code from "./Code.svelte";
import { meta } from "./Code.variants.ts";

const command = "devagotchi hub serve --config hub.toml";

/** userEvent.setup() installs its own clipboard; this replaces it with one the test controls. */
function withClipboard(writeText: (text: string) => Promise<void>) {
  const user = userEvent.setup();
  const spy = vi.fn(writeText);
  Object.defineProperty(navigator, "clipboard", { value: { writeText: spy }, configurable: true });
  return { user, writeText: spy };
}

describe("Code", () => {
  it("is a block by default: pre and code, exactly as written, focusable to scroll", async () => {
    const { container } = render(Code, { code: command, language: "bash" });
    const wrapper = container.querySelector(".code") as HTMLElement;
    expect(wrapper.dataset.display).toBe(meta.variants.display.default);
    expect(wrapper.dataset.language).toBe("bash");
    const pre = container.querySelector("pre") as HTMLElement;
    expect(pre.querySelector("code")?.textContent).toBe(command);
    expect(screen.getByText("bash")).toBeTruthy();
    await userEvent.tab();
    expect(document.activeElement).toBe(pre);
  });

  it("is a bare code inside a sentence when inline, with its language only as data", () => {
    const { container } = render(Code, { code: "hub.toml", language: "toml", display: "inline" });
    const wrapper = container.querySelector(".code") as HTMLElement;
    expect(wrapper.tagName).toBe("SPAN");
    expect(wrapper.dataset.display).toBe("inline");
    expect(wrapper.dataset.language).toBe("toml");
    expect(container.querySelector("pre")).toBeNull();
    expect(wrapper.querySelector("code")?.textContent).toBe("hub.toml");
    expect(screen.queryByText("toml")).toBeNull();
  });

  it("has no copy button or live region unless their labels are given", () => {
    render(Code, { code: command });
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("copies to the clipboard and announces it politely", async () => {
    const { user, writeText } = withClipboard(() => Promise.resolve());
    render(Code, { code: command, copyLabel: "Copy", copiedLabel: "Copied" });
    const status = screen.getByRole("status");
    expect(status.getAttribute("aria-live")).toBe("polite");
    expect(status.textContent).toBe("");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith(command);
    await waitFor(() => expect(status.textContent).toBe("Copied"));
  });

  it("is copied no longer once the code changes", async () => {
    const { user } = withClipboard(() => Promise.resolve());
    const { rerender } = render(Code, { code: command, copyLabel: "Copy", copiedLabel: "Copied" });
    await user.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("Copied"));
    await rerender({ code: "devagotchi hub stop" });
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("selects the text to copy by hand when the clipboard refuses, and says so", async () => {
    const { user } = withClipboard(() => Promise.reject(new Error("denied")));
    const failed = "Copy it by hand; the text is selected";
    render(Code, { code: command, copyLabel: "Copy", copiedLabel: "Copied", copyFailedLabel: failed });
    await user.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() => expect(getSelection()?.toString()).toBe(command));
    expect(screen.getByRole("status").textContent).toBe(failed);
  });

  it("offers copy inline too", async () => {
    const { user, writeText } = withClipboard(() => Promise.resolve());
    render(Code, { code: "dg_7f3a", display: "inline", copyLabel: "Copy", copiedLabel: "Copied" });
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith("dg_7f3a");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
