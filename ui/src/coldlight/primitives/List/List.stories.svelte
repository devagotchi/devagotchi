<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Badge from "../Badge/Badge.svelte";
  import List from "./List.svelte";
  import { meta } from "./List.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/List",
    component: List,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const densities = Object.keys(meta.variants.density.values) as Array<keyof typeof meta.variants.density.values>;

  interface Machine {
    id: string;
    name: string;
    online: boolean;
  }

  const machines: Machine[] = [
    { id: "m1", name: "texas", online: true },
    { id: "m2", name: "platinum", online: false },
    { id: "m3", name: "phone", online: true },
  ];
  const key = (machine: Machine) => machine.id;
</script>

{#snippet machine(entry: Machine)}
  {entry.name}
  {#if entry.online}
    <Badge tone="success">Online</Badge>
  {:else}
    <Badge tone="warning">Offline</Badge>
  {/if}
{/snippet}

<Story name="Every variant">
  {#each densities as density (density)}
    <List {density} items={machines} {key} item={machine} label="Machines ({density})" />
  {/each}
</Story>

<Story name="Empty">
  <List items={[]} {key} item={machine} label="Machines">
    {#snippet empty()}
      <p>No machines yet. Connect one to see it here.</p>
    {/snippet}
  </List>
</Story>

<Story name="One item">
  <List items={machines.slice(0, 1)} {key} item={machine} label="Machines" />
</Story>
