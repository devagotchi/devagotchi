import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Select",
  level: "primitive",
  summary: "A labelled choice of one value from a known list, on the platform's own picker.",
  use: "Choosing one of a handful or more of options that come from the API's configuration: a region, a role, a channel.",
  avoid: "Two to four options that should all be visible: use radio buttons or Tabs. Free text: use Field.",
  variants: {
    width: {
      description: "How much of the line the control takes.",
      default: "fill",
      values: {
        fill: "The whole width of its container: the usual select in a form.",
        fit: "Only as wide as its longest option: a filter beside other controls.",
      },
    },
  },
  props: [
    { name: "label", type: "string", required: true, description: "What is being chosen, as a noun (Region, Role)." },
    { name: "options", type: "Array<{ value: string; label: string; disabled?: boolean }>", required: true, description: "The choices, in order, from the API's configuration. A disabled option is shown but cannot be chosen." },
    { name: "value", type: "string", description: "The chosen option's value. Bind it: bind:value." },
    { name: "hint", type: "string", description: "What the choice changes. Read out after the label." },
    { name: "error", type: "string", description: "Why the choice is refused and what to choose instead. When set, the select is marked invalid and the message is read out." },
    { name: "width", type: "'fill' | 'fit'", description: "See the width variants." },
    { name: "required", type: "boolean", description: "The form cannot be sent without a choice." },
    { name: "disabled", type: "boolean", description: "It cannot be changed now. Say why in the hint." },
    { name: "name", type: "string", description: "The form field's name." },
  ],
  a11y: "A native <select> with a <label> bound to it, so the label is its accessible name and the platform's picker, keyboard and screen reader handling all apply. The hint and the error are linked with aria-describedby; an error also sets aria-invalid. The control is at least 48 dp tall.",
  example: '<Select label="Region" options={regions} bind:value={region} />',
} as const satisfies ComponentMeta;

export type SelectWidth = keyof typeof meta.variants.width.values;

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}
