<!-- Diff: two versions of a text, line by line, in red and green. Its variants are in Diff.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type DiffWrap } from "./Diff.variants.ts";
  import { lineDiff } from "./lineDiff.ts";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
    before: string;
    after: string;
    label: string;
    addedLabel: string;
    removedLabel: string;
    unchanged?: Snippet;
    wrap?: DiffWrap;
  }

  let { before, after, label, addedLabel, removedLabel, unchanged, wrap = meta.variants.wrap.default, ...rest }: Props =
    $props();

  const lines = $derived(lineDiff(before, after));
  const changed = $derived(lines.some((line) => line.kind !== "same"));
  const marks = { same: " ", added: "+", removed: "−" } as const;
</script>

<!-- A region that scrolls sideways must be reachable by keyboard (axe: scrollable-region-focusable). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div {...rest} class="diff" data-wrap={wrap} role="region" aria-label={label} tabindex="0">
  {#if !changed && unchanged}
    <div class="unchanged">{@render unchanged()}</div>
  {:else}
    <ol class="lines">
      {#each lines as line, index (index)}
        <li class="line" data-kind={line.kind}>
          <span class="number" aria-hidden="true">{line.before ?? ""}</span>
          <span class="number" aria-hidden="true">{line.after ?? ""}</span>
          <span class="mark" aria-hidden="true">{marks[line.kind]}</span>
          <span class="text">
            {#if line.kind === "added"}<span class="spoken">{addedLabel}: </span>{/if}
            {#if line.kind === "removed"}<span class="spoken">{removedLabel}: </span>{/if}
            {line.text}
          </span>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .diff {
    overflow-x: auto;
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-m);
    background: var(--cl-color-surface);
    font: var(--cl-type-code);
  }

  .text {
    padding-inline-end: var(--cl-space-xs);
  }

  .diff[data-wrap="scroll"] .text {
    white-space: pre;
  }

  .diff[data-wrap="wrap"] .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .lines {
    margin: 0;
    padding: 0;
    list-style: none;
    min-inline-size: max-content;
  }

  .diff[data-wrap="wrap"] .lines {
    min-inline-size: 0;
  }

  .line {
    position: relative;
    display: grid;
    grid-template-columns: 5ch 5ch 2ch 1fr;
    color: var(--cl-color-text);
  }

  .line[data-kind="added"] {
    background: var(--cl-color-diff-added);
    color: var(--cl-color-diff-added-text);
  }

  .line[data-kind="removed"] {
    background: var(--cl-color-diff-removed);
    color: var(--cl-color-diff-removed-text);
  }

  .number {
    padding-inline: var(--cl-space-3xs);
    text-align: end;
    color: var(--cl-color-text-muted);
    user-select: none;
  }

  .line:is([data-kind="added"], [data-kind="removed"]) .number {
    color: inherit;
  }

  .mark {
    text-align: center;
    user-select: none;
  }

  .unchanged {
    padding: var(--cl-space-s);
    font: var(--cl-type-body);
    color: var(--cl-color-text-muted);
  }

  /* Read by screen readers, hidden on screen: the +/− marks and colours say it for the eye. */
  .spoken {
    position: absolute;
    inline-size: var(--cl-line-thin);
    block-size: var(--cl-line-thin);
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
