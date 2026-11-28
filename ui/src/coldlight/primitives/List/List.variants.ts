import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "List",
  level: "primitive",
  summary: "A vertical list of items drawn from data, one row each, with dividers, and an empty state when there are none.",
  use: "Machines, agents, notes, recent activity: things that read one per line and are each the same shape. Give it an empty snippet that says what would be here and how to add one (U7).",
  avoid: "Rows compared column by column: use Table. A few unrelated things: use Cards.",
  variants: {
    density: {
      description: "How much room each row has.",
      default: "comfortable",
      values: {
        comfortable: "Room to read and to touch: the usual list.",
        compact: "Tight rows for long lists of short items, such as an activity feed.",
      },
    },
  },
  props: [
    { name: "items", type: "T[]", required: true, description: "The items, in the order the API gave them." },
    { name: "item", type: "Snippet<[T]>", required: true, description: "How one item is drawn." },
    { name: "key", type: "(item: T) => string", required: true, description: "Each item's stable identity, usually its id, so rows keep their state when the list changes." },
    { name: "label", type: "string", required: true, description: "What the list is (Machines), read out as the list's name." },
    { name: "empty", type: "Snippet", description: "Shown in place of the list when there are no items: what would be here and how to add one." },
    { name: "density", type: "'comfortable' | 'compact'", description: "See the density variants." },
  ],
  a11y: "A native <ul> named by aria-label, so a screen reader says what the list is and how many items it has. Each item is an <li> in data order. When empty, the empty snippet is shown instead and no empty list is announced.",
  example: '<List items={machines} key={(m) => m.id} label="Machines">{#snippet item(m)}{m.name}{/snippet}</List>',
} as const satisfies ComponentMeta;

export type ListDensity = keyof typeof meta.variants.density.values;
