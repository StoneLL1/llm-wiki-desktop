import type { Plugin } from "vite";

export function bundleGraphPlugin(options?: {
  root?: string;
  fileName?: string;
}): Plugin;
