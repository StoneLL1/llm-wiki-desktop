# llm-wiki 桌面端改进点展开说明

来源：根目录 `REVIEW-REPORT.md`  
用途：把审查报告里的建议翻译成更容易执行的改进清单。每一项都包含：为什么要改、改进方向、大白话理解、专业视角说明。

## 总览

当前项目不是“没有基础”，恰恰相反：本地文件模型、Rust 后端边界、Git checkpoint、Keyring、任务取消/恢复、导入/编译/图谱/Chat/导出这些主链路已经立起来了。现在的问题更像是：房子已经能住，但门锁、验收流程、走线、动线和未来扩建空间还需要补齐。

优先级建议：

| 优先级 | 核心目标 | 代表事项 |
| --- | --- | --- |
| P0 | 先让桌面端“可信、可发布、可回归” | CSP、CI/CD、更新/发布信任链 |
| P1 | 让核心体验更稳、更好用、更能继续扩展 | 服务拆分、AppShell 瘦身、首次引导、当前页工作台、统一风险确认、Compile 可视化 |
| P2 | 做长期产品护城河 | 插件/skill 管理、多格式导出、可选加密、更多性能基准和生态能力 |

## P0：必须优先补的底座

### 1. 启用严格 CSP 与预览隔离

**对应报告问题**  
`src-tauri/tauri.conf.json:24-25` 里 `"csp": null`。项目会处理 Markdown、HTML、LLM 输出、导出预览，这条链路如果没有 CSP 和隔离，会让内容注入风险变大。

**为什么要改进**

大白话：现在像是房子门窗都装好了，但浏览器这扇窗没有上锁。只要用户导入的网页、Markdown 或 AI 生成内容里混入坏脚本，就可能在桌面 WebView 里搞事情。哪怕概率不高，也属于“一旦中招就很难解释”的安全问题。

专业视角：Tauri capability 只限制原生 API 权限，不等于渲染层安全。Markdown/HTML 渲染、导出 HTML 预览和 LLM 生成内容属于不完全可信输入。缺少 CSP、HTML sanitization、iframe/webview sandbox 时，存在 XSS、资源外联、内容注入和导出物二次执行风险。

**改进方向**

大白话：先给 WebView 加“安全规则”：哪些脚本能跑、哪些图片能加载、能连哪些网络地址，都明确写死。导入的网页和导出的 HTML 预览尽量关在一个单独的小房间里，不让它碰主应用。

专业方向：

- 在 `tauri.conf.json` 中配置严格 CSP，默认 `default-src 'self'`。
- 按需允许 `asset:`、`tauri:`、`blob:`，避免宽泛 `*`。
- 网络连接只允许用户配置过的 LLM endpoint 或本地必要服务。
- Markdown/HTML 预览统一走 sanitization。
- HTML export preview 使用 sandboxed iframe/webview 或只打开静态文件，不让导出物拥有主应用上下文。
- 给 CSP 和预览隔离加回归测试，防止后续又变回 `null`。

### 2. 建立 CI/CD 自动验证

**对应报告问题**  
仓库当前没有 `.github` 工作流；`package.json:6-12` 也没有统一 `check` 脚本。

**为什么要改进**

大白话：现在每次改代码，主要靠人记得跑哪些命令。这个项目牵涉前端、Rust、Tauri、Windows 路径、Git、Agent、文件安全，只靠人脑记，迟早漏。CI 就像每次出门前自动检查门窗、电器和钥匙。

专业视角：这是跨平台桌面应用，风险面覆盖 TypeScript 类型、Vite build、Vitest、ESLint、Rust service tests、路径安全、CJK/Windows 行为。缺 CI 会让回归在本地沉默，尤其是多个 agent 或多轮修改后，无法保证主干始终可构建、可测试。

**改进方向**

大白话：让 GitHub 每次 push/PR 自动跑一遍项目体检。体检不过，就不要合进去。

专业方向：

- 新增 `.github/workflows/ci.yml`。
- 至少覆盖 Windows、macOS、Linux 三个平台。
- 固定运行：
  - `npm ci`
  - `npm run test`
  - `npm run lint`
  - `npm run build`
  - `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`
- Windows 上避免默认 `cargo test` 的 GUI-linked loader 坑，固化 `--no-default-features`。
- 后续再补 Tauri bundle smoke test 和 release workflow。

