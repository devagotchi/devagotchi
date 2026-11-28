<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Tabs from "./Tabs.svelte";
  import { meta, type Tab } from "./Tabs.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Tabs",
    component: Tabs,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const views: Tab[] = [
    { id: "overview", label: "Overview" },
    { id: "sessions", label: "Sessions" },
    { id: "logs", label: "Logs" },
  ];

  const withDisabled: Tab[] = [
    { id: "overview", label: "Overview" },
    { id: "gui", label: "GUI sessions", disabled: true },
    { id: "logs", label: "Logs" },
  ];

  const many: Tab[] = [
    "Overview",
    "Sessions",
    "GUI sessions",
    "Logs",
    "Grants",
    "Requests",
    "Role bindings",
    "Audit",
    "Settings",
  ].map((label) => ({ id: label.toLowerCase().replace(/ /g, "-"), label }));
</script>

{#snippet viewPanel(id: string)}
  <p>The {id} view of texas-workstation.</p>
{/snippet}

<Story name="Default">
  <Tabs label="Machine views" tabs={views} panel={viewPanel} />
</Story>

<Story name="Second tab selected">
  <Tabs label="Machine views" tabs={views} selected="sessions" panel={viewPanel} />
</Story>

<Story name="With a disabled tab">
  <Tabs label="Machine views" tabs={withDisabled} panel={viewPanel} />
</Story>

<Story name="More tabs than fit, scrolling inside">
  <div style:max-inline-size="40ch">
    <Tabs label="Machine views" tabs={many} selected="audit" panel={viewPanel} />
  </div>
</Story>
