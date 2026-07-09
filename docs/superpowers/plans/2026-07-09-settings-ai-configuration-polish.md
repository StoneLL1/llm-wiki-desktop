# Settings AI Configuration Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a polished combined AI settings page with Local CLI / BYOK tabs, branded cards, and consistent visual treatment for the rest of Settings.

**Architecture:** Keep all filesystem, Agent process, and secret storage behavior behind existing props and Tauri commands. Add a focused combined AI settings component, shared brand-mark helpers, and token-driven CSS classes in `src/styles.css`. Keep Settings as a floating modal with the existing left navigation and lazy-loaded view.

**Tech Stack:** React 19, TypeScript, Vite, Tailwind CSS v4 utilities, CSS variables in `src/styles.css`, Lucide React, react-i18next, Vitest + Testing Library.

## Global Constraints

- Project content remains Markdown, JSON, and local files; no database.
- API keys and tokens must use OS credential storage and must never be written to project files, logs, screenshots, or test fixtures.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic; use existing props and Tauri IPC boundaries.
- `UI-Frontend-design/` is design reference only; do not modify it.
- Keep the UI Codex-like: compact, pane-based, quiet near-monochrome, sparse teal accent, no decorative gradients, no nested cards.
- `src/styles.css` must remain the token source for reusable styles; avoid conflicting `display` declarations for `.settings-view-layout`.
- After implementation, run `npm run check`; if it fails, fix and rerun from the beginning.

---

## File Structure

- Create `src/features/settings/BrandMark.tsx`: local brand/logo marks for Agent CLIs and BYOK providers. Uses inline SVG/CSS only, no network dependency, and exposes typed helper functions.
- Create `src/features/settings/AiSettings.tsx`: combined AI settings surface with Local CLI / BYOK segmented tabs, Agent cards, Provider cards, inline selected-detail editors, and existing prop callbacks.
- Modify `src/features/settings/SettingsView.tsx`: replace separate `Agent` and `LLM providers` nav entries with one `AI` entry and render `AiSettings`.
- Modify `src/features/settings/AgentSettings.tsx`: either remove from active rendering or keep as compatibility wrapper if tests/imports still use it.
- Modify `src/features/settings/LlmProviderSettings.tsx`: either remove from active rendering or keep as compatibility wrapper if tests/imports still use it.
- Modify `src/features/settings/provider.test.tsx`: point BYOK tests at the combined `AiSettings` flow while preserving secret non-echo assertions.
- Add `src/features/settings/AiSettings.test.tsx`: cover AI tabs, Agent card default behavior, provider selection, and status rendering.
- Modify `src/features/settings/AppearanceSettings.tsx`, `LanguageSettings.tsx`, `SecuritySettings.tsx`, `BackgroundTaskSettings.tsx`, `UpdateSettings.tsx`: apply shared section/form-row/card classes without changing behavior.
- Modify `src/styles.css`: add focused Settings AI card, brand mark, and polished settings section styles.
- Modify `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`: add combined AI tab/card copy and update nav label.
- Modify `SPEC/progress.txt`: append a reverse-chronological milestone entry after implementation passes verification.

---

## Task 1: Combined AI Component Tests

**Files:**
- Create: `src/features/settings/AiSettings.test.tsx`
- Modify: `src/features/settings/provider.test.tsx`

**Interfaces:**
- Consumes: `AgentInfo`, `AgentKind`, `ProviderStatus`, `LlmProviderConfig`, `ProviderTestResult`.
- Produces: test expectations for `AiSettings`:
  - Local CLI tab button name: `/local cli|本机 cli/i`
  - BYOK tab button name: `/byok/i`
  - Provider API key input accessible by `provider.apiKey`
  - Agent default action calls `onChangeDefault(agentKind)`

- [x] **Step 1: Write failing AI tab and Agent default tests**