### 3. 补齐发布、更新、签名和版本信任链

**对应报告问题**  
`src-tauri/tauri.conf.json:28-31` 里 bundle active 但 `icon: []`；`src/stores/settingsStore.ts:45` 有 `autoDownloadUpdates`，但未见 updater 插件/命令链。

**为什么要改进**

大白话：用户下载一个桌面软件时，最怕三件事：是不是正版？坏了能不能更新？更新会不会把数据弄丢？现在项目功能已经像桌面应用了，但发布可信度还像开发阶段。

专业视角：桌面端分发需要明确 artifact、签名、校验、版本策略、更新策略、回滚说明。存在 UI 状态字段但无 updater 实现，会形成产品承诺和实际能力不一致。缺 icon/signing/checksum/release notes 会影响用户信任和平台分发。

**改进方向**

大白话：把“能打包”升级成“能安全发给用户安装和升级”。

专业方向：

- 配置平台 icon、bundle metadata、版本号策略。
- 引入 Tauri updater 或明确暂不支持自动更新，并隐藏/禁用 `autoDownloadUpdates`。
- 发布包生成 checksum。
- 配置签名流程，至少为正式 release 准备 signing 文档。
- Settings 显示当前版本、更新策略、release notes 链接。
- 写清楚更新失败后的回滚和数据不变承诺。

## P1：重要，决定项目能不能继续长大

### 4. 拆分超大 Rust services

**对应报告问题**  
`lint_service.rs` 2833 行、`search_service.rs` 2313 行、`import_service.rs` 2275 行、`chat_service.rs` 2197 行、`compile_service.rs` 1838 行、`extraction_service.rs` 1983 行。

**为什么要改进**

大白话：现在几个后端文件像“万能杂物间”，什么都能找到，但东西越来越多。短期能用，长期会让每次改功能都害怕碰坏别的地方。

专业视角：单文件多职责会增加认知负担、测试定位成本和变更冲突。Import、Search、Lint、Chat、Compile 都已经有明确子领域：校验、规划、执行、回滚、持久化、prompt、报告。它们适合拆成模块化 use-case 和纯函数组件。

**改进方向**

大白话：不是推倒重来，而是把大房间慢慢隔成几个功能间。先从最常改、最危险的区域拆。

专业方向：

- `import_service` 拆为 preview、confirm、source_actions、promotion、dedupe。
- `lint_service` 拆为 rules、fixes、history、agent_normalization、reporting。
- `chat_service` 拆为 sessions、retrieval、citations、convenience_edit、prompt。
- `search_service` 拆为 wiki_index、query、excerpt、page_mutations。
- `compile_service` 拆为 plan、manifest、semantic_validation、workspace、apply。
- 每次拆分只移动代码和测试，不顺手改业务行为。

### 5. 瘦身 AppShell，把业务编排下沉

**对应报告问题**  
`src/components/app/AppShell.tsx` 746 行，并在 `AppShell.tsx:480-733` 直接编排导入确认、编译启动、provider/secret、Agent 默认值、任务取消、视图分发等逻辑。

**为什么要改进**

大白话：AppShell 应该是“桌面外壳”，现在它还兼职了导入经理、编译经理、设置经理、Agent 经理。它越忙，整个应用越容易被一个改动牵连。

专业视角：Shell 层应该负责 layout、navigation、pane state 和 feature mounting。跨服务业务流程应放入 feature controller/hook，便于测试、复用和错误边界隔离。当前 AppShell 的多领域 orchestration 会导致依赖数组复杂、状态刷新重复、toast/error 逻辑分散。

**改进方向**

大白话：让 AppShell 回到“搭架子”，具体业务交给各自的小负责人。

专业方向：

- 提取 `useImportWorkflow`：preview、confirm、compile-after-import、source delete/replace。
- 提取 `useProviderWorkflow`：save provider、store/delete secret、test provider、refresh capabilities。
- 提取 `useAgentWorkflow`：detect、set default、run dialog。
- 提取 `useTaskActions`：cancel、open drawer、task filters。
- 视图分发可改成 route/view registry，减少大段条件分支。

### 6. 把首次使用流程做成可恢复 checklist

**对应报告问题**  
用户从导入到编译再到图谱/Chat 的成功标准不够明确；现有流程散在 AppShell 和各 feature view。

**为什么要改进**

