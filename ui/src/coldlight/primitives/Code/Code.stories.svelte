<script module lang="ts">
  import { defineMeta } from "@storybook/addon-svelte-csf";
  import Code from "./Code.svelte";
  import { meta } from "./Code.variants.ts";

  const { Story } = defineMeta({
    title: "Primitives/Code",
    component: Code,
    parameters: { docs: { description: { component: `${meta.summary} ${meta.use}` } } },
  });

  const config = `[hub]
listen = "0.0.0.0:8443"
storage = "postgres://devagotchi@localhost:55432/devagotchi?sslmode=disable&application_name=devagotchi-hub"`;
</script>

<Story name="Every display">
  <Code code="devagotchi hub serve --config hub.toml" language="bash" />
  <p>
    The hub reads <Code display="inline" code="~/.config/devagotchi/hub.toml" /> when it starts.
  </p>
</Story>

<Story name="Block with copy">
  <Code code="devagotchi machine add --name texas-workstation" language="bash" copyLabel="Copy" copiedLabel="Copied" />
</Story>

<Story name="Block without language or copy">
  <Code code="d6f1c2a9-3b7e-4f0a-9c1d-5e8b2a7f4c33" />
</Story>

<Story name="Long lines, scrolling inside">
  <div style:max-inline-size="40ch">
    <Code code={config} language="toml" copyLabel="Copy" copiedLabel="Copied" />
  </div>
</Story>

<Story name="Inline with copy">
  <p>
    Your token starts <Code display="inline" code="dg_7f3a" copyLabel="Copy" copiedLabel="Copied" />.
  </p>
</Story>

<Story name="Inline, long, wrapping">
  <p style:max-inline-size="30ch">
    Logs are under <Code display="inline" code="D:\localdev\devagotchi\wt-coldlight\ui\node_modules\.cache\storybook" />.
  </p>
</Story>