Add `src/features/settings/AiSettings.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import type { AgentInfo } from "../../types/agent";
import type { ProviderStatus } from "../../types/llm";
import { AiSettings } from "./AiSettings";

const agents: AgentInfo[] = [
  {
    kind: "codex",
    command: "codex",
    state: "installed",
    version: "0.135.0",
    executablePath: "C:/bin/codex.exe",
    isDefault: false,
    installGuidance: "",
    error: null,
  },
  {
    kind: "claude",
    command: "claude",
    state: "missing",
    version: null,
    executablePath: null,
    isDefault: false,
    installGuidance: "Install Claude Code manually.",
    error: null,
  },
];

const providers: ProviderStatus[] = [
  {
    config: {
      provider: "anthropic",
      model: "claude-test",
      baseUrl: "https://api.anthropic.com",
      contextWindow: 100_000,
      enabled: true,
    },
    hasSecret: true,
    secretMask: "****test",
  },
];

function renderAi(overrides: Partial<React.ComponentProps<typeof AiSettings>> = {}) {
  return render(
    <AiSettings
      agents={agents}
      providers={providers}
      agentDefault={null}
      contextWindow={32_000}
      onRefreshAgents={vi.fn()}
      onChangeDefault={vi.fn()}
      onSaveProvider={vi.fn()}
      onSaveSecret={vi.fn()}
      onDeleteSecret={vi.fn()}
      onTestProvider={vi.fn().mockResolvedValue({ ok: true, message: "Connected" })}
      {...overrides}
    />,
  );
}

describe("AiSettings", () => {
  it("switches between Local CLI and BYOK in one AI settings surface", () => {
    renderAi();
    expect(screen.getByRole("button", { name: /local cli/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/codex cli/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));
    expect(screen.getByRole("button", { name: /byok/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/anthropic/i)).toBeInTheDocument();
  });

  it("sets an installed Agent CLI as the default route", () => {
    const onChangeDefault = vi.fn();
    renderAi({ onChangeDefault });
    fireEvent.click(screen.getByRole("button", { name: /set default codex/i }));
    expect(onChangeDefault).toHaveBeenCalledWith("codex");
  });
});
```

- [x] **Step 2: Update BYOK secret tests to render the combined component**

In `src/features/settings/provider.test.tsx`, replace direct `LlmProviderSettings` rendering with `AiSettings`, click BYOK before finding the key input, and keep assertions that the raw secret is not echoed:

```tsx
fireEvent.click(screen.getByRole("button", { name: /byok/i }));
const input = screen.getByLabelText(/API key/i) as HTMLInputElement;
fireEvent.change(input, { target: { value: "sk-secret-value" } });
fireEvent.click(screen.getByRole("button", { name: /save key/i }));
await screen.findByText(/saved/i);
expect(saveSecret).toHaveBeenCalled();
expect(input.value).toBe("");
expect(screen.queryByText("sk-secret-value")).not.toBeInTheDocument();
```

- [x] **Step 3: Run tests and confirm failure**

Run:

```powershell
npm run test -- src/features/settings/AiSettings.test.tsx src/features/settings/provider.test.tsx
```

Expected: fails because `AiSettings` does not exist or the combined tabs are not implemented.

---

## Task 2: Brand Marks And Combined AI Settings

**Files:**
- Create: `src/features/settings/BrandMark.tsx`
- Create: `src/features/settings/AiSettings.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`

**Interfaces:**
- `BrandMark` props:
  - `kind: AgentKind | LlmProviderKind`
  - `type: "agent" | "provider"`
  - `className?: string`
- `agentDisplay(agent: AgentInfo): { name: string; vendor: string; descriptionKey: string }`
- `providerDisplay(kind: LlmProviderKind): { nameKey: string; descriptionKey: string }`
- `AiSettingsProps`:
  - `agents: AgentInfo[]`
  - `providers: ProviderStatus[]`
  - `agentDefault: AgentKind | null`
  - `contextWindow: number`
  - existing callback props from `AgentSettings` and `LlmProviderSettings`

- [x] **Step 1: Implement local brand marks**

Create `BrandMark.tsx` with deterministic inline marks:

```tsx
import type { AgentKind } from "../../types/agent";
import type { LlmProviderKind } from "../../types/llm";

type BrandKind = AgentKind | LlmProviderKind;

interface BrandMarkProps {
  kind: BrandKind;
  type: "agent" | "provider";
  className?: string;
}

export function BrandMark({ kind, type, className = "" }: BrandMarkProps) {
  return (
    <span className={`settings-brand settings-brand--${type} settings-brand--${kind} ${className}`} aria-hidden="true">
      <BrandGlyph kind={kind} />
    </span>
  );
}

function BrandGlyph({ kind }: { kind: BrandKind }) {
  if (kind === "codex" || kind === "open_ai") return <span className="settings-brand__openai">◎</span>;
  if (kind === "claude" || kind === "anthropic") return <span className="settings-brand__anthropic">✶</span>;
  if (kind === "google") return <span className="settings-brand__google">G</span>;
  if (kind === "ollama") return <span className="settings-brand__ollama">O</span>;
  if (kind === "custom") return <span className="settings-brand__custom">↗</span>;
  if (kind === "openclaw") return <span className="settings-brand__openclaw">OC</span>;
  if (kind === "hermes") return <span className="settings-brand__hermes">H</span>;
  return <span>{String(kind).slice(0, 2).toUpperCase()}</span>;
}
```

- [x] **Step 2: Implement `AiSettings.tsx`**

Create the component with two tabs, Agent card list, Provider card list, and inline editor. Use existing callbacks only. The secret input must clear after save:

