// What every ColdLight component says about itself, in its `<Name>.variants.ts`. The catalog
// (ui/catalog.json, ui/CATALOG.md) is built from these, so a person or an AI can choose and use a
// component without opening its source. Keep each file free of runtime imports: the catalog
// build loads it with Node.

export type Level = "primitive" | "block" | "template" | "pattern";

/** One axis of variants: its values are data the component's CSS selects with `data-<axis>`. */
export interface VariantAxis {
  description: string;
  default: string;
  values: Record<string, string>;
}

export interface PropDoc {
  name: string;
  type: string;
  required?: boolean;
  description: string;
}

export interface ComponentMeta {
  name: string;
  level: Level;
  summary: string;
  /** When to reach for it. */
  use: string;
  /** When to reach for something else instead, and what. */
  avoid?: string;
  variants: Record<string, VariantAxis>;
  props: PropDoc[];
  /** How it works with a keyboard and a screen reader. */
  a11y: string;
  /** The smallest real use, as Svelte markup. */
  example: string;
}
