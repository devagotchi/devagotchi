<!-- Field: a labelled text input, or a text area when multiline. Its variants are in Field.variants.ts. -->
<script lang="ts">
  import type { HTMLInputAttributes, HTMLTextareaAttributes } from "svelte/elements";
  import { meta, type FieldType, type FieldWidth } from "./Field.variants.ts";

  interface Props extends Omit<HTMLInputAttributes, "children" | "type" | "value" | "id"> {
    label: string;
    value?: string | number | null;
    hint?: string;
    error?: string;
    multiline?: boolean;
    type?: FieldType;
    width?: FieldWidth;
  }

  let {
    label,
    value = $bindable(""),
    hint,
    error,
    multiline = false,
    type = "text",
    width = meta.variants.width.default,
    required = false,
    disabled = false,
    "aria-describedby": describedBy,
    ...rest
  }: Props = $props();

  // One id per field, so the label, hint and error find their control however many fields a screen has.
  const id = $props.id();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedby = $derived(
    [describedBy, hint ? hintId : undefined, error ? errorId : undefined].filter(Boolean).join(" ") || undefined,
  );
</script>

<div class="field" data-width={width}>
  <label class="label" for={id}>{label}</label>
  {#if hint}
    <p class="hint" id={hintId}>{hint}</p>
  {/if}
  {#if multiline}
    <textarea
      {...rest as HTMLTextareaAttributes}
      {id}
      class="control"
      bind:value
      {required}
      {disabled}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedby}
    ></textarea>
  {:else}
    <input
      {...rest}
      {id}
      {type}
      class="control"
      bind:value
      {required}
      {disabled}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedby}
    />
  {/if}
  {#if error}
    <p class="error" id={errorId}>{error}</p>
  {/if}
</div>

<style>
  /* The label, then how to fill it in, then the control, then what is wrong with it. */
  .field {
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
    padding-block: var(--cl-space-3xs);
    padding-inline: var(--cl-space-xs);
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-m);
    background: var(--cl-color-surface);
    color: var(--cl-color-text);
    font: var(--cl-type-body);
  }

  textarea.control {
    resize: vertical;
  }

  .control[aria-invalid="true"] {
    border-color: var(--cl-color-danger);
  }

  .control:disabled {
    background: var(--cl-color-surface-sunk);
    cursor: not-allowed;
    opacity: var(--cl-opacity-disabled);
  }

  .field[data-width="fill"] .control {
    inline-size: 100%;
  }

  .field[data-width="fit"] {
    display: inline-flex;
  }

  .field[data-width="fit"] .control {
    inline-size: auto;
    max-inline-size: 100%;
  }
</style>
