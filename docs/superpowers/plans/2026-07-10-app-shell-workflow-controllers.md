# AppShell Workflow Controllers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `AppShell.tsx` 中的 Import、Provider、Agent、task 和 confirmation 业务编排下沉到 feature hooks/controllers，使 AppShell 只负责 Codex-like 桌面布局、pane 行为和全局壳层接线。

**Architecture:** 采用“纯 shell + workspace controller + feature workflows”三层：AppShell 保留布局与快捷键，WorkspaceController 组合项目级 controller，WorkspaceRouter 负责 lazy view dispatch；Import、Provider、Agent 各自拥有 hook，并共享 capability 与 task launcher 基础 hook。所有文件/Git/Agent/secret 操作仍只通过 typed Tauri IPC 进入 Rust 后端。

**Tech Stack:** React 19、TypeScript、Zustand、Tauri v2 IPC、Vitest、Testing Library、react-i18next、React.lazy/Suspense、ViewErrorBoundary。

## Global Constraints

- 不改变用户可见布局、CSS token、字体、间距、导航层级或中英文文案；`UI-Frontend-design/` 只读。
- AppShell 仍保持 top bar、left sidebar、center workspace、right context panel、bottom status bar 的 Codex-like shell。
- React 不直接执行文件系统、Git、Agent process 或 secret storage；workflow 只能调用 typed Tauri commands/stores。
- Import preview、confirm、source delete/replace 与 compile-after-import 的当前时序和显式确认必须保持。
- URL Readability 继续动态导入；不得把 `@mozilla/readability` 拉回首屏 bundle。
- 每个 `React.lazy` 继续由同址 `Suspense` + `ViewErrorBoundary` 保护；导航切换仍重置错误边界。
- Agent CLI 不静默安装；provider secret 不进入 store、日志或错误 toast。
- 长任务继续写入 `taskStore`、打开统一 TaskLogDrawer，并可取消；不得在 feature 内建立第二套 task 状态。
- 项目快速切换时，旧项目的 import preview、agent/provider probe 或任务结果不得写入新项目状态。
- 不改变 Tauri command 名、请求/响应 DTO、Zustand 持久化键或 feature view props 的用户语义。
- 不新增前端依赖；本任务是职责重构，不是 UI 重设计。
- 完成后必须运行 `npm run check`；失败则修复并从完整命令开头重跑。

---

## 1. 架构审阅结论

### 1.1 建议是否靠谱

结论：**建议成立，而且比单纯缩短 AppShell 行数更重要；但必须避免制造一个新的万能 hook。**

当前 `AppShell.tsx` 768 行，其中 `WorkspaceView` 同时承担：

- project switch 后清理 Import 状态、进入 Import 时拉 source list；
- Agent/Provider capability probe 与 `agentRoute` 回写；
- compile task 启动和 task drawer 接线；
- 文件/文件夹、clipboard、URL 三种 Import preview；
- source delete/replace PendingAction；
- confirm import 后 scan Wiki，再可选 compile；
- provider config/secret save、delete、test；
- default Agent 更新；
- Agent skill 到 compile/lint/chat/export 的路由；
- task cancel；
- lazy view dispatch、SettingsDialog、RunAgentDialog。

这些逻辑本身大多没有越过 Tauri 边界，但它们被放在 shell 文件里，导致任何 feature 改动都修改全局布局组件，测试也只能通过大型 AppShell 集成场景覆盖。

### 1.2 不采纳的两种方案

1. **不创建单个 `useWorkspaceController` 巨型 hook。** 这只会把 300 行回调从 `.tsx` 搬到 `.ts`，Import/Provider/Agent 仍然强耦合。
2. **不把所有 orchestration 塞入 Zustand store。** Store 适合可观察状态和稳定 action，不适合承载 URL Readability 动态加载、view navigation、dialog 生命周期与多 command 流程；否则 store 会变成隐藏的 service locator。

### 1.3 采用的方案

```text
AppShell.tsx
  ├── shell layout / pane resize / global shortcuts
  ├── WorkspaceController
  ├── ProjectConfirmationController
  └── Toaster + TaskLogDrawer

WorkspaceController
  ├── useAiCapabilities
  ├── useTaskLauncher
  ├── useImportWorkflow
  ├── useProviderWorkflow
  ├── useAgentWorkflow
  ├── WorkspaceRouter
  ├── SettingsDialog
  └── RunAgentDialog
```

职责的关键区别：

- **hook** 管理一个业务流程的状态、异步 guard 与命令序列；
- **controller component** 把 hook 返回值接到现有 view/dialog props；
- **router** 只做 view → component 映射和 lazy boundary；
- **AppShell** 只做 frame/layout，不知道 import DTO、provider secret 或 Agent skill。

