<!-- List: a vertical list of items drawn from data. Its variants are in List.variants.ts. -->
<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { meta, type ListDensity } from "./List.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLUListElement>, "children" | "aria-label" | "role"> {
    items: T[];
    item: Snippet<[T]>;
    key: (item: T) => string;
    label: string;
    empty?: Snippet;
    density?: ListDensity;
  }

  let { items, item, key, label, empty, density = meta.variants.density.default, ...rest }: Props = $props();
</script>

{#if items.length === 0 && empty}
  {@render empty()}
{:else}
  <!-- role="list" brings back the semantics that list-style: none removes in Safari. -->
  <ul {...rest} class="list" data-density={density} role="list" aria-label={label}>
    {#each items as entry (key(entry))}
      <li class="item">{@render item(entry)}</li>
    {/each}
  </ul>
{/if}

<style>
  /* A density names how much room a row has; a thin line divides one row from the next. */
  .list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    padding-block: var(--pad);
  }

  .item + .item {
    border-block-start: var(--cl-line-thin) solid var(--cl-color-border);
  }

  .list[data-density="comfortable"] {
    --pad: var(--cl-space-xs);
  }

  .list[data-density="compact"] {
    --pad: var(--cl-space-3xs);
  }
</style>
