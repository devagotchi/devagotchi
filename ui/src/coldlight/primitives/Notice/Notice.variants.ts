import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Notice",
  level: "primitive",
  summary: "An inline message that says what happened, why, and what to do next (U9).",
  use: "A refusal or failure from the API, shown from its {code, message, next}; a warning before an action; the result of one. When the API offers a fix, put it in next as a Button.",
  avoid: "A one-word state: use Badge. A question that must be answered before going on: use Dialog.",
  variants: {
    tone: {
      description: "What kind of message it is.",
      default: "info",
      values: {
        info: "Something worth knowing; nothing is wrong.",
        success: "What was asked for was done.",
        warning: "Nothing failed yet, but something needs attention.",
        danger: "Something failed or was refused.",
      },
    },
  },
  props: [
    { name: "title", type: "string", required: true, description: "What happened, in one short line (Deploy refused)." },
    { name: "children", type: "Snippet", required: true, description: "Why it happened: the API's message, as plain text in full sentences." },
    { name: "next", type: "Snippet", description: "What to do next: the API's next, usually as a Button that does it." },
    { name: "tone", type: "'info' | 'success' | 'warning' | 'danger'", description: "See the tone variants." },
    { name: "live", type: "boolean", description: "It appeared because of something the person just did, so a screen reader announces it: as an alert for warning and danger, politely for info and success. Leave it off for a notice that is part of the page from the start." },
  ],
  a11y: "Static by default: its text is read in its place. With live, a warning or danger notice has role=alert and is announced at once; an info or success notice has role=status and is announced when the reader is idle. Its title says what happened in words, so colour is never the only sign. Any control in next is reached by Tab in reading order.",
  example: '<Notice tone="danger" title="Deploy refused" live>Port 80 needs administrator rights.{#snippet next()}<Button onclick={usePort8080}>Use port 8080</Button>{/snippet}</Notice>',
} as const satisfies ComponentMeta;

export type NoticeTone = keyof typeof meta.variants.tone.values;