### 1.4 审阅后的补充决策

- 报告只举了三个 hook 示例，但若没有共享 `useAiCapabilities` 与 `useTaskLauncher`，Provider/Agent 两边仍会重复 probe 和 task 接线。因此计划增加两个小型共享 hook。
- source delete/replace 的全局确认目前在 AppShell；要真正达到“AppShell 只接 layout”，必须另建 `ProjectConfirmationController`，否则高风险流程仍留在 shell。
- view dispatch 不是业务 hook，应该抽成 `WorkspaceRouter.tsx`；不要用 React Router 重写当前内部 view-state，因为 SPEC 明确不规定 URL router，本轮也没有收益。
- `useProjectStatus` 已缓存另一套 agent/provider probe。实施时先不合并 Git status hook，但要记录重复探测：`useAiCapabilities` 是可变管理源，`useProjectStatus` 是只读 shell snapshot。后续可统一缓存，不能在本轮无测试地共享可变 module cache。

## 2. 目标文件结构

```text
src/
├── components/app/
│   ├── AppShell.tsx                         # 仅 shell/layout/global wiring
│   ├── WorkspaceController.tsx              # 组合 workflows 与 dialogs
│   ├── WorkspaceRouter.tsx                  # lazy view dispatch + boundary
│   ├── ProjectConfirmationController.tsx    # PendingAction/compile confirmation
│   ├── appShellActions.test.tsx             # 保留 shell 行为测试
│   ├── WorkspaceController.test.tsx
│   ├── WorkspaceRouter.test.tsx
│   └── ProjectConfirmationController.test.tsx
├── features/import/
│   ├── useImportWorkflow.ts
│   └── useImportWorkflow.test.tsx
├── features/settings/
│   ├── useProviderWorkflow.ts
│   └── useProviderWorkflow.test.tsx
├── features/agent/
│   ├── useAgentWorkflow.ts
│   └── useAgentWorkflow.test.tsx
├── hooks/
│   ├── useAiCapabilities.ts
│   ├── useAiCapabilities.test.tsx
│   ├── useTaskLauncher.ts
│   └── useTaskLauncher.test.tsx
├── stores/
│   ├── projectStore.ts                      # 增加 guarded setAgentRoute
│   └── projectStore.test.ts                 # 项目切换 guard
└── test/
    └── app-shell-architecture.test.ts        # 防止领域编排回流
```

## 3. Controller 合同

### 3.1 `useAiCapabilities`

```ts
export interface AiCapabilitiesWorkflow {
  agents: AgentInfo[];
  providers: ProviderStatus[];
  refreshing: boolean;
  refresh: () => Promise<void>;
}

export function useAiCapabilities(
  project: ProjectSummary,
  refreshWhenVisible: boolean,
): AiCapabilitiesWorkflow;
```

### 3.2 `useTaskLauncher`

```ts
export interface TaskLaunchOptions {
  route: "auto" | "agent" | "byok";
  agent: AgentKind | null;
  provider: LlmProviderKind | null;
}

export interface TaskLauncher {
  startCompile: (options?: Partial<TaskLaunchOptions>) => Promise<BackendTask>;
  startDeepLint: (options: TaskLaunchOptions) => Promise<BackendTask>;
  startExport: (
    exportType: ExportType,
    sourcePath: string | null,
    options: TaskLaunchOptions,
  ) => Promise<BackendTask>;
  cancel: (taskId: string) => Promise<void>;
}
```

所有 start 方法内部统一 `invoke → upsertTask → openTaskDrawer → return task`。

### 3.3 `useImportWorkflow`

```ts
export interface ImportWorkflow {
  importedSources: ImportedSource[];
  isConfirming: boolean;
  requestPreview: (paths: string[]) => void;
  requestClipboard: (content: string) => Promise<void>;
  requestUrl: (url: string) => Promise<void>;
  requestDeleteSource: (path: string) => Promise<void>;
  requestReplaceSource: (path: string, replacementPath: string) => Promise<void>;
  confirm: (options: {
    createCheckpoint: boolean;
    compileAfterImport: boolean;
  }) => void;
}
```

### 3.4 `useProviderWorkflow`

```ts
export interface ProviderWorkflow {
  providers: ProviderStatus[];
  saveProvider: (config: LlmProviderConfig) => Promise<void>;
  saveSecret: (provider: LlmProviderKind, secret: string) => Promise<void>;
  deleteSecret: (provider: LlmProviderKind) => Promise<void>;
  testProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}
```

### 3.5 `useAgentWorkflow`

