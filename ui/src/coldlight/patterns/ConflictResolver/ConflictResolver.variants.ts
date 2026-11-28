import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "ConflictResolver",
  level: "pattern",
  summary: "The friendly popup for a change that meets a newer version: keep yours, take the incoming one, or merge both, with a red and green preview before anything is written.",
  use: "Whenever the engine refuses a write because the target changed since it was read (changed_since_read), an import collides, or a setting is stale (ui.md U10). It is the only conflict UI: use it everywhere.",
  avoid: "Asking someone to confirm an ordinary change with no newer version: use Dialog.",
  variants: {},
  props: [
    { name: "open", type: "boolean", description: "Whether it is showing. Bindable." },
    { name: "title", type: "string", required: true, description: "What happened, as a heading (This file changed while you were editing)." },
    { name: "description", type: "string", description: "One sentence on why it matters and what the choice does." },
    { name: "name", type: "string", required: true, description: "What is in conflict: the file path or setting, read out with the preview." },
    { name: "existing", type: "string", required: true, description: "The version in place now, as the engine reported it." },
    { name: "incoming", type: "string", required: true, description: "The version this change would write." },
    { name: "merged", type: "string", description: "Both merged, as the engine produced it. Without it, merging is not offered: the UI never merges by itself (U2)." },
    { name: "choice", type: "'incoming' | 'existing' | 'merged'", description: "The selected choice. Bindable; defaults to incoming." },
    { name: "labels", type: "ConflictLabels", required: true, description: "Every word it shows: the three choices and their hints, apply, cancel, close, the diff words, the no-change message and the counts." },
    { name: "onresolve", type: "(choice, text) => void", required: true, description: "Called with the choice and the text to write when the person applies it." },
    { name: "oncancel", type: "() => void", description: "Called when the person closes it without choosing." },
  ],
  a11y: "A modal Dialog: focus stays inside and Escape cancels. The choices are a radio group with a legend, each choice reads its line counts, and the preview is a labelled Diff whose added and removed lines are read with their words.",
  example:
    '<ConflictResolver bind:open title="config.toml changed while you were editing" name="config.toml" {existing} {incoming} {merged} {labels} onresolve={(choice, text) => api.files.write(path, text)} />',
} as const satisfies ComponentMeta;

export type ConflictChoice = "incoming" | "existing" | "merged";

export interface ConflictLabels {
  legend: string;
  existing: string;
  existingHint: string;
  incoming: string;
  incomingHint: string;
  merged: string;
  mergedHint: string;
  preview: string;
  apply: string;
  cancel: string;
  close: string;
  added: string;
  removed: string;
  noChange: string;
  counts: (added: number, removed: number) => string;
}
