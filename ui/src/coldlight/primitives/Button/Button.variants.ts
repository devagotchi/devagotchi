import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Button",
  level: "primitive",
  summary: "A control that does one thing when pressed.",
  use: "Any action: save, approve, cancel, open a dialog. A screen has at most one solid accent button, its main action.",
  avoid: "Going to another page: use a link. Choosing one of several options: use Tabs or Select.",
  variants: {
    tone: {
      description: "What kind of action it is.",
      default: "neutral",
      values: {
        neutral: "An ordinary action.",
        accent: "The main action of the screen.",
        danger: "An action that destroys or refuses something: delete, revoke, deny.",
      },
    },
    emphasis: {
      description: "How loudly it asks to be pressed.",
      default: "outline",
      values: {
        solid: "Filled with its tone: the one action the screen is for.",
        outline: "Edged: a normal action beside others.",
        quiet: "Text only: a minor or repeated action, such as one in every row of a table.",
      },
    },
  },
  props: [
    { name: "children", type: "Snippet", required: true, description: "The label: a verb for what it does (Save, Approve)." },
    { name: "tone", type: "'neutral' | 'accent' | 'danger'", description: "See the tone variants." },
    { name: "emphasis", type: "'solid' | 'outline' | 'quiet'", description: "See the emphasis variants." },
    { name: "busy", type: "boolean", description: "The action is running: the button is disabled and says so to screen readers." },
    { name: "disabled", type: "boolean", description: "The action cannot run now. Say why nearby." },
    { name: "type", type: "'button' | 'submit' | 'reset'", description: "Defaults to button; a form's own button is submit." },
    { name: "onclick", type: "(event: MouseEvent) => void", description: "What pressing it does." },
  ],
  a11y: "A native <button>: Enter and Space press it, and it is labelled by its text. When busy it carries aria-busy. It is at least 48 dp in both directions.",
  example: '<Button tone="accent" emphasis="solid" onclick={save}>Save</Button>',
} as const satisfies ComponentMeta;

export type ButtonTone = keyof typeof meta.variants.tone.values;
export type ButtonEmphasis = keyof typeof meta.variants.emphasis.values;
