import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Field",
  level: "primitive",
  summary: "A labelled text input, or a labelled text area when it takes more than one line.",
  use: "Any free text a person types: a name, an email, a URL, a search, a number, a note. Give it a hint when the format is not obvious, and an error when the value is refused.",
  avoid: "Choosing one of a known set of values: use Select. A yes or no: use a checkbox.",
  variants: {
    width: {
      description: "How much of the line the control takes.",
      default: "fill",
      values: {
        fill: "The whole width of its container: the usual field in a form.",
        fit: "Only as wide as its content needs: a short value such as a port or a count, beside other controls.",
      },
    },
  },
  props: [
    { name: "label", type: "string", required: true, description: "What the field is for, as a noun (Email, Machine name). Mark a required field in the label's words." },
    { name: "value", type: "string | number | null", description: "The text in the field. Bind it: bind:value. A number field binds a number." },
    { name: "hint", type: "string", description: "How to fill it in: the format, a limit, an example. Read out after the label." },
    { name: "error", type: "string", description: "Why the value is refused and how to fix it, from the API's refusal. When set, the field is marked invalid and the message is read out." },
    { name: "multiline", type: "boolean", description: "A text area for more than one line, instead of a single-line input." },
    { name: "type", type: "'text' | 'email' | 'password' | 'url' | 'search' | 'number'", description: "The kind of text, which picks the phone keyboard. Ignored when multiline." },
    { name: "width", type: "'fill' | 'fit'", description: "See the width variants." },
    { name: "required", type: "boolean", description: "The form cannot be sent without it." },
    { name: "disabled", type: "boolean", description: "It cannot be changed now. Say why in the hint." },
    { name: "name", type: "string", description: "The form field's name." },
    { name: "autocomplete", type: "string", description: "What the browser may fill in: email, username, current-password, off." },
  ],
  a11y: "A native <input> or <textarea> with a <label> bound to it, so the label is its accessible name and clicking the label focuses it. The hint and the error are linked with aria-describedby; an error also sets aria-invalid. The control is at least 48 dp tall.",
  example: '<Field label="Email" type="email" autocomplete="email" hint="We send the sign-in link here." bind:value={email} />',
} as const satisfies ComponentMeta;

export type FieldWidth = keyof typeof meta.variants.width.values;
export type FieldType = "text" | "email" | "password" | "url" | "search" | "number";
