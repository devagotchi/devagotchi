<!-- Tabs: one of several panels of the same thing at a time. It has no variants; see Tabs.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import type { Tab } from "./Tabs.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
    label: string;
    tabs: Tab[];
    selected?: string;
    panel: Snippet<[string]>;
  }

  let { label, tabs, selected = $bindable(), panel, ...rest }: Props = $props();

  const id = $props.id();
  const tabId = (tab: string) => `${id}-tab-${tab}`;
  const panelId = `${id}-panel`;

  const enabled = $derived(tabs.filter((tab) => !tab.disabled));
  // A missing or disabled choice falls back to the first tab that can be chosen.
  const current = $derived(enabled.some((tab) => tab.id === selected) ? selected : enabled[0]?.id);

  const buttons: Record<string, HTMLButtonElement> = {};

  function choose(tab: string) {
    selected = tab;
    buttons[tab]?.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    const at = enabled.findIndex((tab) => tab.id === current);
    const last = enabled.length - 1;
    const next = {
      ArrowRight: at >= last ? 0 : at + 1,
      ArrowLeft: at <= 0 ? last : at - 1,
      Home: 0,
      End: last,
    }[event.key];
    const target = next === undefined ? undefined : enabled[next];
    if (!target) return;
    event.preventDefault();
    choose(target.id);
  }
</script>

<div {...rest} class="tabs">
  <div class="list" role="tablist" aria-label={label}>
    {#each tabs as tab (tab.id)}
      {@const chosen = tab.id === current}
      <button
        bind:this={buttons[tab.id]}
        type="button"
        class="tab"
        role="tab"
        id={tabId(tab.id)}
        aria-selected={chosen}
        aria-controls={chosen ? panelId : undefined}
        tabindex={chosen ? 0 : -1}
        disabled={tab.disabled}
        onclick={() => choose(tab.id)}
        {onkeydown}
      >
        {tab.label}
      </button>
    {/each}
  </div>
  {#if current !== undefined}
    <!-- Each tab gets a fresh panel, so one tab's state never shows under another's name. -->
    {#key current}
      <div class="panel" role="tabpanel" id={panelId} aria-labelledby={tabId(current)} tabindex="0">
        {@render panel(current)}
      </div>
    {/key}
  {/if}
</div>

<style>
  .tabs {
    display: grid;
    gap: var(--cl-space-s);
    min-inline-size: 0;
  }

  /* The row scrolls inside itself instead of widening the page; the padding keeps the focus ring visible. */
  .list {
    display: flex;
    overflow-x: auto;
    padding-block-start: calc(var(--cl-focus-width) + var(--cl-focus-offset));
    padding-inline: calc(var(--cl-focus-width) + var(--cl-focus-offset));
    border-block-end: var(--cl-line-thin) solid var(--cl-color-border);
    scrollbar-width: thin;
  }

  .tab {
    flex: none;
    min-block-size: var(--cl-size-target);
    min-inline-size: var(--cl-size-target);
    padding-inline: var(--cl-space-s);
    border: 0;
    border-block-end: var(--cl-line-thick) solid transparent;
    background: transparent;
    color: var(--cl-color-text-muted);
    font: var(--cl-type-label);
    white-space: nowrap;
    cursor: pointer;
    transition: color var(--cl-duration-fast) var(--cl-easing-standard);
  }

  .tab:disabled {
    cursor: not-allowed;
    opacity: var(--cl-opacity-disabled);
  }

  .tab:hover:enabled {
    color: var(--cl-color-text);
  }

  .tab[aria-selected="true"] {
    border-block-end-color: var(--cl-color-accent);
    color: var(--cl-color-text);
  }

  .panel {
    min-inline-size: 0;
  }
</style>
