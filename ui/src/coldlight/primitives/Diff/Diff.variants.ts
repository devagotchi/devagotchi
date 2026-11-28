import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Diff",
  level: "primitive",
  summary: "Two versions of a text, line by line: what a change keeps, removes (red) and adds (green).",
  use: "Previewing a change before anything is written: a file edit, a setting, an import. The conflict pattern shows one for each choice.",
  avoid: "Showing one version: use Code.",
  variants: {
    wrap: {
      description: "What a line longer than the box does.",
      default: "scroll",
      values: {
        scroll: "The diff scrolls sideways inside itself; lines keep their shape. For code.",
        wrap: "Long lines wrap. For prose and settings.",
      },
    },
  },
  props: [
    { name: "before", type: "string", required: true, description: "The version now in place." },
    { name: "after", type: "string", required: true, description: "The version the change would write." },
    { name: "label", type: "string", required: true, description: "What is being compared, for screen readers (the file or setting name)." },
    { name: "addedLabel", type: "string", required: true, description: "The word read before an added line (Added)." },
    { name: "removedLabel", type: "string", required: true, description: "The word read before a removed line (Removed)." },
    { name: "unchanged", type: "Snippet", description: "Shown when the two versions are the same." },
    { name: "wrap", type: "'scroll' | 'wrap'", description: "See the wrap variants." },
  ],
  a11y: "A list of lines labelled by `label`. Each added or removed line starts with its word, read aloud and hidden on screen; the + and − markers and the colours repeat it for sighted readers, so colour is never the only signal. A scrolling diff can be reached and scrolled with the keyboard.",
  example: '<Diff label="config.toml" before={current} after={incoming} addedLabel="Added" removedLabel="Removed" />',
} as const satisfies ComponentMeta;

export type DiffWrap = keyof typeof meta.variants.wrap.values;