```tsx
const saveKey = async () => {
  await onSaveSecret(activeProvider, secret);
  setSecret("");
  setSaved(true);
};
```

Use accessible button labels:

```tsx
aria-label={t("settings.ai.agent.setDefaultFor", { agent: meta.name })}
aria-pressed={activeTab === "cli"}
```

- [x] **Step 3: Add i18n keys**

Add English and Chinese keys:

```json
"settings.nav.ai": "AI",
"settings.ai.title": "AI route",
"settings.ai.description": "Choose between a local Agent CLI and BYOK providers for this project.",
"settings.ai.tab.cli": "Local CLI",
"settings.ai.tab.byok": "BYOK",
"settings.ai.agent.count": "{{count}} CLI(s)",
"settings.ai.agent.setDefaultFor": "Set default {{agent}}",
"settings.ai.agent.defaultBadge": "Default",
"settings.ai.agent.detailTitle": "Route details",
"settings.ai.agent.modelLabel": "Model",
"settings.ai.agent.modelDefault": "Default (CLI config)",
"settings.ai.provider.count": "{{count}} provider(s)",
"settings.ai.provider.detailTitle": "Provider details"
```

Use equivalent Chinese strings:

```json
"settings.nav.ai": "AI 配置",
"settings.ai.title": "AI 路线",
"settings.ai.description": "为当前项目选择本机 Agent CLI 或 BYOK Provider。",
"settings.ai.tab.cli": "本机 CLI",
"settings.ai.tab.byok": "BYOK",
"settings.ai.agent.count": "{{count}} 个 CLI",
"settings.ai.agent.setDefaultFor": "设为默认 {{agent}}",
"settings.ai.agent.defaultBadge": "默认",
"settings.ai.agent.detailTitle": "路线详情",
"settings.ai.agent.modelLabel": "模型",
"settings.ai.agent.modelDefault": "Default（CLI 配置）",
"settings.ai.provider.count": "{{count}} 个 Provider",
"settings.ai.provider.detailTitle": "Provider 详情"
```

- [x] **Step 4: Run focused tests**

Run:

```powershell
npm run test -- src/features/settings/AiSettings.test.tsx src/features/settings/provider.test.tsx
```

Expected: tests pass or fail only on style/copy mismatches that are fixed in this task.

---

## Task 3: SettingsView Integration

**Files:**
- Modify: `src/features/settings/SettingsView.tsx`
- Modify: `src/features/settings/AgentSettings.tsx`
- Modify: `src/features/settings/LlmProviderSettings.tsx`

**Interfaces:**
- Consumes: `AiSettings` from Task 2.
- Produces: one nav section key `"ai"` in `SettingsSectionKey`.

- [x] **Step 1: Replace nav entries**

Change `SettingsSectionKey` to include `"ai"` and remove `"agent"` / `"providers"` from active rendering. In `NAV_GROUPS`, the AI group contains one item:

```ts
{
  labelKey: "settings.nav.group.ai",
  items: [
    { key: "ai", labelKey: "settings.nav.ai", icon: Cpu },
  ],
}
```

- [x] **Step 2: Render `AiSettings`**

Replace the `activeSection === "agent"` and `activeSection === "providers"` blocks with:

```tsx
{activeSection === "ai" ? (
  <AiSettings
    agents={agents}
    providers={providerStatuses}
    agentDefault={settings.agentDefault}
    contextWindow={settings.contextWindow}
    onRefreshAgents={() => { void onRefreshCapabilities(); }}
    onChangeDefault={(agentDefault) => { void savePatch({ agentDefault }, true); }}
    onSaveProvider={(config) => onSaveProvider({ ...config, contextWindow: settings.contextWindow })}
    onSaveSecret={async (provider, secret) => {
      await onSaveSecret(provider, secret);
      await onRefreshCapabilities();
    }}
    onDeleteSecret={async (provider) => {
      await onDeleteSecret(provider);
      await onRefreshCapabilities();
    }}
    onTestProvider={(config) => onTestProvider({ ...config, contextWindow: settings.contextWindow })}
  />
) : null}
```

- [x] **Step 3: Keep compatibility components if still imported by tests**

If `AgentSettings.tsx` and `LlmProviderSettings.tsx` are no longer imported by production code but tests still import them, keep them as thin wrappers or update tests to import `AiSettings`.

- [x] **Step 4: Run Settings tests**

Run:

```powershell
npm run test -- src/features/settings src/app/App.test.tsx
```

Expected: Settings and app shell tests pass.

---

## Task 4: Settings Visual Polish CSS And Secondary Sections

**Files:**
- Modify: `src/styles.css`
- Modify: `src/features/settings/AppearanceSettings.tsx`
- Modify: `src/features/settings/LanguageSettings.tsx`
- Modify: `src/features/settings/SecuritySettings.tsx`
- Modify: `src/features/settings/BackgroundTaskSettings.tsx`
- Modify: `src/features/settings/UpdateSettings.tsx`
- Modify: `src/features/settings/SettingsView.tsx`

