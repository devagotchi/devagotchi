import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Badge",
  level: "primitive",
  summary: "A short label for the state of something: online, approved, waiting, failed.",
  use: "Beside a name or in a table cell, to say in a word or two what state a thing is in, as the API reports it.",
  avoid: "Explaining what happened and what to do next: use Notice. Anything that can be pressed: use Button.",
  variants: {
    tone: {
      description: "What kind of state it names. The words say it too; colour is never the only sign.",
      default: "neutral",
      values: {
        neutral: "A plain fact with no judgement: a channel, a kind, a count.",
        accent: "Selected, current or yours: the board you are on, your own agent.",
        success: "Done, online, allowed.",
        warning: "Needs attention but nothing failed: offline, expiring.",
        pending: "Waiting on someone or something: an approval, a running job.",
        danger: "Failed, refused or revoked.",
      },
    },
  },
  props: [
    { name: "children", type: "Snippet", required: true, description: "The state, in a word or two (Online, Waiting for approval)." },
    { name: "tone", type: "'neutral' | 'accent' | 'success' | 'warning' | 'pending' | 'danger'", description: "See the tone variants." },
  ],
  a11y: "Plain inline text, read in its place: it has no role and takes no focus. Its words carry the meaning, so a screen reader and a colour-blind reader get the same state.",
  example: '<Badge tone="success">Online</Badge>',
} as const satisfies ComponentMeta;

export type BadgeTone = keyof typeof meta.variants.tone.values;
