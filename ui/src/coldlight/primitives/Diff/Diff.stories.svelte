<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Diff from "./Diff.svelte";
  import { meta } from "./Diff.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Diff",
    component: Diff,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const current = '[hub]\nport = 8080\nchannel = "main"\nmachines = 3\n';
  const incoming = '[hub]\nport = 8443\nchannel = "main"\nmachines = 3\ninvites = true\n';
  const prose =
    "Friends and family sign in with the link you send them. Each one sees only the machines you share with them, and every action is recorded in the audit log.";
</script>

<Story name="A change to a file">
  <Diff label="hub.toml" before={current} after={incoming} addedLabel="Added" removedLabel="Removed" />
</Story>

<Story name="Wrapped prose">
  <Diff
    label="Invitation text"
    wrap="wrap"
    before={prose}
    after={prose.replace("the link you send them", "the one-time link you send them")}
    addedLabel="Added"
    removedLabel="Removed"
  />
</Story>

<Story name="No change">
  <Diff label="hub.toml" before={current} after={current} addedLabel="Added" removedLabel="Removed">
    {#snippet unchanged()}Both versions are the same; there is nothing to write.{/snippet}
  </Diff>
</Story>
