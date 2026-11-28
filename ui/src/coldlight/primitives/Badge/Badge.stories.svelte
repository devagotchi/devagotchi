<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Badge from "./Badge.svelte";
  import { meta } from "./Badge.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Badge",
    component: Badge,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const tones = Object.keys(meta.variants.tone.values) as Array<keyof typeof meta.variants.tone.values>;
</script>

<Story name="Every variant">
  <p>
    {#each tones as tone (tone)}
      <Badge {tone}>{tone}</Badge>
    {/each}
  </p>
</Story>

<Story name="Beside a name">
  <p>texas <Badge tone="success">Online</Badge></p>
  <p>platinum <Badge tone="warning">Offline for 3 days</Badge></p>
  <p>deploy-hub <Badge tone="pending">Waiting for approval</Badge></p>
</Story>
