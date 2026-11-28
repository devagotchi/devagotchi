import type { Preview } from "@storybook/sveltekit";
import "../src/coldlight/tokens/build/tokens.css";

/** Every story renders in both themes, and an accessibility failure fails the story (U7, U13). */
const preview: Preview = {
  globalTypes: {
    theme: {
      description: "ColdLight theme",
      toolbar: { title: "Theme", items: ["light", "dark"], dynamicTitle: true },
    },
  },
  initialGlobals: { theme: "light" },
  decorators: [
    (story, context) => {
      document.documentElement.dataset.theme = String(context.globals.theme ?? "light");
      return story();
    },
  ],
  parameters: { a11y: { test: "error" } },
};

export default preview;
