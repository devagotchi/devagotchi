<!-- Card: a surface that groups one thing. Its variants are in Card.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type CardEmphasis, type CardHeadingLevel } from "./Card.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "children" | "title"> {
    title?: string;
    headingLevel?: CardHeadingLevel;
    description?: string;
    actions?: Snippet;
    children?: Snippet;
    emphasis?: CardEmphasis;
  }

  let {
    title,
    headingLevel = 2,
    description,
    actions,
    children,
    emphasis = meta.variants.emphasis.default,
    ...rest
  }: Props = $props();

  const heading = $derived(`h${headingLevel}`);
</script>

<div {...rest} class="card" data-emphasis={emphasis}>
  {#if title || description || actions}
    <div class="header">
      {#if title || description}
        <div class="heading">
          {#if title}
            <svelte:element this={heading} class="title">{title}</svelte:element>
          {/if}
          {#if description}
            <p class="description">{description}</p>
          {/if}
        </div>
      {/if}
      {#if actions}
        <div class="actions">{@render actions()}</div>
      {/if}
    </div>
  {/if}
  {#if children}
    <div class="content">{@render children()}</div>
  {/if}
</div>

<style>
  /* The actions sit under the title on a narrow card and beside it on a wide one. */
  .card {
    container-type: inline-size;
    display: grid;
    gap: var(--cl-space-s);
    padding: var(--cl-space-s);
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-l);
    background: var(--cl-color-surface);
    color: var(--cl-color-text);
  }

  .header {
    display: grid;
    gap: var(--cl-space-xs);
  }

  .heading {
    display: grid;
    gap: var(--cl-space-3xs);
  }

  .title {
    margin: 0;
    font: var(--cl-type-title);
  }

  .description {
    margin: 0;
    color: var(--cl-color-text-muted);
    font: var(--cl-type-small);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--cl-space-xs);
  }

  @container (min-inline-size: 40ch) {
    .header {
      grid-template-columns: 1fr auto;
      align-items: start;
    }

    .actions {
      grid-column: 2;
    }
  }

  .card[data-emphasis="flat"] {
    box-shadow: none;
  }

  .card[data-emphasis="raised"] {
    box-shadow: var(--cl-elevation-raised);
  }
</style>
