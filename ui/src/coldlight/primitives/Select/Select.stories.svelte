<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Select from "./Select.svelte";
  import { meta, type SelectOption } from "./Select.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Select",
    component: Select,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const widths = Object.keys(meta.variants.width.values) as Array<keyof typeof meta.variants.width.values>;
  const regions: SelectOption[] = [
    { value: "eu", label: "Europe" },
    { value: "us", label: "United States" },
    { value: "ap", label: "Asia Pacific (full)", disabled: true },
  ];
</script>

<Story name="Every variant">
  {#each widths as width (width)}
    <Select {width} label="Region ({width})" options={regions} value="eu" />
  {/each}
</Story>

<Story name="With a hint">
  <Select label="Region" options={regions} hint="Where this board's data is kept." />
</Story>

<Story name="Error">
  <Select
    label="Region"
    options={regions}
    value="us"
    error="Your organisation keeps its data in Europe. Choose Europe, or ask an owner to change the policy."
  />
</Story>

<Story name="Required">
  <Select label="Region (required)" options={regions} required />
</Story>

<Story name="Disabled">
  <Select label="Region" options={regions} value="eu" hint="Set by your organisation." disabled />
</Story>
