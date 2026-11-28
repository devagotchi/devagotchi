<!-- Table: a semantic data table built from records. Its variants are in Table.variants.ts. -->
<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type Column, type TableDensity } from "./Table.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
    caption: string;
    captionHidden?: boolean;
    columns: Column[];
    rows: T[];
    rowKey: (row: T) => string;
    cell: Snippet<[T, string]>;
    empty?: Snippet;
    density?: TableDensity;
  }

  let {
    caption,
    captionHidden = false,
    columns,
    rows,
    rowKey,
    cell,
    empty,
    density = meta.variants.density.default,
    ...rest
  }: Props = $props();
</script>

<!-- A table wider than the screen scrolls inside this region; tabindex lets a keyboard scroll it
     (the WAI pattern for a scrollable region, which Svelte's heuristic cannot tell from a static one). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div {...rest} class="table" role="region" aria-label={caption} tabindex="0" data-density={density}>
  <table>
    <caption class:hidden={captionHidden}>{caption}</caption>
    <thead>
      <tr>
        {#each columns as column (column.key)}
          <th scope="col" data-align={column.align ?? "start"}>{column.label}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (rowKey(row))}
        <tr>
          {#each columns as column (column.key)}
            <td data-align={column.align ?? "start"}>{@render cell(row, column.key)}</td>
          {/each}
        </tr>
      {:else}
        {#if empty}
          <tr>
            <td class="empty" colspan={columns.length}>{@render empty()}</td>
          </tr>
        {/if}
      {/each}
    </tbody>
  </table>
</div>

<style>
  .table {
    max-inline-size: 100%;
    overflow-x: auto;
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-m);
    background: var(--cl-color-surface);
  }

  .table[data-density="comfortable"] {
    --cell-block: var(--cl-space-xs);
    --cell-inline: var(--cl-space-s);
  }

  .table[data-density="compact"] {
    --cell-block: var(--cl-space-3xs);
    --cell-inline: var(--cl-space-xs);
  }

  table {
    inline-size: 100%;
    border-collapse: collapse;
    font: var(--cl-type-body);
  }

  caption {
    padding: var(--cl-space-xs) var(--cl-space-s);
    color: var(--cl-color-text-muted);
    font: var(--cl-type-small);
    text-align: start;
  }

  /* Hidden from sight, not from screen readers. */
  caption.hidden {
    position: absolute;
    inline-size: 1ch;
    block-size: 1ch;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  th,
  td {
    padding: var(--cell-block) var(--cell-inline);
    border-block-end: var(--cl-line-thin) solid var(--cl-color-border);
    text-align: start;
    vertical-align: top;
  }

  th {
    background: var(--cl-color-surface-sunk);
    color: var(--cl-color-text-muted);
    font: var(--cl-type-label);
    white-space: nowrap;
  }

  [data-align="end"] {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }

  tbody tr:last-child > td {
    border-block-end: 0;
  }

  .empty {
    padding: var(--cl-space-l) var(--cl-space-s);
    color: var(--cl-color-text-muted);
    text-align: center;
  }
</style>
