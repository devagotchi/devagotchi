import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Card",
  level: "primitive",
  summary: "A surface that groups one thing: its title, a line about it, its actions and its content.",
  use: "One machine, one agent, one setting group, one dashboard figure: anything a screen shows several of side by side or one after another.",
  avoid: "A row in a long list: use List or Table. A message about a failure: use Notice. Content that must be answered before going on: use Dialog.",
  variants: {
    emphasis: {
      description: "How far it stands off the canvas.",
      default: "flat",
      values: {
        flat: "Edged and level with the page: the usual card, many to a screen.",
        raised: "Lifted with a shadow: the one card a screen is about, or one that can be picked up.",
      },
    },
  },
  props: [
    { name: "title", type: "string", description: "What the card is about, as a heading." },
    { name: "headingLevel", type: "2 | 3 | 4 | 5 | 6", description: "The title's heading level, so it fits the page's outline. Defaults to 2." },
    { name: "description", type: "string", description: "One line about it, under the title." },
    { name: "actions", type: "Snippet", description: "Buttons for the whole card: beside the title on a wide card, under it on a narrow one." },
    { name: "children", type: "Snippet", description: "The content." },
    { name: "emphasis", type: "'flat' | 'raised'", description: "See the emphasis variants." },
  ],
  a11y: "A plain group: the title is a real heading at the level given, so screen reader users can jump between cards by heading. Actions come after the title in reading and Tab order.",
  example: '<Card title="texas" description="Windows 11, online">{#snippet actions()}<Button>Open</Button>{/snippet}…</Card>',
} as const satisfies ComponentMeta;

export type CardEmphasis = keyof typeof meta.variants.emphasis.values;
export type CardHeadingLevel = 2 | 3 | 4 | 5 | 6;
