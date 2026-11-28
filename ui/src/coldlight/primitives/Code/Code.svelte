<!-- Code: code, paths, commands and IDs, exactly as written. Its variants are in Code.variants.ts. -->
<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";
  import Button from "../Button/Button.svelte";
  import { meta, type CodeDisplay } from "./Code.variants.ts";

  interface Props extends Omit<HTMLAttributes<HTMLElement>, "children"> {
    code: string;
    language?: string;
    copyLabel?: string;
    copiedLabel?: string;
    copyFailedLabel?: string;
    display?: CodeDisplay;
  }

  let { code, language, copyLabel, copiedLabel, copyFailedLabel, display = meta.variants.display.default, ...rest }: Props = $props();

  let text: HTMLElement | undefined = $state();
  // How the last copy went; a new `code` has not been copied yet.
  let outcome: { code: string; copied: boolean } | undefined = $state();
  const status = $derived(outcome?.code === code ? (outcome.copied ? copiedLabel : copyFailedLabel) : undefined);

  async function copy() {
    outcome = undefined;
    try {
      await navigator.clipboard.writeText(code);
      outcome = { code, copied: true };
    } catch {
      // The clipboard refused (no permission, or not a secure page): select the text to copy by
      // hand, and say so (U9).
      if (text) getSelection()?.selectAllChildren(text);
      outcome = { code, copied: false };
    }
  }
</script>

{#snippet controls()}
  {#if copyLabel}
    <Button emphasis="quiet" onclick={copy}>{copyLabel}</Button>
  {/if}
  {#if copiedLabel || copyFailedLabel}
    <span class="status" role="status" aria-live="polite">{status ?? ""}</span>
  {/if}
{/snippet}

{#if display === "inline"}
  <span {...rest} class="code" data-display="inline" data-language={language}>
    <code bind:this={text}>{code}</code>
    {@render controls()}
  </span>
{:else}
  <div {...rest} class="code" data-display="block" data-language={language}>
    {#if language || copyLabel || copiedLabel || copyFailedLabel}
      <div class="bar">
        {#if language}
          <span class="language">{language}</span>
        {/if}
        <span class="controls">{@render controls()}</span>
      </div>
    {/if}
    <!-- A long line scrolls inside the block; tabindex lets a keyboard scroll it
         (the WAI pattern for a scrollable region, which Svelte's heuristic cannot tell from a static one). -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <pre tabindex="0"><code bind:this={text}>{code}</code></pre>
  </div>
{/if}

<style>
  code {
    font: var(--cl-type-code);
  }

  .code[data-display="inline"] {
    display: inline;
  }

  .code[data-display="inline"] code {
    padding-inline: var(--cl-space-3xs);
    border-radius: var(--cl-radius-s);
    background: var(--cl-color-surface-sunk);
    overflow-wrap: anywhere;
  }

  .code[data-display="block"] {
    display: grid;
    min-inline-size: 0;
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-s);
    background: var(--cl-color-surface-sunk);
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--cl-space-xs);
    padding-inline: var(--cl-space-s) 0;
    border-block-end: var(--cl-line-thin) solid var(--cl-color-border);
  }

  .language {
    color: var(--cl-color-text-muted);
    font: var(--cl-type-small);
  }

  .controls {
    display: inline-flex;
    flex-direction: row-reverse;
    align-items: center;
    gap: var(--cl-space-2xs);
    margin-inline-start: auto;
  }

  .status {
    color: var(--cl-color-text-muted);
    font: var(--cl-type-small);
  }

  pre {
    margin: 0;
    padding: var(--cl-space-s);
    overflow-x: auto;
    font: var(--cl-type-code);
  }
</style>
