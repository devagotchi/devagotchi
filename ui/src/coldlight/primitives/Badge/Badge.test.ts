import { render, screen } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Badge.svelte?raw";
import Badge from "./Badge.svelte";
import { meta } from "./Badge.variants.ts";

const text = (words: string) => createRawSnippet(() => ({ render: () => `<span>${words}</span>` }));

describe("Badge", () => {
  it("shows its words with the default tone", () => {
    render(Badge, { children: text("Online") });
    const badge = screen.getByText("Online").closest<HTMLElement>(".badge");
    expect(badge?.dataset.tone).toBe(meta.variants.tone.default);
  });

  it("carries the tone it is given", () => {
    render(Badge, { children: text("Refused"), tone: "danger" });
    expect(screen.getByText("Refused").closest<HTMLElement>(".badge")?.dataset.tone).toBe("danger");
  });

  it("is plain text: no role and no focus", () => {
    const { container } = render(Badge, { children: text("Waiting"), tone: "pending" });
    const badge = container.querySelector<HTMLElement>(".badge");
    expect(badge?.hasAttribute("role")).toBe(false);
    expect(badge?.hasAttribute("tabindex")).toBe(false);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
