import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

import { bundleGraphPlugin } from "./scripts/bundle-graph-plugin.mjs";

export default defineConfig({
  plugins: [react(), tailwindcss(), bundleGraphPlugin()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: Boolean(process.env.TAURI_DEBUG),
    rollupOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "lucide-icons",
              test: /node_modules[\\/]lucide-react/,
              tags: ["$initial"],
            },
            {
              name: "app-initial",
              test: /[\\/]src[\\/]/,
              tags: ["$initial"],
            },
          ],
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    exclude: [
      "**/node_modules/**",
      "**/.git/**",
      "**/dist/**",
      ".worktrees/**",
      "worktrees/**",
    ],
  },
});
