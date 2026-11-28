<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Button from "../Button/Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { meta } from "./Dialog.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Dialog",
    component: Dialog,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  let closedOpen = $state(false);
</script>

<Story name="Neutral">
  <Dialog open title="Rename machine" description="The new name shows on every board." closeLabel="Close">
    <p>texas-workstation will be renamed for everyone on this hub.</p>
    {#snippet actions()}
      <Button>Cancel</Button>
      <Button tone="accent" emphasis="solid">Rename</Button>
    {/snippet}
  </Dialog>
</Story>

<Story name="Danger">
  <Dialog open tone="danger" title="Revoke this token?" description="Agents using it lose access at once." closeLabel="Close">
    <p>The token ending in 7f3a is used by two agents.</p>
    {#snippet actions()}
      <Button>Keep it</Button>
      <Button tone="danger" emphasis="solid">Revoke</Button>
    {/snippet}
  </Dialog>
</Story>

<Story name="Without actions">
  <Dialog open title="Keyboard shortcuts" closeLabel="Close">
    <p>Press ? anywhere to open this list.</p>
  </Dialog>
</Story>

<Story name="Closed, with its trigger">
  <Button onclick={() => (closedOpen = true)}>Open the dialog</Button>
  <Dialog bind:open={closedOpen} title="Rename machine" closeLabel="Close">
    <p>Escape or the close button closes it.</p>
  </Dialog>
</Story>
