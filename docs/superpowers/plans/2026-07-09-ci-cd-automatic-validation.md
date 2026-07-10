# CI/CD Automatic Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable local and GitHub-hosted validation for the P0 review item "建立 CI/CD 自动验证".

**Architecture:** Keep validation as repository tooling, not app runtime behavior. GitHub Actions runs the same frontend and Rust service-test gates documented in the project context skill, plus a default-feature Tauri Rust compile check so CI catches GUI integration drift without running the GUI test runner.

**Tech Stack:** npm scripts, Vitest contract test, Node.js filesystem scanner, GitHub Actions, Rust cargo tests with `--no-default-features`.

## Global Constraints

- Do not introduce a database or change project content storage.
- Do not run Agent install commands or write secrets.
- CI must cover Windows, macOS, and Linux.
- Rust verification must use `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` to avoid the Windows GUI-linked test runner issue.
- Required local gates are `npm run test`, `npm run lint`, `npm run build`, console-log scan, default-feature Tauri Rust compile, and Rust no-default-features tests.

---

### Task 1: Pin The CI Contract

**Files:**
- Create: `src/test/ci-contracts.test.ts`

**Interfaces:**
- Consumes: root `package.json` and `.github/workflows/ci.yml`.
- Produces: a Vitest regression test that fails if the unified check script or CI workflow is removed.

- [x] **Step 1: Write the failing test**

```ts
expect(packageJson.scripts.check).toBe(
  "npm run test && npm run lint && npm run build && npm run check:console && npm run test:rust",
);
expect(workflow).toContain("windows-latest");
expect(workflow).toContain("cargo test --manifest-path src-tauri/Cargo.toml --no-default-features");
```

- [x] **Step 2: Run test to verify it fails**

Run: `npm run test -- src/test/ci-contracts.test.ts`
Expected: FAIL because `check` and `.github/workflows/ci.yml` are missing.

### Task 2: Add Local And Hosted Validation

**Files:**
- Modify: `package.json`
- Create: `scripts/check-console-log.mjs`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: npm, Node.js, cargo, GitHub Actions runners.
- Produces: `npm run check`, `npm run check:console`, `npm run test:rust`, and a three-platform CI workflow.

- [x] **Step 1: Add npm scripts**

```json
"check:console": "node scripts/check-console-log.mjs",
"check:rust:gui": "cargo check --manifest-path src-tauri/Cargo.toml",
"test:rust": "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features",
"check": "npm run test && npm run lint && npm run build && npm run check:console && npm run check:rust:gui && npm run test:rust"
```

- [x] **Step 2: Add console-log scanner**

```js
const roots = ["src", path.join("src-tauri", "src")];
const pattern = /\bconsole\.log\s*\(/g;
```

- [x] **Step 3: Add GitHub Actions workflow**

```yaml
strategy:
  fail-fast: false
  matrix:
    os: [windows-latest, macos-latest, ubuntu-latest]
```

### Task 3: Verify And Review

**Files:**
- Modify: `SPEC/progress.txt`

**Interfaces:**
- Consumes: repository checks and project review workflow.
- Produces: verification evidence and a progress ledger entry.

- [x] **Step 1: Run focused CI contract test**

Run: `npm run test -- src/test/ci-contracts.test.ts`
Expected: PASS.

- [x] **Step 2: Run required checks**

Run:

```powershell
npm run test
npm run lint
npm run build
npm run check:console
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
```

Expected: all pass, or report exact environment-level blocker.

- [x] **Step 3: Perform two reviews**

Subagent A reviews design intent and integration. Subagent B reviews blind spots and missing tests. If subagents are unavailable, perform both manually.
