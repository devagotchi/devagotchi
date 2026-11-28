<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Field from "./Field.svelte";
  import { meta, type FieldType } from "./Field.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Field",
    component: Field,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const widths = Object.keys(meta.variants.width.values) as Array<keyof typeof meta.variants.width.values>;
  const types: FieldType[] = ["text", "email", "password", "url", "search", "number"];
</script>

<Story name="Every variant">
  {#each widths as width (width)}
    <Field {width} label="Machine name ({width})" value="texas" />
  {/each}
</Story>

<Story name="Every type">
  {#each types as type (type)}
    <Field {type} label="A {type} field" />
  {/each}
</Story>

<Story name="With a hint">
  <Field label="Email" type="email" autocomplete="email" hint="We send the sign-in link here." />
</Story>

<Story name="Error">
  <Field
    label="Port"
    type="number"
    value={80}
    hint="Between 1024 and 65535."
    error="Port 80 needs administrator rights on this machine. Choose a port from 1024 up, such as 8080."
  />
</Story>

<Story name="Required">
  <Field label="Name (required)" required />
</Story>

<Story name="Disabled">
  <Field label="Hub URL" type="url" value="https://hub.example" hint="Set by your organisation." disabled />
</Story>

<Story name="Multiline">
  <Field label="Description" multiline hint="What this machine is for." />
</Story>