```ts
export interface AgentWorkflow {
  agents: AgentInfo[];
  defaultAgentKind: AgentKind | null;
  dialogOpen: boolean;
  dialogPreset: AgentSkill | undefined;
  openRunDialog: (preset?: AgentSkill) => void;
  closeRunDialog: () => void;
  setDefaultAgent: (agent: AgentKind) => Promise<void>;
  runAgent: (options: RunAgentOptions) => Promise<void>;
}
```

## 4. 数据与错误流

```text
Feature View event
  → feature workflow validates UI input
  → typed Tauri invoke
  → Rust command/service executes protected operation
  → workflow updates existing Zustand state
  → task workflow opens shared log drawer OR toast reports actionable error
```

异步 guard 统一使用 `projectKey = projectId + "\0" + rootPath`：

- 发起请求时捕获 key；
- await 返回后对比 ref 中最新 key；
- 只有 key 相同才提交 store state；
- 项目切换 effect 先清理 Import staging，再开始新 probe；
- 不通过闭包中的旧 `currentProject` 覆盖新项目整个 summary。

---

### Task 1: 建立 AppShell 架构与行为基线

**Files:**
- Create: `src/test/app-shell-architecture.test.ts`
- Modify: `src/components/app/appShellActions.test.tsx`
- Read: `src/components/app/AppShell.tsx`
- Read: `src/components/app/ViewErrorBoundary.tsx`

**Interfaces:**
- Consumes: 当前 AppShell 源码与现有 shell tests。
- Produces: 防止领域 imports/commands 回流的静态契约，以及重构前 UI 行为基线。

- [ ] **Step 1: 写架构契约测试并先让它描述目标状态**

```ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const shell = readFileSync(
  new URL("../components/app/AppShell.tsx", import.meta.url),
  "utf8",
);

describe("AppShell architecture", () => {
  it("keeps feature orchestration outside the layout shell", () => {
    expect(shell).not.toContain("@tauri-apps/api/core");
    expect(shell).not.toContain("useImportStore");
    expect(shell).not.toContain("waitForTaskTerminal");
    expect(shell).not.toContain("start_wiki_compile");
    expect(shell).not.toContain("save_llm_provider");
    expect(shell).not.toContain("start_deep_lint");
    expect(shell).not.toContain("start_export");
  });
});
```

- [ ] **Step 2: 运行测试并确认它按预期失败**

Run: `npm run test -- src/test/app-shell-architecture.test.ts`

Expected: FAIL，至少命中 `@tauri-apps/api/core`、`useImportStore` 或 command string。

- [ ] **Step 3: 补齐当前 shell 行为回归断言**

在 `appShellActions.test.tsx` 保留并补齐：settings modal 不改变 active view、workspace focus Escape、right panel control、checkpoint copy、首屏 agent route 刷新。当前这些测试在后续 controller 抽离时不得删除，只能调整 mock/fixture 入口。

- [ ] **Step 4: 运行现有 AppShell 测试**

Run: `npm run test -- src/components/app/appShellActions.test.tsx`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/test/app-shell-architecture.test.ts src/components/app/appShellActions.test.tsx
git commit -m "test: define app shell orchestration boundary"
```

### Task 2: 增加 guarded Agent route 更新与 capability hook

**Files:**
- Create: `src/hooks/useAiCapabilities.ts`
- Create: `src/hooks/useAiCapabilities.test.tsx`
- Modify: `src/stores/projectStore.ts`
- Modify: `src/stores/projectStore.test.ts`

**Interfaces:**
- Consumes: `detect_agents`、`list_llm_providers`、`ProjectSummary`、project store。
- Produces: `AiCapabilitiesWorkflow` 与不会覆盖新项目的 `setAgentRoute` store action。

- [ ] **Step 1: 为 project store 写 stale-project guard 测试**

```ts
it("ignores an agent route update for a project that is no longer active", () => {
  useProjectStore.getState().setCurrentProject(projectB);
  useProjectStore.getState().setAgentRoute(projectA.projectId, projectA.rootPath, "agent");
  expect(useProjectStore.getState().currentProject).toEqual(projectB);
});
```

- [ ] **Step 2: 运行测试并确认 action 不存在而失败**

Run: `npm run test -- src/stores/projectStore.test.ts`

Expected: FAIL，`setAgentRoute` 未定义。

- [ ] **Step 3: 实现原子 route action**

```ts
setAgentRoute: (projectId, rootPath, agentRoute) =>
  set((state) => {
    if (
      state.currentProject.projectId !== projectId ||
      state.currentProject.rootPath !== rootPath
    ) {
      return state;
    }
    return {
      currentProject: { ...state.currentProject, agentRoute },
    };
  }),
