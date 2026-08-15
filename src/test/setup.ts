import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeAll } from "vitest";

import { i18nReady } from "../i18n";

beforeAll(async () => {
  await i18nReady;
});

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

// jsdom does not implement HTMLCanvasElement.prototype.getContext (it would
// need the optional native `canvas` npm package) and emits a noisy
// "Not implemented: HTMLCanvasElement.prototype.getContext" error on every
// call — three per Sigma init (webgl2 / webgl / experimental-webgl probes).
// GraphView already handles a null WebGL context via its try/catch and falls
// back to the "canvas unavailable" placeholder, so returning null here
// accurately models "WebGL unavailable" without flooding the output.
// This is environment setup, NOT console suppression: real warnings from
// GraphView (`[graph] sigma renderer init failed:`) still surface — only the
// jsdom not-implemented noise is removed. See PERF-001 in
// docs/audits/2026-07-06-performance-complexity-audit.md.
//
// The stub returns null for ALL context types (including "2d"). No src/ caller
// other than GraphView is rendered in the test suite today (graphExport.ts is
// the only other getContext user and no test exercises it); if a future test
// needs a real 2D context, give it a local stub rather than relying here.
//
// Tagged with `__silencedJSDOMCanvasNoise` so App.test.tsx can pin its
// presence as a regression guard for PERF-001.
const silencedCanvasGetContext = function getContext(): null {
  return null;
};
Object.defineProperty(silencedCanvasGetContext, "__silencedJSDOMCanvasNoise", {
  value: true,
  enumerable: false,
});
HTMLCanvasElement.prototype.getContext = silencedCanvasGetContext;

afterEach(cleanup);