大白话：老用户知道怎么点，新用户不知道。现在用户看到的是一堆专业工具：导入、编译、图谱、Chat、Lint、导出。但他真正想要的是：“我丢进去几份资料，什么时候能看到一个像样的 wiki？”

专业视角：Time-to-first-value 是此类工具的核心指标。当前功能覆盖不等于完成用户路径。缺少 onboarding checklist 会导致用户在导入预览、编译、Agent/BYOK 配置、图谱构建之间迷路。可恢复 checklist 可以降低学习成本，也能把任务状态、错误恢复和下一步建议串起来。

**改进方向**

大白话：做一个“第一次生成 Wiki”的向导：选项目、导入资料、确认、编译、打开第一篇页面、看图谱、问一个问题。

专业方向：

- 新增 First Wiki Run state，存 `.app/`，可恢复、可跳过。
- 步骤包括：
  1. 打开或创建项目
  2. 导入 1-3 个 source
  3. 预览并确认
  4. 配置 BYOK 或选择 Agent
  5. 编译生成 wiki
  6. 打开第一篇页面
  7. 查看 graph
  8. Ask AI 验证内容
- 每一步展示当前状态、失败原因和下一步按钮。
- 不做 landing page，不做大 hero，保持 Codex-like 紧凑工作台风格。

### 7. 建立“当前页知识工作台”

**对应报告问题**  
相比 Obsidian/Logseq/SiYuan，当前缺少围绕当前页的反链、出链、未链接提及、source provenance、邻居节点工作流。

**为什么要改进**

大白话：知识库不是只有“全局搜索”和“大图谱”。用户读一篇笔记时，最想知道的是：这篇从哪来？链接到谁？谁又提到了它？还能问什么？现在这些信息散了。

专业视角：当前页上下文是 PKM 工具的核心交互面。全局图谱适合探索，但当前页反链/出链/source/citation/task/history 更适合高频阅读和编辑。右侧 context panel 已有容器，是承接这些信息的自然位置。

**改进方向**

大白话：把右侧栏变成“这篇页面的仪表盘”。

专业方向：

- 右栏按 tab 或 sections 展示：
  - Page info：路径、类型、tags、sources。
  - Links：outgoing links、backlinks、unlinked mentions。
  - Sources：引用的 `wiki/sources` 和原始 `raw/sources`。
  - Graph：一跳邻居、社区、degree。
  - AI：当前页 Ask、保存答案、相关 citations。
  - Tasks：与当前页相关的 compile/lint/export 任务。
- 后端优先复用 SearchService/WikiIndex/GraphService，不新增数据库。
- 先实现 page-level read-only 信息，再做编辑动作。

### 8. 统一高风险确认 Dialog，替换 `window.confirm`

**对应报告问题**  
`src/features/chat/ChatView.tsx:152`、`:169`、`:484` 使用 `window.confirm`；高风险操作没有展示影响范围、checkpoint、可撤销性。

**为什么要改进**

大白话：原生确认框太粗糙，只会问“确定吗？”。但用户真正需要知道的是：会改哪些文件？有没有备份？能不能撤销？Agent 会不会乱动？

专业视角：原生 confirm 不符合设计系统，也不利于可访问性、国际化、焦点管理、风险说明和结构化操作审计。高风险操作需要统一 modal，展示 affected paths、Git checkpoint、operation type、rollback boundary、不可逆部分。

**改进方向**

大白话：把“确定吗”升级成“这是将要发生的事，你确认接受这些影响吗”。

专业方向：

- 建立 `RiskConfirmDialog` 或 `PendingActionDialog`。
- 支持 delete、replace source、Chat convenience write、rollback、batch fix、conflict apply。
- 展示：
  - 操作类型
  - 影响路径
  - 是否已有 checkpoint
  - checkpoint hash
  - 是否可撤销
  - 风险等级
- 所有高风险动作走同一套组件和后端 confirmation DTO。

### 9. 把 Compile 做成可观察流水线

**对应报告问题**  
Compile 已有 plan/manifest 校验，但用户看不到像 GPT Researcher/AutoWiki 那样的 planner/reviewer/exporter 过程。

**为什么要改进**

大白话：现在用户点“编译”，等结果。出了错也许能看到日志，但很难知道 AI 为什么这么拆页面、为什么引用这些来源、哪些页面不确定。知识库编译最值钱的地方，恰好应该被看见。