```

同时把 action 签名加入 `ProjectState`。

- [ ] **Step 4: 写 capability hook 测试**

覆盖：首次项目加载并行 probe、Agent 优先 route、Ollama/secret-enabled BYOK fallback、两者均不可用时 unconfigured、A 请求晚于 B 返回时不覆盖 B。

- [ ] **Step 5: 实现 `useAiCapabilities`**

核心实现必须使用 ref guard：

```ts
const projectKey = `${project.projectId}\0${project.rootPath}`;
const latestProjectKey = useRef(projectKey);
latestProjectKey.current = projectKey;

const refresh = useCallback(async () => {
  if (!hasTauri() || !project.projectId) return;
  const requestKey = projectKey;
  setRefreshing(true);
  try {
    const [agents, providers] = await Promise.all([
      invoke<AgentInfo[]>("detect_agents", { request }),
      invoke<ProviderStatus[]>("list_llm_providers", { request }),
    ]);
    if (latestProjectKey.current !== requestKey) return;
    setAgents(agents);
    setProviders(providers);
    setAgentRoute(project.projectId, project.rootPath, resolveRoute(agents, providers));
  } finally {
    if (latestProjectKey.current === requestKey) setRefreshing(false);
  }
}, [projectKey, project.projectId, project.rootPath, setAgentRoute]);
```

- [ ] **Step 6: 运行 hook/store 测试**

Run: `npm run test -- src/hooks/useAiCapabilities.test.tsx src/stores/projectStore.test.ts`

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add src/hooks/useAiCapabilities.ts src/hooks/useAiCapabilities.test.tsx src/stores/projectStore.ts src/stores/projectStore.test.ts
git commit -m "refactor: add guarded ai capability workflow"
```

### Task 3: 提取统一 task launcher

**Files:**
- Create: `src/hooks/useTaskLauncher.ts`
- Create: `src/hooks/useTaskLauncher.test.tsx`
- Read: `src/stores/taskStore.ts`
- Read: `src/types/task.ts`

**Interfaces:**
- Consumes: project identity、task commands、`upsertTask`、`openDrawer`、`cancelTaskRequest`。
- Produces: `TaskLauncher` typed methods，供 Import/Agent/confirmation 共享。

- [ ] **Step 1: 写 start task 行为测试**

使用 `renderHook` 和 mocked `invoke`，断言 `startCompile()` 调用：

```ts
expect(invokeMock).toHaveBeenCalledWith("start_wiki_compile", {
  request: {
    projectId: "p1",
    projectRootPath: "/wiki/p1",
    route: "auto",
    agent: null,
    provider: null,
  },
});
expect(useTaskStore.getState().tasks).toContainEqual(task);
expect(useTaskStore.getState().selectedTaskId).toBe(task.id);
```

- [ ] **Step 2: 写 cancel error 测试**

断言 `cancel` 调用现有 `cancel_task` 入口，失败时发 `task.cancelError` toast，不吞掉错误上下文。

- [ ] **Step 3: 实现 launcher**

用私有 `track(task)` 统一 upsert/open：

```ts
const track = useCallback((task: BackendTask) => {
  upsertTask(task);
  openTaskDrawer(task.id);
  return task;
}, [openTaskDrawer, upsertTask]);
```

三个 start 方法只负责构造各自 typed request；不要暴露任意 command string 的通用 `launch(command)`。

- [ ] **Step 4: 运行 task launcher 测试**

