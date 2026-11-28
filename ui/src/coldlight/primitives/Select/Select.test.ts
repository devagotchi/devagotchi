import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { expectVariantsStyled } from "../../testing/variants.ts";
import source from "./Select.svelte?raw";
import Select from "./Select.svelte";
import { meta, type SelectOption } from "./Select.variants.ts";

const regions: SelectOption[] = [
  { value: "eu", label: "Europe" },
  { value: "us", label: "United States" },
  { value: "ap", label: "Asia Pacific", disabled: true },
];

describe("Select", () => {
  it("is a combobox named by its label, offering every option, with the default variant", () => {
    const { container } = render(Select, { label: "Region", options: regions });
    const select = screen.getByRole("combobox", { name: "Region" });
    expect(select.tagName).toBe("SELECT");
    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual(["Europe", "United States", "Asia Pacific"]);
    expect(screen.getByRole<HTMLOptionElement>("option", { name: "Asia Pacific" }).disabled).toBe(true);
    expect(select.hasAttribute("aria-invalid")).toBe(false);
    expect(container.querySelector<HTMLElement>(".select")?.dataset.width).toBe(meta.variants.width.default);
  });

  it("shows the bound value and binds what is chosen", async () => {
    let value: string | undefined = "us";
    render(Select, {
      props: {
        label: "Region",
        options: regions,
        get value() {
          return value;
        },
        set value(next) {
          value = next;
        },
      },
    });
    const select = screen.getByRole<HTMLSelectElement>("combobox", { name: "Region" });
    expect(select.value).toBe("us");
    await userEvent.selectOptions(select, "eu");
    expect(value).toBe("eu");
  });

  it("reads out its hint, and marks itself invalid with the error read out too", () => {
    render(Select, { label: "Role", options: regions, hint: "Decides what they can approve.", error: "Only an owner can grant this role." });
    const select = screen.getByRole("combobox", { name: "Role" });
    expect(select.getAttribute("aria-invalid")).toBe("true");
    const described = (select.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent);
    expect(described).toEqual(["Decides what they can approve.", "Only an owner can grant this role."]);
  });

  it("passes required, disabled and name to the control", () => {
    render(Select, { label: "Channel", options: regions, required: true, disabled: true, name: "channel" });
    const select = screen.getByRole<HTMLSelectElement>("combobox", { name: "Channel" });
    expect(select.required).toBe(true);
    expect(select.disabled).toBe(true);
    expect(select.name).toBe("channel");
  });

  it("styles every variant it declares, and no other", () => {
    expectVariantsStyled(meta, source);
  });
});
