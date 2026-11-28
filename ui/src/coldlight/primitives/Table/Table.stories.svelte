<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Table from "./Table.svelte";
  import { meta, type Column } from "./Table.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Table",
    component: Table,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const densities = Object.keys(meta.variants.density.values) as Array<keyof typeof meta.variants.density.values>;

  interface Machine {
    id: string;
    name: string;
    os: string;
    sessions: number;
    seen: string;
  }

  const columns: Column[] = [
    { key: "name", label: "Machine" },
    { key: "os", label: "System" },
    { key: "sessions", label: "Sessions", align: "end" },
    { key: "seen", label: "Last seen" },
  ];

  const machines: Machine[] = [
    { id: "m1", name: "texas-workstation", os: "Windows 11 Pro", sessions: 3, seen: "2026-11-28 10:02" },
    { id: "m2", name: "va-infra-prod", os: "Debian 12 arm64", sessions: 0, seen: "2026-11-28 09:58" },
    { id: "m3", name: "pixel-9", os: "Android 16", sessions: 1, seen: "2026-11-27 22:41" },
  ];

  const wide: Column[] = [
    ...columns,
    { key: "id", label: "Machine ID" },
    { key: "broker", label: "Broker version" },
    { key: "link", label: "Link state" },
  ];

  const value = (machine: Machine, key: string): string => {
    if (key === "broker") return "0.14.2";
    if (key === "link") return "Connected";
    return String(machine[key as keyof Machine]);
  };
</script>

{#snippet machineCell(machine: Machine, key: string)}
  {value(machine, key)}
{/snippet}

{#snippet noMachines()}
  No machines are connected yet. Add one from Machines, then come back.
{/snippet}

<Story name="Every density">
  {#each densities as density (density)}
    <Table caption="Machines ({density})" {columns} rows={machines} rowKey={(m) => m.id} cell={machineCell} {density} />
  {/each}
</Story>

<Story name="Caption hidden">
  <Table caption="Machines" captionHidden {columns} rows={machines} rowKey={(m) => m.id} cell={machineCell} />
</Story>

<Story name="Empty">
  <Table caption="Machines" {columns} rows={[] as Machine[]} rowKey={(m) => m.id} cell={machineCell} empty={noMachines} />
</Story>

<Story name="Wider than its container, scrolling inside">
  <div style:max-inline-size="40ch">
    <Table caption="Machines" columns={wide} rows={machines} rowKey={(m) => m.id} cell={machineCell} />
  </div>
</Story>