Run: `npm run test -- src/hooks/useTaskLauncher.test.tsx src/stores/taskStore.test.ts`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useTaskLauncher.ts src/hooks/useTaskLauncher.test.tsx
git commit -m "refactor: centralize typed task launch workflows"
```

### Task 4: 提取 `useImportWorkflow`

**Files:**
- Create: `src/features/import/useImportWorkflow.ts`
- Create: `src/features/import/useImportWorkflow.test.tsx`
- Modify later: `src/components/app/WorkspaceController.tsx`
- Read: `src/stores/importStore.ts`
- Read: `src/features/wiki/wikiStore.ts`

**Interfaces:**
- Consumes: project、activeView、`TaskLauncher.startCompile`、import/wiki/project/toast stores。
- Produces: `ImportWorkflow`，可直接映射到 `ImportView` props。

- [ ] **Step 1: 写项目切换与 source list 测试**

覆盖：project key 变化调用 `importStore.reset()`；只有 activeView=`import` 且有 Tauri/project 时拉 `list_imported_sources`；A 列表晚返回不覆盖 B。

- [ ] **Step 2: 写 file preview task 测试**

覆盖：空路径清 preview；路径 trim/filter；`preview_import → waitForTaskTerminal → get_import_preview`；非 succeeded task 转为用户可见 error；成功结果受 project key guard。

- [ ] **Step 3: 写 clipboard/URL preview 测试**

URL case mock `fetch_import_url` 和 `../../lib/readability`，断言 Readability 只在 URL 调用时动态 import；clipboard 不加载它。

- [ ] **Step 4: 写 confirm 顺序测试**

```text
confirm_import_preview succeeds
→ setPreview(null)
→ wikiStore.scan(project)
→ optional taskLauncher.startCompile()
→ finally setIsConfirming(false)
```

失败时 preview 保留、toast 出错、`isConfirming` 恢复 false。

- [ ] **Step 5: 写 source action 测试**

delete/replace 分别调用 `request_delete_source` / `request_replace_source`，返回的 `PendingAction` 进入 project store；replacement path 只在 replace request 中存在。

- [ ] **Step 6: 实现 workflow**

将 AppShell 当前 305-529 行对应逻辑迁移；所有回调在 hook 内引用明确的 primitive dependencies，避免把整个 `project` 对象放入依赖数组造成无意义重建。

- [ ] **Step 7: 运行 Import workflow 与现有 view tests**

Run: `npm run test -- src/features/import/useImportWorkflow.test.tsx src/features/import/ImportView.test.tsx`

Expected: PASS。

- [ ] **Step 8: Commit**

```bash
git add src/features/import/useImportWorkflow.ts src/features/import/useImportWorkflow.test.tsx
git commit -m "refactor: extract import workflow hook"
```

### Task 5: 提取 `useProviderWorkflow`

**Files:**
- Create: `src/features/settings/useProviderWorkflow.ts`
- Create: `src/features/settings/useProviderWorkflow.test.tsx`
- Read: `src/features/settings/SettingsDialog.tsx`
- Read: `src/features/settings/LlmProviderSettings.tsx`

**Interfaces:**
- Consumes: project、`AiCapabilitiesWorkflow.providers/refresh`、provider commands。
- Produces: SettingsDialog 当前所需 provider props 与操作 callbacks。

- [ ] **Step 1: 写 config/secret mutation 测试**

断言 `saveProvider`、`saveSecret`、`deleteSecret` 各自成功后只调用一次 `capabilities.refresh()`；失败时不假装 refresh 成功，错误继续交给 Settings UI 的现有 error handling。

- [ ] **Step 2: 写 provider test fallback 测试**

无 Tauri 时返回：

```ts
{ ok: false, message: t("provider.testUnavailable") }
```

有 Tauri 时调用 `test_llm_provider` 并保留完整 typed result。

- [ ] **Step 3: 实现 hook**

```ts
export function useProviderWorkflow(
  project: ProjectSummary,
  capabilities: AiCapabilitiesWorkflow,
): ProviderWorkflow {
  // save config, store/delete secret, test, refresh
}
```

不得把 secret 存入 local state、Zustand 或 toast。

- [ ] **Step 4: 运行 provider tests**

Run: `npm run test -- src/features/settings/useProviderWorkflow.test.tsx src/features/settings/provider.test.tsx src/features/settings/AiSettings.test.tsx`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/features/settings/useProviderWorkflow.ts src/features/settings/useProviderWorkflow.test.tsx
git commit -m "refactor: extract provider workflow hook"
```

### Task 6: 提取 `useAgentWorkflow`

**Files:**
- Create: `src/features/agent/useAgentWorkflow.ts`
- Create: `src/features/agent/useAgentWorkflow.test.tsx`
- Read: `src/features/agent/RunAgentDialog.tsx`
- Read: `src/features/agent/AgentView.tsx`

**Interfaces:**
- Consumes: project、capabilities、TaskLauncher、navigation/settings/toast stores。
- Produces: AgentView + RunAgentDialog 所需 callbacks/state。

- [ ] **Step 1: 写 dialog/default Agent 测试**

覆盖 preset open/close、installed default agent derivation、`set_default_agent` 后 `settingsStore.loadSettings` 与 capability refresh；失败发 error toast。

- [ ] **Step 2: 写每个 skill route 测试**

精确覆盖：

| Skill | 预期行为 |
| --- | --- |
| `wiki-ingest` | `taskLauncher.startCompile`，info toast |
| `wiki-lint` | `startDeepLint`，导航到 lint |
| `wiki-query` | 导航到 chat，显示 query hint |
| 单页 HTML skills | 导航到 exports，不直接启动 task |
| `html-project-report` | `startExport(project_report, null)`，导航 exports |

- [ ] **Step 3: 实现 workflow 与导出映射**

