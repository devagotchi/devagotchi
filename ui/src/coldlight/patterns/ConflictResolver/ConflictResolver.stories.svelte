<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Button from "../../primitives/Button/Button.svelte";
  import ConflictResolver from "./ConflictResolver.svelte";
  import { meta, type ConflictLabels } from "./ConflictResolver.variants.ts";

  const { Story } = defineMeta({
    title: "Patterns/ConflictResolver",
    component: ConflictResolver,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const labels: ConflictLabels = {
    legend: "Which version should be written?",
    existing: "Keep what is there now",
    existingHint: "Your change is dropped; the file stays as someone else left it.",
    incoming: "Use your change",
    incomingHint: "The newer version on the machine is replaced by yours.",
    merged: "Merge both",
    mergedHint: "Both changes are kept, as the hub merged them.",
    preview: "What will be written",
    apply: "Write it",
    cancel: "Cancel",
    close: "Close",
    added: "Added",
    removed: "Removed",
    noChange: "Nothing changes: the file stays as it is.",
    counts: (added, removed) => `${added} lines added, ${removed} removed`,
  };

  const existing = '[hub]\nport = 8080\nchannel = "main"\nmachines = 3\n';
  const incoming = '[hub]\nport = 8443\nchannel = "main"\nmachines = 3\n';
  const merged = '[hub]\nport = 8443\nchannel = "main"\nmachines = 3\ninvites = true\n';
</script>

<script lang="ts">
  let openWithMerge = $state(true);
  let openWithoutMerge = $state(true);
</script>

<Story name="Incoming, existing or both">
  <Button onclick={() => (openWithMerge = true)}>Show the conflict</Button>
  <ConflictResolver
    bind:open={openWithMerge}
    title="hub.toml changed while you were editing"
    description="Someone saved hub.toml on the machine after you opened it. Choose what to write."
    name="hub.toml"
    {existing}
    {incoming}
    {merged}
    {labels}
    onresolve={() => {}}
  />
</Story>

<Story name="No merge offered">
  <Button onclick={() => (openWithoutMerge = true)}>Show the conflict</Button>
  <ConflictResolver
    bind:open={openWithoutMerge}
    title="hub.toml changed while you were editing"
    name="hub.toml"
    {existing}
    {incoming}
    {labels}
    onresolve={() => {}}
  />
</Story>