专业视角：LLM content compilation 需要可追溯性：source coverage、plan rationale、draft page mapping、citation grounding、review findings、apply manifest。否则用户无法信任生成结果，也难以修复模型漏读/误读。AutoWiki 和 GPT Researcher 的优势就在于把研究/编译流程显性化。

**改进方向**

大白话：把“黑盒生成”变成“五步流水线”，每一步都能看、能停、能修。

专业方向：

- Compile UI 分为：
  1. Plan：计划生成，展示要创建/合并/更新的页面。
  2. Source Map：每个 source 覆盖了哪些主题。
  3. Draft Pages：草稿页面和引用关系。
  4. Review Findings：缺引用、重复页、冲突、低置信内容。
  5. Apply Manifest：最终写入和 Git checkpoint。
- 后端保存每步 artifact 到 `.app/compile-runs/{id}/`。
- 支持用户在 apply 前查看 diff 和拒绝部分变更。
- BYOK 和 Agent 路径共用同一套 plan/manifest/review DTO。

### 10. 提升 lint/TypeScript/Rust 测试组织

**对应报告问题**  
`eslint.config.js:5-23` 规则偏基础；超大 service 的测试和逻辑耦合度较高。

**为什么要改进**

大白话：现在测试不少，但“防呆规则”还不够。很多异步 bug、忘记 await、promise 漏处理、类型导入混乱，靠人看很容易漏。

专业视角：React/Tauri 应用中 no-floating-promises、no-misused-promises、consistent-type-imports、exhaustive deps 等规则能提前捕获异步与 hooks 风险。Rust service 也应按领域拆测试，避免所有测试堆在一个巨型文件内。

**改进方向**

大白话：让工具帮我们抓低级错误，让测试结构更像目录而不是长卷轴。

专业方向：

- 渐进启用 type-aware ESLint rules。
- 对已有误报先用局部注释或封装 helper 处理，不一次性大改。
- Rust 大 service 拆分后，同步拆测试模块。
- 建立 `npm run check` 聚合：
  - test
  - lint
  - build
  - console.log scan
  - rust no-default-features tests

### 11. 建立性能基准和 bundle budget

**对应报告问题**  
项目已做 lazy split 和 WikiIndex，但缺少自动防回归机制。

**为什么要改进**

大白话：你已经减过肥，但没有体重秤。以后谁加了一个大依赖，把首屏包又撑大，可能很晚才发现。

专业视角：性能优化需要预算和回归检测。当前 build 输出能看到 chunk size，但没有阈值约束。Wiki/Graph/Search/Chat retrieval 的性能也需要数据集基准，否则无法判断 500 页、2000 页、10000 页时是否退化。

**改进方向**

大白话：给项目装一个性能仪表盘。

专业方向：

- 加 bundle budget check，至少保护首屏 `index` chunk 和重依赖 async chunk。
- 建立样本 wiki：
  - 500 页
  - 2000 页
  - 10000 页
  - CJK 文件名
  - 大量 wikilinks
- 测量：
  - 启动到可交互
  - scan wiki
  - search
  - graph build
  - chat retrieval
  - export
- 将结果写入 `docs/audits` 或 `docs/perf`，CI 可先做 smoke，再做 nightly benchmark。

### 12. 建立 Threat Model 文档

**对应报告问题**  
项目已有很多安全机制，但没有一份统一威胁模型说明 Agent、BYOK、HTML、导出、日志、Git rollback 的边界。

**为什么要改进**

大白话：你已经做了很多安全措施，但没有一张“安全地图”。以后新增功能时，很容易不知道哪条线不能越。

专业视角：本项目处理本地文件、外部源、LLM 输出、Agent CLI、API keys、HTML export。安全边界横跨 renderer、Tauri IPC、Rust services、OS keyring、Git 和外部命令。Threat Model 可以把可信/不可信输入、资产、攻击面、缓解措施和残余风险写清楚。

**改进方向**

大白话：写一份“哪些东西危险、怎么防、还剩哪些风险”的说明书。

专业方向：

- 覆盖资产：
  - user wiki content
  - raw sources
  - `.app` state
  - API keys
  - exported HTML
  - Git history
- 覆盖攻击面：
  - imported HTML/Markdown
  - LLM output
  - Agent CLI execution
  - file path traversal
  - secrets in logs
  - updater/release supply chain
