# LLM Wiki Desktop 可访问性、国际化与产品质量审查

日期：2026-08-14

来源：从《第一性原理对抗性审查》拆分

范围：错误体验、中英文一致性、DOM language、键盘/读屏语义、隐私提示、空状态与恢复承诺、产品文档一致性

## 1. 结论

项目已经具备紧凑桌面 shell、统一视觉 token、部分键盘/role 测试、skip link、局部 error boundary、中英文 locale 和明确任务状态。这些说明产品质量不是从零开始。

当前问题主要不是“界面不够漂亮”，而是**界面所表达的承诺与实际行为不完全一致**：

- “重试”按钮不能真的重试 lazy chunk；
- “检查更新/自动更新”界面没有后端能力；
- 首次打开失败可能显示 `[object Object]`；
- 英文界面的 DOM 仍声明中文；
- 少数中文 key 缺失，风险枚举直接显示英文；
- 搜索 listbox、toggle 和 folder disclosure 的读屏语义不完整；
- 不可信 Markdown 远程图片会自动联网，但用户没有可理解的授权提示。

成熟产品的标准不是“按钮存在”，而是按钮、状态、错误和辅助技术都准确表达系统当前能做到什么。

## 2. 用户可见的确定问题

### UX-P1-01 lazy 错误面板给出无效的“重试”承诺

对应总报告：P1-12；可靠性主报告详述。

- `src/components/app/WorkspaceRouter.tsx:12-46`
- `src/components/app/ViewErrorBoundary.tsx:32-44`

**用户影响**：看到可操作按钮却永远回到同一错误，比明确要求重启更令人困惑，也会降低对其他恢复按钮的信任。

**改善方向**：按钮必须执行真实 reload/re-import；若只能重启，文案就应明确说明会重新载入应用，并保护未保存编辑。

### UX-P1-02 更新设置展示了当前不可用的产品能力

**Batch 6 状态（2026-08-21）：源码产品承诺 Closed；真实 signed upgrade Not Closed。** Commits `8db5b4ca`、`9f8bc2ac` 已把设置与全局 controller/store 连接到真实签名 offer、下载进度、取消/重试/忽略、changelog、安装保护和重启 receipt；Batch 6 updater/frontend tests 为绿。由于 `latest.json` 仍为 404 且没有签名 draft，UI 的真实 package upgrade 仍是 release-blocking Pending，见 [`../release/batch-6-acceptance-evidence.md`](../release/batch-6-acceptance-evidence.md)。

对应总报告：P1-10。

`src/features/settings/UpdateSettings.tsx:25-55,88-95` 的检查只读取当前版本，下载状态只改文案。

**用户影响**：用户可能以为应用会自动获得安全更新，实际不会；这不只是“功能未做”，而是错误的安全预期。

**改善方向**：实现前隐藏，或明确标注“当前版本不支持应用内更新”；实现后展示检查时间、当前/最新版本、签名状态、下载/安装同意和失败恢复。

### UX-P2-01 首次创建/打开失败可能显示 `[object Object]`

**Batch 6 状态（2026-08-21）：Closed。** Commit `214605d5` 统一 object/serialized/circular failure normalization、本地化 summary/recovery action 与脱敏 technical detail；Batch 6 前端聚焦组 242/242 和 final-four redline 均通过。

对应总报告：P2-05。

- `src-tauri/src/errors/backend_error.rs:10-18`
- `src/stores/projectStore.ts:326-334,428-432,452-458`
- `src/features/project/NoProjectWorkspace.tsx:11-13,61-109,189,241`

**用户影响**：无法知道是权限、路径、项目格式、Git 还是临时 I/O 问题，也不知道下一步该做什么。

**改善方向**：统一错误适配层，显示本地化摘要、建议动作和可展开技术详情；technical message 用于复制/支持，不直接代替用户文案。

## 3. 国际化问题

本节对应总报告：P2-06。

### I18N-P2-01 DOM language 永久为中文

`index.html:2` 固定 `<html lang="zh-CN">`；i18next 初始化和语言切换没有同步 `document.documentElement.lang`。

**影响**：英文 UI 中读屏发音、断词、拼写和浏览器辅助行为可能仍按中文处理。

**修复**：初始化完成和每次 language change 都更新 DOM lang；只有支持的规范值可写入。

**验收**：zh-CN/en 切换后 DOM、读屏语音和持久化设置一致；应用重启保持。

### I18N-P2-02 中英文 key 不完全对等

en 有 2,503 keys、zh 有 2,501。中文缺：

- `confirmation.confirm.repair_project`
- `projectRecovery.banner.repair`

现有 parity test 只扫描 backend 相关三个 namespace：`src/test/i18n-backend-keys.test.ts:41-68`。

**影响**：中文恢复路径混入英文 fallback，最关键的 repair/recovery 状态显得不完整。

**修复**：全 locale exact-key parity；允许的语言特定 key 必须显式 allowlist，默认不接受 silent fallback。

### I18N-P2-03 风险枚举直接显示英文技术值

`src/components/app/ConfirmationDialog.tsx:91-93` 把 `high/destructive/...` 直接插入 UI；测试还固定了 `Risk: destructive`。

**影响**：中文高风险确认中最重要的风险等级反而是未翻译的内部 enum。

**修复**：enum → locale key 映射；颜色/icon 只是辅助，不代替文字。

**验收**：中文 repair/restricted/destructive 截图与 DOM 无非技术性的英文 fallback。

## 4. 可访问性问题

### A11Y-P2-01 顶栏搜索的 ARIA pattern 不完整

结果容器声明 `role="listbox"`，子项仍是普通 button；输入只处理 Enter/Escape，没有上下键、active descendant 或一致的 option 模型：`src/components/app/TopBar.tsx:447-464`。

