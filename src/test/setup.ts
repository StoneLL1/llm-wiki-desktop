import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// sigma.js references WebGL context constructors at module load time. jsdom
// does not define them, so importing anything that pulls in sigma (GraphView,
// and therefore AppShell) would crash the test suite. Provide inert stubs so
// the import succeeds; sigma rendering itself is guarded in GraphView.
if (typeof globalThis.WebGL2RenderingContext === "undefined") {
  (globalThis as { WebGL2RenderingContext?: unknown }).WebGL2RenderingContext = class {};
}
if (typeof globalThis.WebGLRenderingContext === "undefined") {
  (globalThis as { WebGLRenderingContext?: unknown }).WebGLRenderingContext = class {};
}

afterEach(cleanup);
