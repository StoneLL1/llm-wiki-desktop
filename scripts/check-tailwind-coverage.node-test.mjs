import assert from "node:assert/strict";
import test from "node:test";

import {
  evaluateTailwindCoverage,
  formatCoverageFailure,
} from "./check-tailwind-coverage.mjs";

test("evaluateTailwindCoverage passes when every required selector is present", () => {
  const result = evaluateTailwindCoverage({
    cssText: ".flex{display:flex}.overflow-x-auto{overflow-x:auto}.pr-7{padding-right:calc(var(--spacing)*7)}.min-w-\\[132px\\]{min-width:132px}.py-\\[4px\\]{padding-block:4px}",
    requiredSelectors: [".overflow-x-auto", ".pr-7", ".min-w-\\[132px\\]", ".py-\\[4px\\]"],
  });

  assert.equal(result.checkedCount, 4);
  assert.deepEqual(result.missing, []);
});

test("evaluateTailwindCoverage reports only the selectors missing from the CSS", () => {
  const result = evaluateTailwindCoverage({
    cssText: ".flex{display:flex}.overflow-x-auto{overflow-x:auto}",
    requiredSelectors: [".overflow-x-auto", ".pr-7", ".py-\\[4px\\]"],
  });

  assert.deepEqual(result.missing, [".pr-7", ".py-\\[4px\\]"]);
});

test("evaluateTailwindCoverage does not confuse responsive variants with base utilities", () => {
  const result = evaluateTailwindCoverage({
    cssText: "@media (min-width:640px){.sm\\:overflow-x-auto{overflow-x:auto}}",
    requiredSelectors: [".overflow-x-auto"],
  });

  assert.deepEqual(result.missing, [".overflow-x-auto"]);
});

test("formatCoverageFailure lists missing selectors and points at the gitignore trap", () => {
  const message = formatCoverageFailure({
    checkedCount: 1,
    missing: [".overflow-x-auto"],
  });

  assert.match(message, /missing from dist CSS: \.overflow-x-auto/);
  assert.match(message, /\.gitignore/);
  assert.match(message, /`\/wiki\/`, never `wiki\/`/);
});