- 每类风险写现有防线、缺口和下一步。

## P2：可以排后，但会影响长期竞争力

### 13. 最小本地 skill/plugin 管理

**为什么要改进**

大白话：现在项目已经有 `skills/` 的影子，但用户看不到、管不了。以后功能越来越多，如果全塞进核心应用，会越来越重。

专业视角：Obsidian/SiYuan 的长期生态优势来自扩展机制。llm-wiki 不需要一开始做完整市场，但应提供本地 skill/plugin 的注册、权限说明、启用状态和版本来源，以便将 export、lint rules、wiki-query、compile recipes 逐步外置。

**改进方向**

- 列出本地 skills。
- 展示名称、版本、来源、权限、启用状态。
- 支持启用/禁用，不允许 silent install。
- 插件不能直接越过 Rust service 操作文件/密钥。

### 14. 多格式导出和静态站点发布

**为什么要改进**

大白话：HTML 导出已经有了，但用户常见需求还包括 PDF、整包 Markdown、静态站点、分享版资料包。知识库最终要能拿出去用。

专业视角：导出是知识库工具的价值闭环。Trilium/SiYuan 等工具提供多种导出/发布路径。llm-wiki 的差异化是 LLM 编译后的知识网络，因此导出时要保留链接、来源、引用和图谱信息。

**改进方向**

- 先完善 HTML export 的可信预览和引用。
- 增加 Markdown package 导出。
- 评估 PDF/Word/report 导出。
- 设计静态站点发布结构，保留 source/citation 页面。

### 15. 可选项目级加密或明确“不加密”边界

**为什么要改进**

大白话：用户的知识库可能很私密。如果不加密，也要明明白白告诉用户：文件就是本地明文 Markdown，优点是透明可迁移，缺点是别人拿到磁盘就能看。

专业视角：项目当前依赖 OS keyring 存密钥，但 wiki 内容本身未加密。相比 Trilium/SiYuan，这是一个功能差距。由于项目强调 Markdown/local-first，直接加密可能破坏外部编辑和 Git diff，所以需要先做产品决策。

**改进方向**

- 短期：在 Settings/文档中明确本地内容加密边界。
- 中期：研究 per-project optional encryption。
- 评估与 Markdown、Git、外部编辑器、搜索索引、导出的兼容性。
- 不在没有设计前贸然加密，避免破坏项目核心优势。

### 16. 更细的可访问性和键盘工作流验收

**为什么要改进**

大白话：桌面工具不能只靠鼠标点。搜索、导航、会话、弹窗、图谱侧栏这些地方，键盘能不能顺畅走一遍，会直接影响长期使用体验。

专业视角：当前已有 `aria-current`、`role`、`aria-label` 等基础，但复杂 modal、listbox、graph inspector、task drawer、chat log 还需要系统化键盘/focus/focus trap 测试。中英文文本长度也需要持续验收。

**改进方向**

- 给关键 workflow 写键盘验收清单。
- Modal 必须有 focus trap、Esc、Enter、初始焦点和返回焦点。
- 搜索结果支持上下键和回车。
- 图谱节点列表/inspector 需要可键盘访问的替代路径。
- 中英文长文本做溢出测试。

## 建议执行顺序

1. **第一批：可信底座**
   - CSP 与预览隔离
   - CI/CD
   - 发布/更新信任链

2. **第二批：架构减压**
   - AppShell workflow hooks
   - Import/Search/Lint/Chat/Compile service 拆分
   - `npm run check` 与 bundle budget

3. **第三批：用户主路径**
   - First Wiki Run checklist
   - 当前页知识工作台
   - 统一风险确认 Dialog

4. **第四批：核心差异化**
   - Compile 五步可视流水线
   - Source Map / Review Findings
   - 编译 artifact 和 diff 审查

5. **第五批：长期生态**
   - skill/plugin 管理
   - 多格式导出
   - 可选加密研究
   - 更细的 accessibility/performance benchmark

## 一句话结论

这个项目现在最该避免的不是“功能少”，而是“继续把功能堆上去但缺少可信底座和清晰主路径”。先补安全、CI、发布，再把大服务和 AppShell 拆轻，随后集中打磨首次生成 Wiki、当前页知识工作台和可观察 Compile 流水线，项目的完成度会明显上一个台阶。