**Interfaces:**
- Produces reusable CSS classes:
  - `.settings-section-panel`
  - `.settings-formrow`
  - `.settings-choice-grid`
  - `.settings-choice-card`
  - `.settings-ai-tabs`
  - `.settings-ai-card`
  - `.settings-ai-detail`
  - `.settings-brand`

- [x] **Step 1: Add CSS classes without changing `.settings-view-layout` display**

Append focused settings classes near the existing Settings block. Do not add a new `display` declaration to `.settings-view-layout`.

```css
.settings-section-panel {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}
.settings-formrow {
  display: grid;
  grid-template-columns: minmax(160px, 220px) minmax(0, 1fr);
  gap: var(--sp-4);
  padding: var(--sp-4) 0;
  border-bottom: 1px solid var(--border-subtle);
}
.settings-formrow:last-child { border-bottom: 0; }
.settings-choice-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: var(--sp-3);
}
.settings-choice-card,
.settings-ai-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--background);
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}
.settings-ai-card {
  display: grid;
  grid-template-columns: 36px minmax(0, 1fr) auto;
  gap: var(--sp-3);
  align-items: center;
  padding: var(--sp-3);
  text-align: left;
}
.settings-ai-card:hover { border-color: var(--border-strong); background: var(--surface); }
.settings-ai-card.is-active {
  border-color: var(--accent-border);
  background: var(--accent-soft);
}
.settings-ai-detail {
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--surface-raised);
  padding: var(--sp-4);
}
```

- [x] **Step 2: Apply shared row/card classes to secondary settings sections**

Move one-off card button class strings to the new classes where behavior stays identical. Example for `BackgroundTaskSettings`:

```tsx
<div className="settings-choice-grid">
  {(["minimize_to_tray", "quit"] as const).map((option) => (
    <button className={`settings-choice-card p-3 text-left ${selected ? "is-selected" : ""}`}>
```

- [x] **Step 3: Add responsive rules**

Add:

```css
@media (max-width: 900px) {
  .settings-formrow,
  .settings-ai-card {
    grid-template-columns: minmax(0, 1fr);
  }
  .settings-ai-card__actions {
    justify-content: flex-start;
  }
}
```

- [x] **Step 4: Run CSS contract and settings tests**

Run:

```powershell
npm run test -- src/test/ui-css-contracts.test.ts src/features/settings
```

Expected: pass.

---

## Task 5: Logo Source Check, Progress Log, Full Verification

**Files:**
- Modify: `SPEC/progress.txt`
- Optionally modify: `SPEC/gotchas.txt` only if a subtle recurring issue appears.

**Interfaces:**
- Produces: progress entry format `[YYYY-MM-DD] Module/Task — Summary — Key decision or open issue`.

- [x] **Step 1: Verify logo source decision**

Use official web references for visual comparison, but keep app rendering local. If official logo assets are not safely reusable, document fallback behavior in the progress entry. Do not hotlink remote image URLs from the app.

- [x] **Step 2: Run focused tests**

Run:

```powershell
npm run test -- src/features/settings src/test/ui-css-contracts.test.ts
```

Expected: pass.

- [x] **Step 3: Run full check**

Run:

```powershell
npm run check
```

Expected: pass. If it fails, fix the issue and rerun `npm run check` from the beginning.

- [x] **Step 4: Append progress entry**

Insert at the top of `SPEC/progress.txt` after the title:

```text
[2026-07-09] Settings AI configuration polish — Combined Agent CLI and BYOK into one Settings AI surface with Local CLI/BYOK tabs, branded local cards, keychain-safe BYOK editing, and consistent compact Settings section styling — Key decision: render brand marks locally for offline reliability, using official sources only as visual references and safe fallbacks where assets are unclear.
```

- [x] **Step 5: Run final full check after progress update**

Run:

```powershell
npm run check
```

Expected: pass.

---

## Self-Review

Spec coverage:

- Combined AI section is covered by Tasks 1-3.
- Agent CLI cards and BYOK cards are covered by Tasks 1-4.
- Logo sourcing and local fallback are covered by Tasks 2 and 5.
- Remaining Settings polish is covered by Task 4.
- Secret safety and provider tests are covered by Tasks 1, 2, and 5.
- Required `npm run check` and progress logging are covered by Task 5.

Placeholder scan:

- No TBD/TODO placeholders remain.
- Each task names files, interfaces, commands, and expected outputs.

Type consistency:

- `AiSettingsProps`, `AgentKind`, `LlmProviderKind`, `ProviderStatus`, and callback names match existing project types and current Settings props.
