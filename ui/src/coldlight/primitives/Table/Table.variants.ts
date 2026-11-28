import type { ComponentMeta } from "../../catalog/types.ts";

export const meta = {
  name: "Table",
  level: "primitive",
  summary: "A semantic data table: rows of the same kind of record, compared column by column.",
  use: "Records a person scans and compares: machines, tokens, grants, audit lines. On a narrow screen it scrolls sideways inside itself.",
  avoid: "A handful of records each read on its own: use List or Card. Laying out a page: use grid, never a table.",
  variants: {
    density: {
      description: "How much room each row has.",
      default: "comfortable",
      values: {
        comfortable: "Room to read: the default for most tables.",
        compact: "More rows on the screen: long logs and audit trails that are scanned rather than read.",
      },
    },
  },
  props: [
    { name: "caption", type: "string", required: true, description: "What the table holds. It names the table and its scrolling region." },
    { name: "captionHidden", type: "boolean", description: "Hide the caption visually when a heading beside the table already says the same; screen readers still hear it." },
    { name: "columns", type: "Array<{ key: string; label: string; align?: 'start' | 'end' }>", required: true, description: "The columns, in order. Numbers align to the end." },
    { name: "rows", type: "T[]", required: true, description: "The records, one per row." },
    { name: "rowKey", type: "(row: T) => string", required: true, description: "A stable, unique key for a record, such as its id." },
    { name: "cell", type: "Snippet<[T, string]>", required: true, description: "Renders one cell, given the record and the column key." },
    { name: "empty", type: "Snippet", description: "The empty state, shown in one full-width row when there are no records: what is missing and what to do." },
    { name: "density", type: "'comfortable' | 'compact'", description: "See the density variants." },
  ],
  a11y: "A native <table> with a <caption> and column headers (th scope=col). It sits in a region named by the caption that can be focused (tabindex=0), so a keyboard can scroll a table wider than the screen.",
  example: '<Table caption={t.machines} {columns} rows={machines} rowKey={(m) => m.id}>{#snippet cell(m, key)}{m[key]}{/snippet}</Table>',
} as const satisfies ComponentMeta;

export type TableDensity = keyof typeof meta.variants.density.values;

export interface Column {
  key: string;
  label: string;
  align?: "start" | "end";
}
