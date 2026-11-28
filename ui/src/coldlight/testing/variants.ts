// Every value of every variant axis a component declares has its own `[data-<axis>="<value>"]`
// rule in the component's CSS, and the CSS selects no value the data does not declare. The data
// and the look cannot drift apart.

import { expect } from "vitest";
import type { ComponentMeta } from "../catalog/types.ts";

export function expectVariantsStyled(meta: ComponentMeta, source: string): void {
  for (const [axis, { values }] of Object.entries(meta.variants)) {
    for (const value of Object.keys(values)) {
      expect(source, `${meta.name}: no CSS rule selects [data-${axis}="${value}"]`).toContain(
        `[data-${axis}="${value}"]`,
      );
    }
    for (const [, used] of source.matchAll(new RegExp(`\\[data-${axis}="([^"]+)"\\]`, "g"))) {
      expect(Object.keys(values), `${meta.name}: the CSS selects ${axis}="${used}", which its variants do not declare`).toContain(used);
    }
  }
}
