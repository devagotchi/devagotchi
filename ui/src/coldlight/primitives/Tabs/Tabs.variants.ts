import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Tabs",
  level: "primitive",
  summary: "A row of tabs that shows one of several panels of the same thing at a time.",
  use: "Views of one object that a person moves between freely: a machine's overview, sessions and logs; the tabs of a settings domain.",
  avoid: "Steps that must happen in order: use a form with sections. Going to another page: use links. Choosing a value for a form: use Select.",
  variants: {},
  props: [
    { name: "label", type: "string", required: true, description: "The accessible name of the row of tabs (Machine views)." },
    { name: "tabs", type: "Array<{ id: string; label: string; disabled?: boolean }>", required: true, description: "The tabs, in order. A disabled tab is shown but cannot be chosen." },
    { name: "selected", type: "string", description: "The id of the chosen tab. Bindable. Without one, the first tab that is not disabled is chosen." },
    { name: "panel", type: "Snippet<[string]>", required: true, description: "Renders the panel of the chosen tab, given its id." },
  ],
  a11y: "The WAI-ARIA tabs pattern with automatic activation: one tab is in the Tab order (roving tabindex); ArrowLeft and ArrowRight move to the previous and next tab and choose it, Home and End the first and last; disabled tabs are skipped. The chosen tab has aria-selected and aria-controls its panel; the panel is labelled by its tab. Each tab is at least 48 dp tall. The row scrolls sideways inside itself on a narrow screen.",
  example: '<Tabs label={t.machineViews} {tabs} bind:selected>{#snippet panel(id)}<MachineView {id} />{/snippet}</Tabs>',
} as const satisfies ComponentMeta;

export interface Tab {
  id: string;
  label: string;
  disabled?: boolean;
}
