<!-- Dialog: a modal window over the page, the native <dialog>. Its variants are in Dialog.variants.ts. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLDialogAttributes } from "svelte/elements";
  import Button from "../Button/Button.svelte";
  import { meta, type DialogTone } from "./Dialog.variants.ts";

  interface Props extends Omit<HTMLDialogAttributes, "children" | "title" | "open" | "onclose"> {
    open?: boolean;
    title: string;
    description?: string;
    children: Snippet;
    actions?: Snippet;
    closeLabel: string;
    tone?: DialogTone;
    onclose?: () => void;
  }

  let {
    open = $bindable(false),
    title,
    description,
    children,
    actions,
    closeLabel,
    tone = meta.variants.tone.default,
    onclose,
    ...rest
  }: Props = $props();

  const id = $props.id();
  const titleId = `${id}-title`;
  const descriptionId = `${id}-description`;

  let dialog: HTMLDialogElement | undefined = $state();

  // `open` is the one source of truth; the element follows it.
  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });

  /** The person closed it: by the close button, by Escape, or by a form inside with method="dialog". */
  function dismiss() {
    if (!open) return;
    open = false;
    onclose?.();
  }

  // Escape fires `cancel`; the state closes the element, so the two cannot disagree.
  function oncancel(event: Event) {
    event.preventDefault();
    dismiss();
  }
</script>

<dialog
  {...rest}
  bind:this={dialog}
  class="dialog"
  data-tone={tone}
  aria-labelledby={titleId}
  aria-describedby={description ? descriptionId : undefined}
  {oncancel}
  onclose={dismiss}
>
  <header class="head">
    <h2 class="title" id={titleId}>{title}</h2>
    <Button emphasis="quiet" aria-label={closeLabel} onclick={dismiss}>
      <svg class="cross" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M6 6l12 12M18 6L6 18" />
      </svg>
    </Button>
  </header>
  <div class="body">
    {#if description}
      <p class="description" id={descriptionId}>{description}</p>
    {/if}
    {@render children()}
  </div>
  {#if actions}
    <footer class="actions">
      {@render actions()}
    </footer>
  {/if}
</dialog>

<style>
  /* The browser keeps its own gutter (max-inline-size) on a phone; this sets the width on a wide screen. */
  .dialog {
    container-type: inline-size;
    inline-size: min(100%, var(--cl-size-dialog));
    padding: 0;
    border: var(--cl-line-thin) solid var(--cl-color-border);
    border-block-start: var(--cl-line-thick) solid var(--edge);
    border-radius: var(--cl-radius-l);
    background: var(--cl-color-surface);
    color: var(--cl-color-text);
    box-shadow: var(--cl-elevation-overlay);
  }

  .dialog::backdrop {
    background: var(--cl-color-overlay);
  }

  .dialog[data-tone="neutral"] {
    --edge: var(--cl-color-border);
    --title-ink: var(--cl-color-text);
  }

  .dialog[data-tone="danger"] {
    --edge: var(--cl-color-danger);
    --title-ink: var(--cl-color-danger);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--cl-space-s);
    padding-block: var(--cl-space-2xs);
    padding-inline: var(--cl-space-m) var(--cl-space-2xs);
  }

  .title {
    margin: 0;
    color: var(--title-ink);
    font: var(--cl-type-title);
  }

  .cross {
    inline-size: 2ch;
    block-size: 2ch;
    fill: none;
    stroke: currentcolor;
    stroke-width: 2;
    stroke-linecap: round;
  }

  .body {
    display: grid;
    gap: var(--cl-space-s);
    padding-inline: var(--cl-space-m);
    padding-block-end: var(--cl-space-m);
  }

  .description {
    margin: 0;
    color: var(--cl-color-text-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--cl-space-xs);
    padding: var(--cl-space-s) var(--cl-space-m);
    border-block-start: var(--cl-line-thin) solid var(--cl-color-border);
    background: var(--cl-color-surface-sunk);
    border-end-start-radius: var(--cl-radius-l);
    border-end-end-radius: var(--cl-radius-l);
  }

  /* On a narrow dialog the buttons stack full width, the main action (last) on top. */
  @container (inline-size < 40ch) {
    .actions {
      flex-direction: column-reverse;
      align-items: stretch;
    }
  }
</style>