保持显式 mapping，禁止依赖字符串 replace 推导 ExportType：

```ts
const exportSkillMap: Partial<Record<AgentSkill, ExportType>> = {
  "html-beautiful-read": "beautiful_read",
  "html-knowledge-card": "knowledge_card",
  "html-concept-map": "concept_map",
  "html-project-report": "project_report",
};
```

- [ ] **Step 4: 运行 Agent workflow/view tests**

Run: `npm run test -- src/features/agent/useAgentWorkflow.test.tsx src/features/agent/agent.test.tsx`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/features/agent/useAgentWorkflow.ts src/features/agent/useAgentWorkflow.test.tsx
git commit -m "refactor: extract agent workflow hook"
```

### Task 7: 提取全局 confirmation controller

**Files:**
- Create: `src/components/app/ProjectConfirmationController.tsx`
- Create: `src/components/app/ProjectConfirmationController.test.tsx`
- Modify later: `src/components/app/AppShell.tsx`

**Interfaces:**
- Consumes: project `pendingAction`、waiting compile task action、project confirmation store、TaskLauncher、`confirm_compile_action`。
- Produces: 两类现有 dialog 的完整展示/confirm/cancel 行为。

- [ ] **Step 1: 写 source action confirm 测试**

确认 delete/replace action 时：先 `confirmPendingAction()`；成功后 `startCompile({ route: "auto", agent: null, provider: null })`；失败显示 `import.sourceCompileError`。

- [ ] **Step 2: 写 compile action 测试**

没有 project pendingAction 但 task 处于 `waiting_for_confirmation` 时，confirm 调用：

```ts
invoke<BackendTask>("confirm_compile_action", {
  request: { actionId: action.id, confirmed: true },
});
```

返回 task 必须 upsert；cancel 传 `confirmed: false`。

- [ ] **Step 3: 实现 controller**

controller 自己读取 stores 并渲染 `ConfirmationDialog` / `CompileConflictDialog`；AppShell 只放 `<ProjectConfirmationController />`，不再知道 action type 或 command。

- [ ] **Step 4: 运行 confirmation tests**

Run: `npm run test -- src/components/app/ProjectConfirmationController.test.tsx src/components/app/ConfirmationDialog.test.tsx src/components/app/CompileConflictDialog.test.tsx`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/components/app/ProjectConfirmationController.tsx src/components/app/ProjectConfirmationController.test.tsx
git commit -m "refactor: extract project confirmation controller"
```

### Task 8: 创建 WorkspaceRouter 与 WorkspaceController

**Files:**
- Create: `src/components/app/WorkspaceRouter.tsx`
- Create: `src/components/app/WorkspaceRouter.test.tsx`
- Create: `src/components/app/WorkspaceController.tsx`
- Create: `src/components/app/WorkspaceController.test.tsx`
- Modify: `src/components/app/AppShell.tsx`

**Interfaces:**
- Consumes: Tasks 2-7 的 workflows、active view、现有 views/dialogs。
- Produces: feature-neutral AppShell 接口；lazy routing 与 controller wiring 独立可测。

- [ ] **Step 1: 把 lazy imports 移到 WorkspaceRouter**

保留 Dashboard 静态 import；Chat/Exports/Graph/Import/Lint/Wiki/Agent 保持 `lazy(() => import(...).then(...))`。不要改变 chunk boundary。

- [ ] **Step 2: 在 Router 中保留 error/suspense 组合**

```tsx
<ViewErrorBoundary key={activeView}>
  <Suspense fallback={<ViewFallback />}>
    {renderActiveView()}
  </Suspense>
</ViewErrorBoundary>
```

`renderActiveView` 只映射 view 和 props，不执行 IPC。

- [ ] **Step 3: 写 Router 测试**

mock lazy view modules，逐一断言 dashboard/wiki/chat/graph/lint/exports/import/agent 映射；增加一个 lazy reject case，断言 `role="alert"` 和 reload action 仍出现。

- [ ] **Step 4: 实现 WorkspaceController composition**

controller 读取 currentProject、activeView、settingsOpen，构造：

```ts
const capabilities = useAiCapabilities(
  currentProject,
  activeView === "agent" || settingsOpen,
);
const tasks = useTaskLauncher(currentProject);
const imports = useImportWorkflow(currentProject, activeView, tasks);
const providers = useProviderWorkflow(currentProject, capabilities);
const agent = useAgentWorkflow(currentProject, capabilities, tasks);
```

然后渲染 workspace header、`WorkspaceRouter`、`RunAgentDialog`、`SettingsDialog`。Settings 与 Agent 共享同一个 capabilities snapshot。

- [ ] **Step 5: 写 controller wiring 测试**

