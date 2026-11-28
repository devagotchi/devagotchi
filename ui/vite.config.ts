import adapter from "@sveltejs/adapter-static";
import { sveltekit } from "@sveltejs/kit/vite";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    // The hub serves this static build (ui.md U1); every route behind sign-in falls back to the shell.
    sveltekit({
      adapter: adapter({ fallback: "shell.html" }),
    }),
    svelteTesting(),
  ],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "scripts/**/*.test.ts"],
  },
});
