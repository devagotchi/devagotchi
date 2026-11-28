import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Dialog",
  level: "primitive",
  summary: "A modal window over the page that holds one task or one question until it is answered or closed.",
  use: "A confirmation before something hard to undo, a short form that belongs to the current screen, or a choice the screen cannot go on without.",
  avoid: "Telling someone something happened: use Notice. Long or multi-step work: give it its own page.",
  variants: {
    tone: {
      description: "What kind of question it asks.",
      default: "neutral",
      values: {
        neutral: "An ordinary task or question.",
        danger: "A confirmation before something destructive: delete, revoke, deny. Its top edge and title carry the danger colour.",
      },
    },
  },
  props: [
    { name: "open", type: "boolean", description: "Whether it is showing. Bindable: closing it by Escape or the close button sets it to false." },
    { name: "title", type: "string", required: true, description: "The heading, and the dialog's accessible name." },
    { name: "description", type: "string", description: "One sentence under the title that says what is being asked; it becomes the accessible description." },
    { name: "children", type: "Snippet", required: true, description: "The body: the form or the details." },
    { name: "actions", type: "Snippet", description: "The footer buttons. The main action goes last." },
    { name: "closeLabel", type: "string", required: true, description: "The accessible name of the close button (Close)." },
    { name: "tone", type: "'neutral' | 'danger'", description: "See the tone variants." },
    { name: "onclose", type: "() => void", description: "Called when the person closes it by Escape or the close button." },
  ],
  a11y: "A native <dialog> opened with showModal(): it sits in the top layer, keeps focus inside, and Escape closes it. It is named by its title (aria-labelledby) and described by its description (aria-describedby). The close button is at least 48 dp.",
  example: '<Dialog bind:open title={t.revokeTitle} closeLabel={t.close} tone="danger">{t.revokeBody}</Dialog>',
} as const satisfies ComponentMeta;

export type DialogTone = keyof typeof meta.variants.tone.values;