断言 ImportView callbacks 来自 import workflow、AgentView/RunAgentDialog 来自 agent workflow、SettingsDialog provider props 来自 provider workflow；切换 active view 不重建项目状态。

- [ ] **Step 6: 运行 Router/Controller 测试**

Run: `npm run test -- src/components/app/WorkspaceRouter.test.tsx src/components/app/WorkspaceController.test.tsx`

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add src/components/app/WorkspaceRouter.tsx src/components/app/WorkspaceRouter.test.tsx src/components/app/WorkspaceController.tsx src/components/app/WorkspaceController.test.tsx
git commit -m "refactor: add workspace router and controller"
```

### Task 9: 瘦身 AppShell 并通过架构契约

**Files:**
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/components/app/appShellActions.test.tsx`
- Test: `src/test/app-shell-architecture.test.ts`

**Interfaces:**
- Consumes: `WorkspaceController`、`ProjectConfirmationController` 与现有 shell components。
- Produces: 只包含 layout/pane/shortcut/global overlay wiring 的 AppShell。

- [ ] **Step 1: 删除 AppShell 的 feature imports/state/callbacks**

最终不得再 import：

```text
@tauri-apps/api/core
RunAgentDialog / AgentSkill / RunAgentOptions
useImportStore / useSettingsStore / useWikiStore
AgentInfo / AgentKind / provider/import/export task DTOs
waitForTaskTerminal / readability
```

- [ ] **Step 2: 保留真正属于 shell 的逻辑**

保留：pane widths、responsive right panel collapse、Escape close focus/panel、Ctrl/Cmd+, settings shortcut、TopBar/LeftSidebar/RightContextPanel/BottomStatusBar、Toaster、TaskLogDrawer。

- [ ] **Step 3: 用 controllers 替换业务块**

```tsx
<main className="app-shell__main">
  <WorkspaceController />
</main>
<ProjectConfirmationController />
<TaskLogDrawer />
<Toaster />
```

实际层级必须保持现有 DOM/CSS selectors 与 aria 关系；不能因为抽组件改变 `.app-shell__workbench`、`main` 或 right panel sibling 结构。

- [ ] **Step 4: 运行架构契约并确认转绿**

Run: `npm run test -- src/test/app-shell-architecture.test.ts`

Expected: PASS。

- [ ] **Step 5: 运行所有 shell 行为测试**

Run: `npm run test -- src/components/app/appShellActions.test.tsx src/components/app/WorkspaceController.test.tsx src/components/app/ProjectConfirmationController.test.tsx`

Expected: PASS；settings modal、agentRoute、confirmation、workspace focus 行为均不退化。

- [ ] **Step 6: 检查 bundle boundary**

Run: `npm run build`

Expected: build PASS；Graph/Sigma、Milkdown、Readability、markdown renderer 仍为 async chunks，Dashboard 入口不新增 Readability/Graph/Milkdown 静态依赖。

- [ ] **Step 7: Commit**

```bash
git add src/components/app/AppShell.tsx src/components/app/appShellActions.test.tsx src/test/app-shell-architecture.test.ts
git commit -m "refactor: reduce app shell to layout wiring"
```

### Task 10: 完整验证、双重审查与进度记录

**Files:**
- Modify: `SPEC/progress.txt`
- Modify only if a subtle issue was discovered: `SPEC/gotchas.txt`
- Review: all files listed in Tasks 1-9

**Interfaces:**
- Consumes: 完成的 controllers/hooks。
- Produces: 无 UI 行为变化、无 stale project 写入、无 bundle 回退、完整检查通过的最终实现。

- [ ] **Step 1: 运行 feature 定向测试集合**

Run:

```powershell
npm run test -- `
  src/hooks/useAiCapabilities.test.tsx `
  src/hooks/useTaskLauncher.test.tsx `
  src/features/import/useImportWorkflow.test.tsx `
  src/features/settings/useProviderWorkflow.test.tsx `
  src/features/agent/useAgentWorkflow.test.tsx `
  src/components/app/WorkspaceRouter.test.tsx `
  src/components/app/WorkspaceController.test.tsx `
  src/components/app/ProjectConfirmationController.test.tsx `
  src/components/app/appShellActions.test.tsx `
  src/test/app-shell-architecture.test.ts
```

Expected: PASS。

- [ ] **Step 2: 运行 lint/build/console 检查**

Run: `npm run lint && npm run build && npm run check:console`

Expected: PASS；无未处理 promise、hook dependency warning、`console.log`。

- [ ] **Step 3: 运行仓库统一检查**

Run: `npm run check`

