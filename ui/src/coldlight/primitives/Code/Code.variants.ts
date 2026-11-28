import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Code",
  level: "primitive",
  summary: "Code, a path, a command or an ID, set in the code face and kept exactly as written.",
  use: "Anything a person types, pastes or reads character by character: a command to run, a config line, a file path, a token's prefix, a machine ID.",
  avoid: "Changes between two versions: use Diff. Ordinary emphasis in a sentence: use the text's own weight.",
  variants: {
    display: {
      description: "Where it sits.",
      default: "block",
      values: {
        block: "Its own block: a command or a snippet of several lines. It scrolls sideways inside itself rather than wrapping.",
        inline: "Inside a sentence: a path, a flag or an ID. It wraps anywhere rather than widening the page.",
      },
    },
  },
  props: [
    { name: "code", type: "string", required: true, description: "The exact text." },
    { name: "language", type: "string", description: "What it is written in (bash, toml). A block shows it as a label; both carry it as data-language. There is no highlighting." },
    { name: "copyLabel", type: "string", description: "The label of a copy button (Copy). Without it there is no button." },
    { name: "copiedLabel", type: "string", description: "What is announced, and shown, once the text is on the clipboard (Copied)." },
    { name: "copyFailedLabel", type: "string", description: "What is announced, and shown, when the clipboard refuses and the text is selected to copy by hand (Copy it by hand; the text is selected)." },
    { name: "display", type: "'block' | 'inline'", description: "See the display variants." },
  ],
  a11y: "Inline it is a <code>; as a block it is <pre><code>, focusable so a keyboard can scroll a long line. The copy button is a Button, at least 48 dp. A copy is announced through a polite live region (role=status). If the clipboard refuses, the text is selected instead, ready to copy by hand.",
  example: '<Code code="devagotchi hub serve" language="bash" copyLabel={t.copy} copiedLabel={t.copied} />',
} as const satisfies ComponentMeta;

export type CodeDisplay = keyof typeof meta.variants.display.values;
