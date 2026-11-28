<!-- ConflictResolver: keep yours, take the incoming change, or merge both, previewed in red and green (ui.md U10). -->
<script lang="ts">
  import Button from "../../primitives/Button/Button.svelte";
  import Dialog from "../../primitives/Dialog/Dialog.svelte";
  import Diff from "../../primitives/Diff/Diff.svelte";
  import { diffCounts, lineDiff } from "../../primitives/Diff/lineDiff.ts";
  import type { ConflictChoice, ConflictLabels } from "./ConflictResolver.variants.ts";

  interface Props {
    open?: boolean;
    title: string;
    description?: string;
    name: string;
    existing: string;
    incoming: string;
    merged?: string;
    choice?: ConflictChoice;
    labels: ConflictLabels;
    onresolve: (choice: ConflictChoice, text: string) => void;
    oncancel?: () => void;
  }

  let {
    open = $bindable(false),
    title,
    description,
    name,
    existing,
    incoming,
    merged,
    choice = $bindable("incoming"),
    labels,
    onresolve,
    oncancel,
  }: Props = $props();

  const id = $props.id();

  const options = $derived(
    [
      { value: "existing" as const, label: labels.existing, hint: labels.existingHint, text: existing },
      { value: "incoming" as const, label: labels.incoming, hint: labels.incomingHint, text: incoming },
      ...(merged === undefined
        ? []
        : [{ value: "merged" as const, label: labels.merged, hint: labels.mergedHint, text: merged }]),
    ].map((option) => ({ ...option, counts: diffCounts(lineDiff(existing, option.text)) })),
  );

  // A choice the engine did not offer (merged, without a merge) falls back to the incoming change.
  const selected = $derived(options.find((option) => option.value === choice) ?? options[1]);

  function apply() {
    if (!selected) return;
    open = false;
    onresolve(selected.value, selected.text);
  }

  function cancel() {
    open = false;
    oncancel?.();
  }
</script>

<Dialog bind:open {title} {description} closeLabel={labels.close} onclose={() => oncancel?.()}>
  <fieldset class="choices">
    <legend class="legend">{labels.legend}</legend>
    {#each options as option (option.value)}
      <label class="choice">
        <input
          class="radio"
          type="radio"
          name="{id}-choice"
          value={option.value}
          checked={selected?.value === option.value}
          onchange={() => (choice = option.value)}
          aria-labelledby="{id}-{option.value}-label"
          aria-describedby="{id}-{option.value}-hint {id}-{option.value}-counts"
        />
        <span class="words">
          <span class="label" id="{id}-{option.value}-label">{option.label}</span>
          <span class="hint" id="{id}-{option.value}-hint">{option.hint}</span>
          <span class="counts" id="{id}-{option.value}-counts">
            {labels.counts(option.counts.added, option.counts.removed)}
          </span>
        </span>
      </label>
    {/each}
  </fieldset>

  {#if selected}
    <section class="preview" aria-labelledby="{id}-preview">
      <h3 class="preview-title" id="{id}-preview">{labels.preview}</h3>
      <Diff
        label={name}
        before={existing}
        after={selected.text}
        addedLabel={labels.added}
        removedLabel={labels.removed}
      >
        {#snippet unchanged()}{labels.noChange}{/snippet}
      </Diff>
    </section>
  {/if}

  {#snippet actions()}
    <Button onclick={cancel}>{labels.cancel}</Button>
    <Button tone="accent" emphasis="solid" onclick={apply}>{labels.apply}</Button>
  {/snippet}
</Dialog>

<style>
  .choices {
    display: grid;
    gap: var(--cl-space-2xs);
    margin: 0;
    padding: 0;
    border: 0;
    container-type: inline-size;
  }

  .legend {
    padding: 0;
    margin-block-end: var(--cl-space-2xs);
    font: var(--cl-type-label);
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: var(--cl-space-xs);
    min-block-size: var(--cl-size-target);
    padding: var(--cl-space-xs);
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-radius: var(--cl-radius-m);
    background: var(--cl-color-surface);
    cursor: pointer;
  }

  .choice:has(.radio:checked) {
    border-color: var(--cl-color-accent);
    background: var(--cl-color-accent-subtle);
  }

  .radio {
    margin: 0;
    margin-block-start: var(--cl-space-3xs);
    accent-color: var(--cl-color-accent);
  }

  .words {
    display: grid;
    gap: var(--cl-space-3xs);
  }

  .label {
    font: var(--cl-type-label);
  }

  .hint,
  .counts {
    font: var(--cl-type-small);
    color: var(--cl-color-text-muted);
  }

  .preview {
    display: grid;
    gap: var(--cl-space-2xs);
    margin-block-start: var(--cl-space-s);
  }

  .preview-title {
    margin: 0;
    font: var(--cl-type-label);
  }
</style>