Expected: frontend tests/lint/build、console scan、Tauri GUI compile、Rust no-default-features tests 全部 PASS。

- [ ] **Step 4: 执行共享上下文审查**

重点检查：与 audit 目标一致、AppShell 是否真正 feature-neutral、Import/Agent/Provider 时序是否保持、lazy ErrorBoundary 是否保留、项目切换 guard 是否覆盖每个 await 边界。

- [ ] **Step 5: 执行 fresh-context 审查**

重点检查：hook 是否变成新巨石、store 是否隐藏业务 orchestration、错误/Loading 状态是否丢失、secret 是否泄漏、task 是否绕过统一 drawer、测试是否只测 mock 而未覆盖 controller wiring。

- [ ] **Step 6: 修复有效问题并完整重跑**

任何审查修复后重新运行 `npm run check`，不能只跑受影响单测后交付。

- [ ] **Step 7: 记录进度**

在 `SPEC/progress.txt` 顶部插入：

```text
[2026-07-10] AppShell workflow controller refactor — Moved Import, Provider, Agent, task launch, capability refresh, confirmation, and view routing orchestration into focused hooks/controllers while preserving the Codex-like shell DOM and lazy error boundaries — Key decision: AppShell owns layout only; workflows share guarded capability/task primitives and never write stale project results.
```

只有遇到新的微妙/重复错误才追加 `SPEC/gotchas.txt`。如果项目切换 guard 暴露新的“子调用先 set 状态”问题，应按现有 Chat epoch gotcha 的原则记录：持有 guard 的层必须原子提交。

- [ ] **Step 8: Commit**

```bash
git add src/components/app src/features/import src/features/settings src/features/agent src/hooks src/stores/projectStore.ts src/stores/projectStore.test.ts src/test/app-shell-architecture.test.ts SPEC/progress.txt SPEC/gotchas.txt
git commit -m "refactor: move feature workflows out of app shell"
```

## 5. 验收标准

- `AppShell.tsx` 不再 import Tauri `invoke`、Import/Agent/Provider DTO/store 或具体 command string。
- AppShell 只保留布局、pane、shell 快捷键与全局 controller 接线；feature view dispatch 位于 `WorkspaceRouter`。
- `useImportWorkflow` 独立覆盖 file/text/url preview、source actions、confirm → scan → optional compile、project switch stale guard。
- `useProviderWorkflow` 独立覆盖 provider config、secret save/delete、test 与 capability refresh，且不持久化 secret。
- `useAgentWorkflow` 独立覆盖 default Agent、dialog、skill routing、task launch 与 navigation。
- Agent/Provider 共用一份 `useAiCapabilities` snapshot；task 启动共用 `useTaskLauncher`，没有重复 invoke/upsert/open drawer 模板。
- source delete/replace 与 compile PendingAction 由 `ProjectConfirmationController` 处理，checkpoint/确认语义不变。
- Dashboard 首屏 agentRoute 仍会刷新；A 项目慢请求不能覆盖 B 项目。
- URL Readability、Graph/Sigma、Milkdown、markdown renderer 仍保持预期 lazy chunk；每个 lazy route 保留 ErrorBoundary。
- 现有 shell DOM hierarchy、CSS selectors、aria controls、中文/英文文案与视觉密度不变。
- `npm run check` 最终通过；两轮审查的有效问题已修复并重新全量验证。

## 6. 风险与回滚

| 风险 | 防护 |
| --- | --- |
| Hook 依赖数组导致重复 probe/loop | 依赖 primitive project id/root；hook tests 断言 call count |
| A 项目异步结果写入 B | project key ref + guarded store action；专门 race test |
| controller 抽离改变 DOM/CSS | AppShell DOM regression test + build + UI contract tests |
| lazy import 抽离导致生产白屏 | Router 同址 Suspense + ViewErrorBoundary reject test |
| capability refresh 重复 | Agent/Settings 共用 hook；visible refresh 有 call-count test |
| Import confirm 顺序变化 | 明确的 scan-before-optional-compile 测试 |
| secret 泄漏到 state/log | Provider hook 不返回 secret；测试只断言 command 参数，不 snapshot secret |
| task 状态分叉 | 所有长任务只经 TaskLauncher + taskStore |

- 每个 task 单独 commit，优先逐 task 反向 commit；不得使用 `git reset --hard` 或覆盖用户现有未提交改动。
- 若 WorkspaceRouter 破坏 chunk boundary，先恢复原 lazy import 位置，再单独重新设计 Router，不为“文件更干净”牺牲生产稳定性。
- 若 controller wiring 需要改变 feature view props，先添加兼容 adapter；不要在同一 commit 重写 ImportView/AgentView/SettingsDialog。
