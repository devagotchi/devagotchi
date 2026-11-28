import type { StorybookConfig } from "@storybook/sveltekit";

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.svelte"],
  addons: ["@storybook/addon-svelte-csf", "@storybook/addon-a11y"],
  framework: { name: "@storybook/sveltekit", options: {} },
  core: { disableTelemetry: true },
};

export default config;
