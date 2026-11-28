<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Button from "../Button/Button.svelte";
  import Notice from "./Notice.svelte";
  import { meta } from "./Notice.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Notice",
    component: Notice,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const tones = Object.keys(meta.variants.tone.values) as Array<keyof typeof meta.variants.tone.values>;
</script>

<Story name="Every variant">
  {#each tones as tone (tone)}
    <Notice {tone} title="A {tone} notice">
      {meta.variants.tone.values[tone]}
    </Notice>
  {/each}
</Story>

<Story name="Refused, with a fix">
  <Notice tone="danger" title="Deploy refused">
    Port 80 needs administrator rights on texas, and the broker runs as an ordinary account.
    {#snippet next()}
      <Button tone="accent" emphasis="solid">Use port 8080</Button>
    {/snippet}
  </Notice>
</Story>

<Story name="Warning, with two next steps">
  <Notice tone="warning" title="Token expires in 2 days">
    Agents using it will be refused once it expires.
    {#snippet next()}
      <Button>Remind me later</Button>
      <Button tone="accent" emphasis="solid">Renew now</Button>
    {/snippet}
  </Notice>
</Story>

<Story name="Live">
  <Notice tone="success" title="Settings saved" live>
    Audit line 4812 records the change.
  </Notice>
  <Notice tone="danger" title="Approval failed" live>
    The approver's session ended before they answered.
  </Notice>
</Story>

<Story name="Narrow">
  <div style:max-inline-size="30ch">
    <Notice tone="danger" title="Deploy refused">
      Port 80 needs administrator rights.
      {#snippet next()}
        <Button tone="accent" emphasis="solid">Use port 8080</Button>
      {/snippet}
    </Notice>
  </div>
</Story>
