import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Field.svelte?raw";
import Field from "./Field.svelte";
import { meta } from "./Field.variants.ts";

describe("Field", () => {
  it("is a text box named by its label, with the default variant", () => {
    const { container } = render(Field, { label: "Machine name" });
    const input = screen.getByRole("textbox", { name: "Machine name" });
    expect(input.getAttribute("type")).toBe("text");
    expect(input.hasAttribute("aria-invalid")).toBe(false);
    expect(input.hasAttribute("aria-describedby")).toBe(false);
    expect(container.querySelector<HTMLElement>(".field")?.dataset.width).toBe(meta.variants.width.default);
  });

  it("focuses its control when the label is clicked", async () => {
    render(Field, { label: "Email", type: "email" });
    await userEvent.click(screen.getByText("Email"));
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Email" }));
  });

  it("binds what is typed", async () => {
    let value: string | number | null | undefined = "ab";
    render(Field, {
      props: {
        label: "Note",
        get value() {
          return value;
        },
        set value(next) {
          value = next;
        },
      },
    });
    const input = screen.getByRole("textbox", { name: "Note" });
    expect((input as HTMLInputElement).value).toBe("ab");
    await userEvent.type(input, "cd");
    expect(value).toBe("abcd");
  });

  it("reads out its hint, and marks itself invalid with the error read out too", () => {
    render(Field, { label: "Port", hint: "Between 1024 and 65535.", error: "Port 80 needs root; choose 8080." });
    const input = screen.getByRole("textbox", { name: "Port" });
    expect(input.getAttribute("aria-invalid")).toBe("true");
    const described = (input.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent);
    expect(described).toEqual(["Between 1024 and 65535.", "Port 80 needs root; choose 8080."]);
  });

  it("keeps a describedby the caller gave it", () => {
    render(Field, { label: "Token", hint: "Shown once.", "aria-describedby": "elsewhere" });
    const input = screen.getByRole("textbox", { name: "Token" });
    expect(input.getAttribute("aria-describedby")?.split(" ")[0]).toBe("elsewhere");
  });

  it("gives every field its own ids", () => {
    render(Field, { label: "First", hint: "One." });
    render(Field, { label: "Second", hint: "Two." });
    const first = screen.getByRole("textbox", { name: "First" });
    const second = screen.getByRole("textbox", { name: "Second" });
    expect(first.id).not.toBe(second.id);
    expect(first.getAttribute("aria-describedby")).not.toBe(second.getAttribute("aria-describedby"));
  });

  it("is a text area when multiline", async () => {
    render(Field, { label: "Description", multiline: true });
    const area = screen.getByRole("textbox", { name: "Description" });
    expect(area.tagName).toBe("TEXTAREA");
    await userEvent.type(area, "one{Enter}two");
    expect((area as HTMLTextAreaElement).value).toBe("one\ntwo");
  });

  it("passes required, disabled, name and autocomplete to the control", async () => {
    render(Field, { label: "Password", type: "password", required: true, disabled: true, name: "password", autocomplete: "current-password" });
    const input = screen.getByLabelText("Password") as HTMLInputElement;
    expect(input.type).toBe("password");
    expect(input.required).toBe(true);
    expect(input.disabled).toBe(true);
    expect(input.name).toBe("password");
    expect(input.getAttribute("autocomplete")).toBe("current-password");
    await userEvent.type(input, "secret");
    expect(input.value).toBe("");
  });

  it("is a spin button when the type is number", () => {
    render(Field, { label: "Count", type: "number" });
    expect(screen.getByRole("spinbutton", { name: "Count" })).toBeTruthy();
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
