<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Badge from "../Badge/Badge.svelte";
  import Button from "../Button/Button.svelte";
  import Card from "./Card.svelte";
  import { meta } from "./Card.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Card",
    component: Card,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const emphases = Object.keys(meta.variants.emphasis.values) as Array<keyof typeof meta.variants.emphasis.values>;
</script>

<Story name="Every variant">
  {#each emphases as emphasis (emphasis)}
    <Card {emphasis} title="A {emphasis} card" description={meta.variants.emphasis.values[emphasis]}>
      <p>Its content.</p>
    </Card>
  {/each}
</Story>

<Story name="With actions">
  <Card title="texas" description="Windows 11 · last seen just now">
    {#snippet actions()}
      <Button>Inspect</Button>
      <Button tone="accent" emphasis="solid">Open</Button>
    {/snippet}
    <p><Badge tone="success">Online</Badge> Two agents running.</p>
  </Card>
</Story>

<Story name="Narrow, with actions">
  <div style:max-inline-size="30ch">
    <Card title="texas" description="Windows 11">
      {#snippet actions()}
        <Button>Inspect</Button>
      {/snippet}
      <p>Two agents running.</p>
    </Card>
  </div>
</Story>

<Story name="Heading level">
  <h2>Fleet</h2>
  <Card title="Machines" headingLevel={3}>
    <p>The title is an h3, under the section's h2.</p>
  </Card>
</Story>

<Story name="Content only">
  <Card>
    <p>A card with no title is a plain surface.</p>
  </Card>
</Story>
