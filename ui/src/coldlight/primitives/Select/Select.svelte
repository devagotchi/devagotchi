<!-- Select: a labelled native choice of one value. Its variants are in Select.variants.ts. -->
<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";
  import { meta, type SelectOption, type SelectWidth } from "./Select.variants.ts";

  interface Props extends Omit<HTMLSelectAttributes, "children" | "value" | "id" | "multiple"> {
    label: string;
    options: SelectOption[];
    value?: string;
    hint?: string;
    error?: string;
    width?: SelectWidth;
  }

  let {
    label,
    options,
    value = $bindable(),
    hint,
    error,
    width = meta.variants.width.default,
    required = false,
    disabled = false,
    "aria-describedby": describedBy,
    ...rest
  }: Props = $props();

  // One id per select, so the label, hint and error find their control however many a screen has.
  const id = $props.id();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedby = $derived(
    [describedBy, hint ? hintId : undefined, error ? errorId : undefined].filter(Boolean).join(" ") || undefined,
  );
</script>

<div class="select" data-width={width}>
  <label class="label" for={id}>{label}</label>
  {#if hint}
    <p class="hint" id={hintId}>{hint}</p>
  {/if}
  <select
    {...rest}
    {id}
    class="control"
    bind:value
    {required}
    {disabled}
    aria-invalid={error ? true : undefined}
    aria-describedby={describedby}
  >
    {#each options as option (option.value)}
      <option value={option.value} disabled={option.disabled}>{option.label}</option>
    {/each}
  </select>
  {#if error}
    <p class="error" id={errorId}>{error}</p>
  {/if}
</div>

<style>
  /* The same order as a Field: the label, how to choose, the control, what is wrong with it. */
  .select {
    display: flex;
    flex-direction: column;
    gap: var(--cl-space-2xs);
  }

  .label {
    color: var(--cl-color-text);
    font: var(--cl-type-label);
  }

  .hint,
  .error {
    margin: 0;
    font: var(--cl-type-small);
  }

  .hint {
    color: var(--cl-color-text-muted);
  }

  .error {
    color: var(--cl-color-danger);
  }

  .control {
    min-block-size: var(--cl-size-target);
    padding-inline: var(--cl-space-xs);
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-m);
    background: var(--cl-color-surface);
    color: var(--cl-color-text);
    font: var(--cl-type-body);
    cursor: pointer;
  }

  .control[aria-invalid="true"] {
    border-color: var(--cl-color-danger);
  }

  .control:disabled {
    background: var(--cl-color-surface-sunk);
    cursor: not-allowed;
    opacity: var(--cl-opacity-disabled);
  }

  .select[data-width="fill"] .control {
    inline-size: 100%;
  }

  .select[data-width="fit"] {
    display: inline-flex;
  }

  .select[data-width="fit"] .control {
    inline-size: auto;
    max-inline-size: 100%;
  }
</style>
