<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Button from "./Button.svelte";
  import { meta } from "./Button.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Button",
    component: Button,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const tones = Object.keys(meta.variants.tone.values) as Array<keyof typeof meta.variants.tone.values>;
  const emphases = Object.keys(meta.variants.emphasis.values) as Array<keyof typeof meta.variants.emphasis.values>;
</script>

<Story name="Every variant">
  {#each emphases as emphasis (emphasis)}
    <p>
      {#each tones as tone (tone)}
        <Button {tone} {emphasis}>{tone} {emphasis}</Button>
      {/each}
    </p>
  {/each}
</Story>

<Story name="Busy">
  <Button tone="accent" emphasis="solid" busy>Deploying</Button>
</Story>

<Story name="Disabled">
  <Button disabled>Approve</Button>
</Story>
