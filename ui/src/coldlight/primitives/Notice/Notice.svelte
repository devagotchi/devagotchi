<!-- Notice: an inline message that says what happened, why, and what to do next. Its variants are in Notice.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type NoticeTone } from "./Notice.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "children" | "title" | "role"> {
    title: string;
    children: Snippet;
    next?: Snippet;
    tone?: NoticeTone;
    live?: boolean;
  }

  let { title, children, next, tone = meta.variants.tone.default, live = false, ...rest }: Props = $props();

  // Only a notice that just appeared is announced; one that is part of the page is read in its place.
  const role = $derived(live ? (tone === "danger" || tone === "warning" ? "alert" : "status") : undefined);
</script>

<div {...rest} class="notice" data-tone={tone} {role}>
  <div class="layout">
    <div class="body">
      <p class="title">{title}</p>
      <div class="message">{@render children()}</div>
    </div>
    {#if next}
      <div class="next">{@render next()}</div>
    {/if}
  </div>
</div>

<style>
  /* A tone names a quiet background and the edge that marks it. The next step sits under the
     message on a narrow notice and beside it on a wide one. */
  .notice {
    container-type: inline-size;
    padding: var(--cl-space-s);
    border-inline-start: var(--cl-line-thick) solid var(--edge);
    border-radius: var(--cl-radius-m);
    background: var(--fill);
    color: var(--cl-color-text);
    font: var(--cl-type-body);
  }

  .layout {
    display: grid;
    gap: var(--cl-space-xs);
  }

  .body {
    display: grid;
    gap: var(--cl-space-3xs);
    max-inline-size: var(--cl-size-measure);
  }

  .title {
    margin: 0;
    font: var(--cl-type-label);
  }

  .next {
    display: flex;
    flex-wrap: wrap;
    gap: var(--cl-space-xs);
  }

  @container (min-inline-size: 48ch) {
    .layout {
      grid-template-columns: 1fr auto;
      align-items: center;
    }
  }

  .notice[data-tone="info"] {
    --fill: var(--cl-color-accent-subtle);
    --edge: var(--cl-color-accent);
  }

  .notice[data-tone="success"] {
    --fill: var(--cl-color-success-subtle);
    --edge: var(--cl-color-success);
  }

  .notice[data-tone="warning"] {
    --fill: var(--cl-color-warning-subtle);
    --edge: var(--cl-color-warning);
  }

  .notice[data-tone="danger"] {
    --fill: var(--cl-color-danger-subtle);
    --edge: var(--cl-color-danger);
  }
</style>
