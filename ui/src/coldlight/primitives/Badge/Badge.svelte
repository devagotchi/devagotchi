<!-- Badge: a short label for the state of something. Its variants are in Badge.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type BadgeTone } from "./Badge.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
    children: Snippet;
    tone?: BadgeTone;
  }

  let { children, tone = meta.variants.tone.default, ...rest }: Props = $props();
</script>

<span {...rest} class="badge" data-tone={tone}>
  {@render children()}
</span>

<style>
  /* A tone names a quiet background and the ink that reads on it. */
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--cl-space-3xs);
    padding-inline: var(--cl-space-2xs);
    border: var(--cl-line-thin) solid var(--edge);
    border-radius: var(--cl-radius-s);
    background: var(--fill);
    color: var(--ink);
    font: var(--cl-type-small);
    white-space: nowrap;
  }

  .badge[data-tone="neutral"] {
    --fill: var(--cl-color-surface-sunk);
    --ink: var(--cl-color-text);
    --edge: var(--cl-color-border);
  }

  .badge[data-tone="accent"] {
    --fill: var(--cl-color-accent-subtle);
    --ink: var(--cl-color-accent);
    --edge: var(--cl-color-accent-subtle);
  }

  .badge[data-tone="success"] {
    --fill: var(--cl-color-success-subtle);
    --ink: var(--cl-color-success);
    --edge: var(--cl-color-success-subtle);
  }

  .badge[data-tone="warning"] {
    --fill: var(--cl-color-warning-subtle);
    --ink: var(--cl-color-warning);
    --edge: var(--cl-color-warning-subtle);
  }

  .badge[data-tone="pending"] {
    --fill: var(--cl-color-pending-subtle);
    --ink: var(--cl-color-pending);
    --edge: var(--cl-color-pending-subtle);
  }

  .badge[data-tone="danger"] {
    --fill: var(--cl-color-danger-subtle);
    --ink: var(--cl-color-danger);
    --edge: var(--cl-color-danger-subtle);
  }
</style>