**影响**：读屏会收到互相矛盾的控件语义，键盘用户无法按常见组合框方式浏览结果。

**修复选项**：

1. 完整实现 combobox/listbox/option + `aria-activedescendant` + 上下键；或
2. 移除错误 listbox role，使用语义清楚的 button list。

### A11Y-P2-02 Graph/Wiki 高频控件缺少可访问名称或状态

- Graph/Wiki 搜索只有 placeholder，没有稳定 accessible name；
- filter pill、Graph color mode 缺 `aria-pressed`；
- folder disclosure 缺 `aria-expanded`/`aria-controls` 或完整 treeitem pattern。

代表位置：

- `src/features/graph/GraphControls.tsx:45-64`
- `src/features/wiki/WikiTree.tsx:99-107,181-192,343-377`

**影响**：视觉上能看出的“当前选中/展开”，读屏无法获得；语音控制也难以准确定位控件。

**修复**：输入用 `<label>` 或 `aria-label`；toggle 使用 `aria-pressed`；folder 采用一致 disclosure/tree pattern，并保证焦点和键盘移动规则。

### A11Y-P2-03 缺少真实桌面辅助技术门禁

仓库已有部分 role/keyboard tests 和 skip link，但没有 axe、真实 WebView 键盘、Narrator/VoiceOver/Orca、高对比、200/400% zoom、reduced-motion release gate。

**影响**：jsdom 中正确的 role 不代表 WebView 实际焦点、系统读屏和缩放可完成核心旅程。

**修复**：axe 用于快速发现问题，人工/半自动三平台核心旅程用于验收；不要以单一 axe 分数替代真实操作。

## 5. 隐私与可理解授权

### UX-P2-02 远程 Markdown 图片在无提示情况下联网

对应总报告：P2-07；安全主报告详述。

Wiki/Chat 可以加载文档中的远程图片，用户看不到 host 或是否会产生网络请求。

**用户影响**：只是阅读本地知识库也可能向第三方服务器发送请求，违背 local-first 的直觉。

**改善方向**：默认占位并显示域名；允许“本次加载”“始终允许该 host”；项目级设置清楚说明；导出/预览采用同一规则或明确差异。

### UX-P2-03 Provider/Agent 外发授权需要精确到目的地和能力

当前产品已有 trusted project 概念，但对普通用户而言，“信任项目”不应等价于“所有 provider、Agent、Skill 永久拥有相同网络和文件能力”。

**改善方向**：授权文案显示：

- 哪个 provider/Agent；
- 精确 host；
- 会发送哪些范围的内容；
- 是否允许读项目外文件、运行命令、写项目；
- 授权持续多久、如何撤销；
- 撤销完成后系统保证什么。

## 6. 信息架构与文档一致性

### QUALITY-P2-01 SPEC 对首次使用当前状态的描述已经漂移

**Batch 6 状态（2026-08-21）：Closed。** `SPEC/SPEC.md` 16.7 已同步当前完整 shell、`NoProjectWorkspace`、typed assessment、普通资料目录新建并导入与 native/compatible/restricted/read-only/recovery 路径；架构合同继续禁止无项目路径绕过 shell。

对应总报告：P2-16。

`SPEC/SPEC.md:725-729` 仍描述独立 `ProjectStartView` 和旧 assessment 现状；实际 App 已使用完整 shell + `NoProjectWorkspace`。

**影响**：开发者和 Agent 可能依据错误“现状”改回旧首屏；QA 也会使用错误验收基线。

**改善方向**：权威文档区分“当前实现”“目标合同”“历史证据”；legacy component 明确标记 unreachable；架构测试固定无项目工作台入口。

### QUALITY-P2-02 状态文案需要区分失败、不可用、未安装和未实现

能力包、updater、provider、Agent、Graph fallback 等状态不应共用模糊的 unavailable/error：

- **未实现**：当前版本没有这项能力；
- **未安装**：可以安装，并给出来源/大小/权限；
- **暂时不可达**：可以重试；
- **无权限**：说明为什么以及如何授权；
- **不安全/被阻止**：说明被哪个安全策略拒绝；
- **失败但可恢复**：提供真实 checkpoint/retry；
- **失败且必须重启**：明确保护未保存内容。

统一状态词汇可以显著减少“按钮很多但不知道哪个有用”的认知负担。

## 7. 产品质量验收旅程

每个旅程都应同时覆盖中文和英文、键盘和鼠标，并在至少一个真实读屏中完成：

1. 无项目 → 新建/打开失败 → 理解原因 → 恢复；
2. restricted/read-only/untrusted → 查看可做与不可做的操作；
3. 全局搜索 → 键盘浏览 → 切项目时结果正确失效；
4. Wiki tree → 搜索、展开、选择、重命名、读屏状态；
5. Chat → 授权外发、流式状态、取消、失败、重试；
6. capability → 未安装、下载安装、失败、恢复；
7. Graph → WebGL fallback、过滤状态、键盘可达；
8. destructive confirmation → 风险、路径、checkpoint 和取消；
9. language switch → DOM lang、全部文案、重启持久化；
10. 200%/400% zoom、高对比、reduced motion。

## 8. 推荐顺序

1. 修复 `[object Object]` 与 lazy retry，先保证错误可理解且恢复真实；
2. 隐藏/改写未实现 updater，能力状态区分未安装/不可用；
3. 同步 DOM lang、全 locale parity、风险枚举翻译；
4. 顶栏搜索采用完整 combobox 或简单 button list；
5. Graph/Wiki 控件补 accessible name/state；
6. 远程图片和外部 AI 使用可理解的精确授权；
7. 更新漂移文档；
8. 建立 axe + 三平台读屏/缩放 release checklist。
