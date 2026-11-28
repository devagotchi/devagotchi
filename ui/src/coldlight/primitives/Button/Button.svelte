<!-- Button: a control that does one thing when pressed. Its variants are in Button.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { meta, type ButtonEmphasis, type ButtonTone } from "./Button.variants.ts";

  interface Props extends Omit<HTMLButtonAttributes, "children"> {
    children: Snippet;
    tone?: ButtonTone;
    emphasis?: ButtonEmphasis;
    busy?: boolean;
  }

  let {
    children,
    tone = meta.variants.tone.default,
    emphasis = meta.variants.emphasis.default,
    busy = false,
    disabled = false,
    type = "button",
    ...rest
  }: Props = $props();
</script>

<button
  {...rest}
  {type}
  class="button"
  data-tone={tone}
  data-emphasis={emphasis}
  disabled={disabled || busy}
  aria-busy={busy || undefined}
>
  {@render children()}
</button>

<style>
  /* A tone names two colours; an emphasis decides where they go. */
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--cl-space-3xs);
    min-block-size: var(--cl-size-target);
    min-inline-size: var(--cl-size-target);
    padding-inline: var(--cl-space-s);
    border: var(--cl-line-thin) solid var(--edge);
    border-radius: var(--cl-radius-m);
    background: var(--fill);
    color: var(--ink);
    font: var(--cl-type-label);
    cursor: pointer;
    transition: background-color var(--cl-duration-fast) var(--cl-easing-standard);
  }

  .button:disabled {
    cursor: not-allowed;
    opacity: var(--cl-opacity-disabled);
  }

  .button[aria-busy="true"] {
    cursor: progress;
  }

  .button[data-tone="neutral"] {
    --tone: var(--cl-color-text);
    --tone-hover: var(--cl-color-text-muted);
    --on-tone: var(--cl-color-surface);
    --tone-edge: var(--cl-color-border);
  }

  .button[data-tone="accent"] {
    --tone: var(--cl-color-accent);
    --tone-hover: var(--cl-color-accent-hover);
    --on-tone: var(--cl-color-on-accent);
    --tone-edge: var(--cl-color-accent);
  }

  .button[data-tone="danger"] {
    --tone: var(--cl-color-danger);
    --tone-hover: var(--cl-color-danger-hover);
    --on-tone: var(--cl-color-on-danger);
    --tone-edge: var(--cl-color-danger);
  }

  .button[data-emphasis="solid"] {
    --fill: var(--tone);
    --ink: var(--on-tone);
    --edge: var(--tone);
  }

  .button[data-emphasis="solid"]:hover:enabled {
    --fill: var(--tone-hover);
    --edge: var(--tone-hover);
  }

  .button[data-emphasis="outline"] {
    --fill: var(--cl-color-surface);
    --ink: var(--tone);
    --edge: var(--tone-edge);
  }

  .button[data-emphasis="quiet"] {
    --fill: transparent;
    --ink: var(--tone);
    --edge: transparent;
  }

  .button:is([data-emphasis="outline"], [data-emphasis="quiet"]):hover:enabled {
    --fill: var(--cl-color-surface-sunk);
  }
</style>
