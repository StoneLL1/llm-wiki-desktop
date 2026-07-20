# Lint 板块第一性原理代码审查

> 审查日期：2026-07-18  
> 审查基线：本地 `master`，`HEAD 89ecf77`（`fix: prevent chat search unicode snippet panic`）  
> 审查范围：Lint 前端、Tauri 命令、Rust 服务、Agent/Skill 接入、Git 安全链路、任务状态、测试与设计实现  
> 限制：当前仓库未配置可查询的 Git remote，因此只能确认“当前本地 master”，无法独立证明它等同于远端最新 master。

## 1. 审查方法：先定义 Lint 必须成立的事实

本次审查先阅读了项目内的 `skills/llm-wiki-desktop-context/SKILL.md`，并以 `PRD.md`、`SPEC/SPEC.md`、`SPEC/APP_flow.md`、`SPEC/TECH_STACK.md`、`SPEC/BACKEND_STRUCTURE.md`、`SPEC/FRONTEND_GUIDELINES.md` 和 `UI-Frontend-design/lint.html` 为产品与设计基线。

从第一性原理看，Lint 不是“列出若干错误”的页面，而是一条会修改用户知识库的安全工作流。它至少必须同时满足：

1. **判断可信**：扫描成功与“没有问题”必须能被区分；扫描覆盖范围、输入版本和失败原因可解释。
2. **修复安全**：修复必须基于被扫描的那个版本；外部修改不能被静默覆盖；高风险修改先展示真实 diff 并重新确认。
3. **结果可恢复**：修复前有 Git checkpoint，修复后有结果提交；所有实际变更路径都进入安全边界，失败时可恢复。
4. **任务事实可靠**：长任务可取消、可追踪；项目切换只抑制过期展示，不能丢失后端已经存在的任务事实。
5. **本地与 Agent 分工明确**：确定性规则负责可复现事实，Agent 负责语义判断；两者结果可以组合，而不是互相覆盖。
6. **界面帮助决策**：用户先看到整体健康度和可操作列表，再查看窄侧栏详情；按钮文案与副作用一致；中英文、键盘和窄窗口均可用。

按上述原则，当前实现已经有较好的模块边界，但还没有完整闭合“扫描 → 计划 → 确认 → checkpoint → 修复 → 验证 → 提交”这条产品承诺。

## 2. 结论摘要

本次共记录 27 组问题：

- **P0 / 发布阻断：2 组**。主要涉及修复后没有 Git 结果提交，以及扫描时版本与服务端修复计划并未真正锁定。
- **P1 / 高优先级：20 组**。主要涉及 Agent 问题没有修复能力、Lint 绕过共享任务生命周期、Skill 未真正接入、Agent 隔离与 prompt 注入边界不足、错误输出被当成零问题、深度扫描不可扩展、本地/深度结果互斥、忽略规则与历史并发丢数据、前后端类型漂移、确定性规则误报漏报、YAML 修复错误、确认不可重试、重复付费任务、索引与死链修复超范围等。
- **P2 / 体验与工程质量：5 组**。主要涉及布局与权威设计相反、交互副作用不透明、历史审计信息不足、读写错误静默降级、历史报告文件不回收。

最高风险不是“少检查了一个规则”，而是界面给用户造成了比实现更强的安全感：当前 UI 明示“修复后立即提交”，但后端只创建修复前 checkpoint；所谓 scan-time hash 实际是在点击修复/确认时才读取；高风险详情按钮还会立即创建后端 pending action。

## 3. 当前实现值得保留的基础

以下部分方向正确，后续重构应保留：

- `LintService` 已经按 `rules`、`deep`、`fixes`、`ignores`、`reports` 拆分，稳定 facade 与薄 Tauri command 的总体形态符合后端结构规范。
- 本地规则、报告与忽略项使用结构化 DTO/JSON，报告写入使用原子文件存储，历史数量有上限。
- 修复路径有项目根目录约束，写入时存在内容哈希校验；高风险死链/索引修复使用 `PendingAction`，并要求 Git checkpoint 创建成功后才写入。
- Deep Lint 会过滤未知页面路径、给证据不足的 error 降级，问题 ID 做了消歧；Agent 任务已经进入统一任务系统并可取消。
- 本地规则已有较丰富的单元测试，覆盖死链、孤儿页、frontmatter、重复名称、大小写、来源等基础场景。

这些优点解决了“模块是否存在”，但尚未解决跨模块工作流是否端到端正确。

## 4. P0：发布阻断问题

### P0-1 修复后没有结果提交，且实际变更路径超出 checkpoint/报告范围

**证据**

- `src-tauri/src/services/lint_service/fixes.rs:189-205`、`:242-258`、`:273-289` 的安全修复、死链修复和索引修复只调用 `create_scoped_checkpoint(HighRiskOperation)`，写入成功后没有 `FinalResult` checkpoint/commit。
- 批量修复路径 `src-tauri/src/services/lint_service/fixes.rs:380-430` 同样只有修复前 checkpoint。
- `src/features/lint/LintIssueDetails.tsx:182-185` 将“修复后立即提交”显示为已选中且强制；对应文案位于 `src/i18n/locales/en.json:907` 和 `src/i18n/locales/zh-CN.json:908`。
- `LintSafetyPrefs.commitAfter` 仅存在于类型、store 和测试，没有进入后端执行路径。
- `src-tauri/src/services/lint_service/fixes.rs:592-602` 在 checkpoint 之后直接追加 `wiki/log.md`，并在 `:585-589` 删除图缓存；这些副作用没有进入 issue 的 affected paths，也没有最终提交。`append_fix_log` 的错误还被忽略。

**影响**

- 违反 `PRD-LINT-003` 和 SPEC 的“修复前 checkpoint、成功后提交”要求。
- 用户看到强安全承诺，但修复完成后工作树仍可能为 dirty。
- `wiki/log.md` 中原有的用户改动没有被准确纳入修复安全边界；图缓存和日志副作用也无法由当前结果报告完整解释。
- 主文件写入成功后，日志追加或缓存删除失败仍可能返回成功；同时当前流程根本不存在一个能够执行、验证并报告 final commit 成败的阶段。

**建议**

将单项和批量修复统一为服务端事务式流程：计算完整变更计划与全部路径 → 创建 pre-fix checkpoint → 写入 → 重跑相关规则验证 → 创建 `FinalResult` scoped commit → 返回 commit/checkpoint ID。任何后置步骤失败时，应明确返回部分失败并提供恢复动作；不得把 `commitAfter` 留作无效 UI 偏好。日志与缓存要么进入计划，要么改成可重建、可失败且不影响主结果的显式副作用。

### P0-2 所谓 scan-time hash 实际在点击修复时才读取，无法防止扫描后的外部修改

**证据**

- Rust `LintIssue`（`src-tauri/src/models/lint.rs:75-93`）和 TypeScript `LintIssue` 均没有扫描时文件哈希、版本、快照 ID 或 finding fingerprint。
- `src/features/lint/LintView.tsx:202-217` 在用户点击安全修复时才调用 `read_wiki_page` 读取“当前”哈希；批量修复在 `:245-270` 的确认阶段才取哈希。
- `src/features/lint/LintIssueDetails.tsx:70-86` 也是打开确认时才读取哈希，而不是使用产生该 issue 时的基线。
- `src-tauri/src/services/lint_service/fixes.rs:263-289` 的索引修复不使用 issue 对应的扫描快照，而是对当前索引重新取 hash 并全量重建。
- `src-tauri/src/models/lint.rs:70-72` 明确说明修复是 stateless：前端把整条 issue 原样传回；`src-tauri/src/commands/lint_commands.rs:306-322` 没有验证 issue 来自某份服务端报告，反而把客户端提交的 issue 注册为后续 confirmation execution。也就是说，pending action 是“后端签发的”，但它的修复内容仍由客户端 payload 决定。

**失败场景**

1. Lint 在版本 A 上发现问题。
2. 用户或外部编辑器把文件改为版本 B。
3. 用户点击修复，前端此时读取 B 的 hash 并把它当作 expected hash。
4. 后端在 B 上应用由 A 的问题触发的修复；“并发保护”形式上通过，但没有保护 A→B 这段变化。

**影响**

这是典型的 optimistic concurrency 假保护。尤其对索引全量再生成，可能覆盖用户在扫描后手动维护的结构与描述。

**建议**

报告必须携带服务端生成的 `scan_id`、每个文件的 scan-time content hash 和规则证据 fingerprint，并由服务端保存/签发不可由前端改写的 fix plan。应用修复时只接受 plan/action ID，由服务端校验版本与计划内容；若变化则重新扫描/重新规划，并对新的 diff 再确认。不要让前端临时读取当前 hash 来证明旧 finding 仍然成立，也不要把“客户端 issue 经 registry 回存”误当成服务端可信计划。

## 5. P1：高优先级问题

### P1-1 Deep/Agent Lint 只有诊断，没有“可修复问题的计划与执行”

**证据**

- `src-tauri/src/services/lint_service/deep.rs:174-187` 将所有 Agent finding 的 `fixability` 固定为 `None`。
- `src-tauri/src/services/lint_service/fixes.rs:44-67` 只实现 `MissingFrontmatter`、`DeadLink`、`IndexDrift` 三类本地修复。
- Deep finding 只保留模型建议文本，没有结构化 fix plan、候选 diff、风险级别和执行路径。

**影响**

`PRD-LINT-003`、SPEC 9.3 和 APP flow 均要求 Agent 对可处理问题给出修复方案，并在高风险时确认后执行。当前语义检查无法进入产品定义的修复闭环，Lint 仍是诊断器而不是工作流。

**建议**

新增结构化 `LintFixPlan`：目标 issue、基线快照、候选 edits、受影响路径、风险级别、解释、验证规则。Agent 只能生成计划，后端负责路径验证、diff 生成和执行；批量改写、合并、删除或覆盖必须进入 `PendingAction`。执行后重跑本地规则和相关语义检查，再创建结果 commit。

### P1-2 Lint 绕过共享 TaskLauncher，项目切换时会丢任务事实或提交过期界面状态

**证据**

- `src/hooks/useTaskLauncher.ts:51-99` 已封装 Deep Lint/Compile 的核心不变量：后端任务一旦创建就总是 upsert 到全局任务事实；仅当项目 key 仍匹配时才打开 drawer。
- `src/components/app/WorkspaceRouter.tsx:84-85` 直接渲染 `<LintView />`，没有注入共享 launcher。
- `src/features/lint/LintView.tsx:146-200` 自行调用 start/list/get/cancel/recompile。
- `src/stores/lintStore.ts:187-207` 在 Deep Lint 启动响应返回前发生项目切换时直接返回 `null`，因此后端已经存在的任务可能不会进入前端全局任务记录。
- recompile 路径没有同等项目 scope guard，可能在切换后打开旧项目任务 drawer。
- 修复前的 `read_wiki_page` 发生在 store 捕获 project scope 之前；如果读取期间切换项目，后续 apply 可能把旧请求的结果提交到新项目 UI 状态。

**影响**

违反项目 skill 明确规定的任务事实与 presentation commit 边界。表现为后台任务“消失”、旧项目任务抽屉在新项目弹出、旧报告/修复状态污染新项目。

**建议**

Lint 使用统一 `TaskLauncher`，并把“发起项目 key + project epoch + 可覆盖请求 epoch”贯穿完整流程。后端返回的有效任务始终 upsert；drawer、notice、report 和 selection 仅在 scope 仍有效时更新。单项/批量修复也应由服务端接受 report/plan ID，而不是前端跨多个 IPC 自行拼接事务。

### P1-3 `wiki-lint` 目前是硬编码提示词标签，不是真正的 Skill-driven 执行

**证据**

- 仓库包含 `src-tauri/templates/skills/wiki-lint/SKILL.md`，定义六个语义维度、证据要求、严重级别和严格 JSON 输出。
- 当前 Lint 源码中 `wiki-lint` 主要出现在注释、日志和硬编码 prompt；`lint_commands.rs:117` 记录“wiki-lint skill”，但 `AgentService::lint_invocation` 只接收拼接好的 raw prompt。
- `create_lint_workspace` 创建的是空临时目录，没有复制 bundled skill 或项目 skill。
- 对照 `src-tauri/src/services/compile_service.rs:349-356` 和 `:406`，Compile 会把 `wiki-ingest/SKILL.md` 写入隔离 workspace 并明确要求 Agent 遵循；Lint 没有同等机制。

**影响**

模板 skill、项目内定制 skill 和 Rust prompt 会独立漂移；用户无法通过项目 `skills/` 扩展 Lint；也不满足 PRD-AGENT-006 的 Skill 驱动要求。

**建议**

明确 bundled skill 与项目 skill 的优先级和安全校验规则，将最终使用的 skill 复制为只读输入到隔离工作区，prompt 只引用该 contract。增加契约测试，确保输出 schema、六个维度和严重级别不会在两处漂移。

### P1-4 Agent 执行 profile 没有明确声明只读、临时与仓库隔离

**证据**

- `src-tauri/src/services/agent_service.rs:475-515` 构造 Lint Agent invocation。
- Codex 仅使用 `exec --json -`（`:497-501`），没有明确 `--ephemeral`、read-only sandbox、忽略项目规则、跳过仓库检查或固定工作目录等限制。
- Claude 虽使用 `--bare`，其他 provider 也没有等价的只读能力声明；空临时 CWD 只能降低直接访问概率，不能等同于能力边界。
- 现有测试主要断言 Claude 的 `--bare`，没有验证所有 provider 的 Lint profile 都不能写项目或继承非预期规则。
- `src-tauri/src/services/lint_service/deep.rs:74-93` 把可能来自外部来源的不可信 wiki 正文直接拼进 prompt；正文与系统指令之间没有结构化数据边界或 prompt-injection 防护说明。
- Lint 使用 `run_task_streaming` 的持久日志路径；`src-tauri/src/services/agent_service.rs:837-846` 只在 `persist_output_logs == false` 时调用 `harden_import_environment`。因此 Lint Agent 继承 GUI 进程环境，而 Claude/Openclaw/Hermes 也没有显式 no-tools/permission mode。

**影响**

Deep Lint 本应是分析阶段，却没有在命令契约上证明它不会写文件、使用项目外上下文或继承环境中的规则，安全性依赖 provider 默认行为。不可信 Markdown 可能尝试 prompt injection，并接触 Agent 默认工具或继承环境中的认证信息。当前证据说明的是“存在能力与秘密暴露路径且隔离保证未被证明”，并不等同于已经复现 Agent 越权访问或修改项目。

**建议**

为 Lint 定义 provider-agnostic capability profile：临时会话、只读文件权限、固定空 workspace、no-tools 或严格只读工具 allowlist、最小化环境、无项目规则继承、超时和取消。把 wiki 正文标记并封装为不可信数据；每个 provider 显式映射并做 argv/环境契约测试。使用 canary 环境变量和恶意 Markdown 集成测试证明正文不能读取秘密或触发写操作；不具备只读保证的 provider 应拒绝运行或清晰降级。

### P1-5 Agent 输出缺失/损坏会被当作“0 问题、扫描成功”

**证据**

- `src-tauri/src/services/lint_service/deep.rs` 的 `parse_agent_issues_for_known_paths` 在找不到 JSON block 时返回空数组。
- 命令随后仍持久化 report，并把任务标记为 `Succeeded`。
- `DeepLintReport.raw_output` 会随报告持久化，但当前 Lint 历史 UI 不展示它；报告也没有 parse status、拒答/截断信息、warnings、丢弃计数或覆盖率。
- `LintAgentIssue.issue_type` 直接反序列化为宽泛的 `LintIssueType`，边界没有限制为 skill 规定的六种 Deep 类型。

**影响**

模型拒答、CLI 噪声、截断或 schema 漂移都会呈现为假绿色；本地类型也可能被 Agent 越权输出，污染来源与修复语义。

**建议**

将结果分为 `SucceededWithReport`、`Incomplete`、`FailedToParse`，缺失合法 JSON 绝不能等于 clean。引入独立 `DeepLintIssueType` 和严格 schema；报告记录 parser 版本、原始输出摘要、模型/provider、覆盖范围与被丢弃项计数。

### P1-6 Deep Lint 上下文策略存在模型相关的超限风险，也可能看不到关键证据

**证据**

- `DEEP_LINT_EXCERPT_CHARS = 1000`，对所有页面逐页截取开头片段，没有总 prompt budget、候选召回或 section-aware 抽取。
- 200–500 页目标下，单正文片段就可能达到约 20–50 万字符，还未计入路径、规则、本地结果和 prompt；因为没有总预算上限，存在随 provider/context window 而变化的截断或超限风险，但尚无真实 provider 压测证明必然超限。
- 来源、结论、schema 字段、交叉引用等证据常位于页面后部，固定前 1000 字符不可见。
- 本地 baseline 扫描和 Agent 完成后的 known-path 检查不是同一快照，也没有 freshness 标记。

**影响**

小项目可能工作，大项目会截断/超 context；即使未超限，对矛盾、过期信息、来源缺失和弱交叉引用也缺乏足够证据。用户看不到实际扫描覆盖率。

**建议**

采用预算化多阶段流程：本地索引/元数据 → 候选对或候选页面召回 → 按规则提取相关 section/引用上下文 → 分批语义检查 → 服务端聚合。报告展示 considered/scanned/truncated/skipped 数量和快照时间；超预算必须标为 incomplete，而非成功。

### P1-7 本地报告与 Deep 报告互相清空，“全部”筛选通常不可能展示组合结果

**证据**

- `src/stores/lintStore.ts:174-179` 的 local scan 成功后将 `deepReport` 置空。
- `src/stores/lintStore.ts:218-223` 加载 Deep report 后将 `localReport` 置空。
- 历史记录也是单个 local/deep report；Deep report 没有保存它所基于的 local baseline。
- UI 却提供 All/Local/Agent 三种模式，selector 也支持合并两个 report。

**影响**

界面能力与状态模型冲突。用户跑完 Deep 后看不到本地确定性问题；跑本地验证修复又会丢掉 Deep finding。summary 和 passed 数量会随最后一次操作改变含义。

**建议**

用聚合 `LintRun` 表示一次健康检查：同一 snapshot 下包含 local report、deep report、实际 route/provider、coverage 和验证状态。Deep 必须关联 baseline run ID；重跑 local 只更新对应层并明确 Deep 是否 stale，而不是删除它。

### P1-8 Deep 任务失败或取消后仍尝试读取不存在的 report，掩盖真正原因

**证据**

- `src/features/lint/LintView.tsx:139-144` 把 succeeded、failed、cancelled 都视为 terminal，并统一调用 `get_deep_lint_report`。
- 失败/取消通常没有持久化 report，于是 UI 得到 `LINT_DEEP_REPORT_MISSING`，随后清理任务。

**影响**

用户最终看到的是二次错误，而不是 Agent 失败、取消或解析失败的原始原因；重试路径也不明确。

**建议**

仅在 succeeded 且 task 带 report ID 时读取报告。failed/cancelled 保留 terminal task、错误摘要、日志入口和 retry；报告读取失败不得覆盖任务失败原因。

### P1-9 Ignore 只对本地规则生效，粒度与 UI 文案不一致，也没有恢复入口

**证据**

- `src/features/lint/LintView.tsx:227-242` 对所有 issue 都提供 Ignore，并在添加后只重跑 local scan。
- `src-tauri/src/services/lint_service/rules.rs:245-255` 仅在 local scan 末尾应用 ignore；Deep normalize 不读取 ignore。
- ignore key 是 `(path, rule)`，但 UI 文案是“忽略此问题”。忽略一个死链会隐藏该页同类型的全部当前和未来死链。
- 后端提供 list/remove commands，前端 store 会加载 ignores，但没有 unignore/管理界面，加载结果几乎未使用。
- ignore JSON 损坏时后端只 `eprintln` 并当空集合，用户不知道所有忽略项已失效。

**影响**

Agent issue 看似被忽略，实则只是 local rerun 清除了 Deep report，下次 Deep 又出现；本地 issue 会被过宽屏蔽且无法在 UI 恢复。

**建议**

使用稳定 finding fingerprint（规则、path、target/evidence key）并统一应用于 local/deep。增加忽略原因、创建时间、可选失效条件和 Ignore 管理器；支持单项 unignore。损坏配置应显示可恢复错误，不能静默清空。

### P1-10 Rust、TypeScript 与 i18n 的 issue type contract 已经漂移

**证据**

- Rust `LintIssueType` 在 `src-tauri/src/models/lint.rs` 包含 `MissingSourceSection` 和 `InvalidPageType`，local rules 也会产生它们。
- `src/types/lint.ts` 的 union 缺少 `missing_source_section`、`invalid_page_type`。
- 中英文 `lint.issueType` 同样缺少这两个 key。

**影响**

运行时会落到未翻译 key，TypeScript 又错误地认为这些值不可能出现；filter、图标、修复能力映射将继续产生隐性分支遗漏。

**建议**

从 Rust schema 生成 TS/i18n 校验清单，或至少建立跨语言 snapshot/contract test。所有 enum 新增值必须同时覆盖展示名称、说明、严重级别和可修复性。

### P1-11 Wikilink 解析与修复会误报文件夹链接，并无法正确修复 anchor 链接

**证据**

- `src-tauri/src/services/lint_service/rules.rs:323-332` 的 resolution keys 主要是 stem/title/alias，没有系统支持项目相对或文件夹限定形式，如 `[[concepts/foo]]`、`[[wiki/concepts/foo.md]]`。
- `extract_wikilinks` 会移除 `#anchor` 后检查目标，但 `src-tauri/src/services/lint_service/fixes.rs:554-574` 的 `strip_wikilink` 只匹配 `[[target]]` 或 `[[target|alias]]`；`[[ghost#section]]` 可能被报告后无法移除。
- `find_wikilink_line` 也使用变形后的 target，anchor/alias 情形可能找不到真实行。
- `src-tauri/src/services/lint_service/rules.rs:635-644` 计算的是 body-relative 行号，UI 却显示为文件行号，没有加 frontmatter 偏移。
- 重复 stem 时 target lookup 选择第一个页面；虽然另有 duplicate finding，但 link 解析本身仍把歧义目标视为已解析。

**影响**

合法 Obsidian 风格链接被报死链，修复按钮可能返回 stale/no-op，用户跳转行号不准确，歧义链接被错误判定为健康。

**建议**

建立独立、共享的 Wikilink parser/resolver AST，扫描、定位、渲染和修复都使用原始 span。支持 path、扩展名、alias、heading、block ref、CJK/Unicode 和大小写策略；歧义解析应产生专门 finding 而不是任意选第一个。

### P1-12 Index 与资源检查存在关键漏报和重复噪声

**证据**

- `check_index_drift` 读取不到 `wiki/index.md` 时返回空结果，因此“索引完全不存在”反而不报错。
- index membership 主要用字符串 `contains("[[stem]]")` 或 `contains(page.path)`，既会错过合法 alias/文件夹链接，也可能把代码块或普通文本中的偶然字符串当作索引项。
- index 中 ghost link 会同时得到 `DeadLink` 和 `IndexDrift`，同一根因重复计数。
- `missing_resource` 只遍历 frontmatter `sources`，没有检查 Markdown 图片、附件或相对链接。
- 缺失 frontmatter 的页面可能同时产生 MissingFrontmatter、SchemaMismatch、MissingSource、MissingSourceSection；当前“安全修复”只补 type/title，修复后仍保留多条派生错误。

**影响**

确定性层的核心健康度不可靠：最严重的缺失索引被漏报，正常路径被误报，用户面对重复且不可一次解决的问题。其中“缺失 index 不报”是本组的 P1 核心；inline 附件覆盖范围与派生问题 suppression 更接近 P2 完整性缺口，除非产品进一步把所有 Markdown 附件明确列为 P0 规则范围。

**建议**

缺失 index 应为独立 error；index 使用 Markdown/Wikilink AST 比较规范化目标；资源检查覆盖 inline images/links/attachments。规则引擎引入 prerequisite/suppression：根因未满足时把派生问题挂在同一组，修复计划一次补齐必要字段。

### P1-13 Index 修复是过度破坏性的全量覆写，确认预览却没有真实 diff

**证据**

- `src-tauri/src/services/lint_service/fixes.rs:471-504` 读取所有 wiki Markdown，排除 index/log 后按 stem 生成扁平列表。
- 该逻辑可能把 sources、queries、overview 等非目标页面纳入索引，并删除用户维护的 frontmatter、章节、说明、排序与手写内容。
- 任意一个 `IndexDrift` issue 都会触发同一个全量 overwrite。
- `src-tauri/src/services/lint_service/fixes.rs:530-550` 创建 pending preview 时 `diff` 为空，没有把真正的 before/after 给用户。
- 批量扫描可能为多个 drift issue 创建多个确认；首次重建后其余确认仍可能保留并再次覆写。

**影响**

这是项目定义中的高风险覆盖，但当前确认只说明动作，不展示真实结果；同时没有扫描时基线和最终 commit，组合风险很高。

**建议**

把 index 视为一种可配置文档结构，而不是可随意重建的缓存。先生成一次合并计划和真实 unified diff，保留非托管区段或使用明确 generated markers。所有 index drift 合并为一个 action；文件改变或第一次 action 完成后其余 pending action 自动失效。

### P1-14 Lint 页面状态在重扫、快速历史切换和项目切换时仍会残留/乱序

**证据**

- local rerun 会清理部分 fix 状态，但不会系统清理 selected issue、单项确认和 batch confirmations。
- 快速点击两个 history entry 时没有同项目 request epoch，较早请求可能后返回并覆盖较新的选择。
- `reset()` 复用模块加载时的 `initial.safetyPrefs`；运行中改变 localStorage 后，项目切换可能恢复成旧快照。
- `cancelHighRisk` 的错误分支没有与成功分支等价的 project guard。

**影响**

用户会看到已过期详情、已解决问题的确认弹窗，或历史记录跳回上一条；跨项目错误提示可能落到当前项目。

**建议**

明确 store 状态机：`idle/running/report/plan/confirm/applying/verifying`，每个 report/plan 都绑定 project key、project epoch、request epoch 和 run ID。新扫描使旧 selection/plan/pending 全部失效；安全偏好从当前持久化源读取，不复用模块初始化快照。

### P1-15 单条死链确认会替换全文所有同目标链接，实际改动超出 finding 与预览

**证据**

- local scan 对同一页面、同一 target 的 finding 做去重，因此用户看到的是一条 issue。
- `src-tauri/src/services/lint_service/fixes.rs:230-256` 把整份原始 Markdown 交给 `strip_wikilink`。
- `src-tauri/src/services/lint_service/fixes.rs:557-574` 使用全局字符串替换，会移除该 target 的所有普通链接和 alias 链接；因为输入包含完整原文，同样文本若出现在 frontmatter 也可能被改变。
- `src-tauri/src/services/lint_service/fixes.rs:507-522` 的确认预览只是模板化的一处 before/after，`diff` 为空，没有揭示全部 replacement spans。

**影响**

用户确认的是“一条死链 finding”，实际写入可能改变正文多处甚至 frontmatter；这违反“展示什么、确认什么、只写什么”的最小变更原则。

**建议**

扫描阶段保留每个原始 wikilink span，修复计划列出全部精确 edits；默认一次只改用户选择的 span，批量同目标替换必须单独说明。确认预览使用基于 scan snapshot 的真实 unified diff，后端按 span/hash 应用而不是全文字符串替换。

### P1-16 三种 Agent 把完整 wiki 摘录放进进程命令行，形成不必要的隐私暴露面

**证据**

- `src-tauri/src/services/agent_service.rs:484-493` 的 Claude、`:503-506` 的 Openclaw、`:509-512` 的 Hermes 都把完整 Deep prompt 放入 argv；只有 Codex 使用 stdin。
- Deep prompt 包含页面路径、元数据和最多每页 1000 字符正文摘录。

**影响**

命令行参数可能出现在同用户进程列表、诊断工具、崩溃报告或安全软件遥测中。即使内容最终只发送给用户选择的 Agent，argv 仍扩大了本地泄漏面。

**建议**

所有 provider 统一通过 stdin、受保护临时句柄或 provider 明确支持的安全输入通道传 prompt；命令行只保留不敏感 flags。测试断言 argv 不含页面标题、路径、正文或 API/认证数据。

### P1-17 Lint history 与 ignore 都是无锁 read-modify-write，并发操作会丢记录

**证据**

- `src-tauri/src/services/lint_service/reports.rs:124-136` 先读取整个 history、在内存插入 entry、再原子覆写。原子写只能防半文件，不能防两个并发 writer 的 last-writer-wins。
- local scan 与 Deep task 完成可以同时调用 `record_history_entry`，因此两边都可能从同一旧版本开始，后写者覆盖先写者的新 entry。
- `src-tauri/src/services/lint_service/ignores.rs:43-90` 的 add/remove 使用相同的无锁读改写模式；快速操作或多窗口调用也可能丢更新。

**影响**

历史审计条目或 ignore 变更会静默消失，且 JSON 文件仍然完全合法，难以从错误日志发现。

**建议**

以 project root 为 key 对 history/ignore 变更串行化，或使用带版本的 CAS/事务封装；读取与写入必须位于同一临界区。增加 barrier 并发测试，证明 local+deep 同时完成、add+remove/多 add 后所有预期变更都保留。

### P1-18 “安全”frontmatter 修复可能生成错误或立即被 Lint 自己判无效的 YAML

**证据**

- `src-tauri/src/services/lint_service/fixes.rs:178-186` 用 `format!("{:?}", page_type)` 和自制 `yaml_scalar` 生成 frontmatter。
- `src-tauri/src/services/lint_service/fixes.rs:577-582` 只在标题包含 `:`、`[`、`]` 时加引号；`null`、`true`、纯数字、`A # B`、换行和其他 YAML 特殊值没有可靠 round-trip。
- 非标准目录页面会推断为 `WikiPageType::Other` 并写出 `type: Other`，但 `src-tauri/src/services/lint_service/rules.rs:541-553` 的合法类型列表不接受 `other`。修复 MissingFrontmatter 后可能立即产生 InvalidPageType。

**影响**

标记为 `safe` 的修复可能改变 title 的类型/内容、生成无效 schema，或让“已修复”页面立即出现新 error。

**建议**

使用项目统一 YAML serializer 和规范化 page type 字符串，不手写 scalar。无法从路径确定合法 type 时不要自动修复，或要求用户选择。增加 YAML parse/serialize round-trip、特殊标题、CJK、非标准目录和“修复后重扫零新增错误”测试。

### P1-19 高风险 confirmation 在真正校验/写入前被一次性消费，失败后无法重试

**证据**

- `src-tauri/src/commands/lint_commands.rs:273-303` 先调用 registry `confirm`，之后才重新解析项目、创建 checkpoint、校验 hash 并写入。
- `src-tauri/src/models/confirmation.rs:180` 的 `confirm` 会先从 registry `remove` action；后续任一步失败都不会恢复 token。

**影响**

临时 Git 错误、hash mismatch、文件占用或写入失败都会把确认动作吞掉。用户无法在修复原因后直接重试，只能重新触发甚至重新扫描；UI 又没有解释 action 已消费。

**建议**

使用 `peek → validate/execute → success consume`，或给失败状态保留可重试 token；过期/类型校验也不应在返回清晰结果前破坏证据。对项目解析、checkpoint、hash、write、post-verify 各阶段做故障注入测试。

### P1-20 Deep Lint 启动没有 in-flight 状态，双击会创建多个 Agent/BYOK 任务

**证据**

- `src/stores/lintStore.ts:187-201` 只在 `start_deep_lint` IPC 返回后设置 `runningDeep`。
- Deep 按钮只按 `runningDeep` 禁用；首次 invoke 尚未返回时仍可再次点击。
- 每次后端调用都会先创建新的 background task，store 最终只保存最后返回的 `deepTaskId`。

**影响**

用户一次双击即可启动多个 Agent/BYOK 请求，产生重复计算、重复费用和多个互相覆盖的报告；较早任务还可能不再被 Lint 页面跟踪。

**建议**

点击同步进入 `starting`，使用 project-scoped idempotency key 或后端 single-flight 防重；UI 展示 Starting 并立即禁用。测试用 deferred invoke 连续双击，断言只创建一个后端任务且任务事实不会丢失。

## 6. P2：前端体验、审计和工程质量问题

### P2-1 当前布局与权威 Lint 设计相反，问题列表被压缩成低效窄栏

**证据**

- 权威设计 `UI-Frontend-design/lint.html:9-13` 使用 `minmax(0, 1fr) 320px`：问题列表占主要空间，详情固定 320px。
- 当前 `src/styles.css:2594` 使用 `360px + splitter + minmax(320px, 1fr)`：列表固定窄栏，详情占主要空间。
- Run Local、Deep、Auto-fix、filter 全部位于列表 pane（`src/features/lint/LintView.tsx:311-353`）；toolbar 允许换行，在 360px 中容易变成多行。
- 4 个 summary card 在窄栏中变成 2×2，再叠加 history（最大 132px）和 passed 区域，真正的问题列表可视高度很少。

**影响**

Lint 的主要任务是浏览、筛选和批量处理问题，但屏幕大部分面积给了单条详情；长中文标题、路径、tag 和动作按钮频繁换行，扫描效率低。

**建议重排**

1. 在 work surface 顶部设置全宽 52px feature header：标题、问题/待处理摘要、All/Local/Agent filter、Run/Deep/Auto-fix。
2. 内容区采用 `minmax(0, 1fr) 320–360px`；列表为主，详情为辅。允许 splitter 但默认遵循设计。
3. 恢复宽主列表后，保留权威 `lint.html` 的四张紧凑 summary cards，并让它们在一行内承担 severity/pass 计数；若希望改为单行状态条，应先作为产品设计变更获得批准，而不是在实现中自行偏离。
4. history 收入 popover/drawer，主列表顶部只显示 last checked、route 和 coverage。
5. 大量 findings 使用虚拟列表；列表 row 固定信息层级：severity → issue title/target → path:line → source/type → fix state。

### P2-2 交互语义、可访问性和状态文案存在明显误导

**证据**

- `LintIssueList.tsx` 使用 `div role="button"` 包裹真实 `<button>`，形成嵌套交互区域，键盘焦点与点击传播不清晰。
- 高风险卡片上的 `Details` 实际调用 `onApplyFix`，会立即创建/register 后端 pending action；“查看详情”的文案没有表达副作用。
- 零 issue 时始终使用“Run a lint pass…”空状态；已有 clean report 时没有使用已存在的 `lint.list.allPassed` 文案。
- 列表按 severity + source 分组，不是前端规范要求的 severity + type；issue type 同时作为标题和 tag，缺少目标/文件作为主扫描信息。
- 没有“打开文件/定位到行”动作；loading 按钮只显示省略号；后端英文错误会直接进入中文界面。
- 读取高风险页面 hash 失败时确认动作会被永久禁用，但没有内联错误、retry 或解释。
- 详情中的 before/after 两列在窄面板中仍并排，长文本难读；详情标题缺少清晰 severity icon/color。
- `run_local_lint` 是同步命令，前端只有整页 `loadingLocal`，没有页级进度、取消或耗时阈值；在目标 200–500 页规模下尚无性能证据证明它始终属于短任务。

**建议**

列表 row 使用一个原生选择按钮/链接，动作区为同级 sibling；把“查看详情”和“准备修复计划”拆成不同按钮。为 loading/disabled 提供可访问状态文本。clean、not-run、failed、cancelled、incomplete 分别设计空状态。详情 diff 在窄栏改为上下堆叠或全宽 drawer，并提供打开文件与复制路径。为 local scan 建立性能基线；一旦超过短交互阈值，就应进入可取消、可见进度的统一任务系统。

### P2-3 历史记录与审计信息不足，无法回答“这次到底检查了什么”

**证据**

- Deep history 记录的是请求 route（可能为 `auto`），不是最终解析到的 Agent/provider。
- 历史 UI 没有展示 severity breakdown、scanned pages、route/provider、coverage、snapshot/freshness 或 checkpoint/commit。
- local/deep 记录没有聚合为一次 run；自动 post-fix local scan 会产生较多独立历史，最多只保留 50 条。
- 原始 Agent 输出、parser warnings 和被丢弃 issue 数量不能从 Lint 历史进入。

**影响**

用户无法审计一次“通过”是否真的覆盖全部页面，也无法从历史追踪某次修复对应的 checkpoint、任务、Agent 和最终 commit。

**建议**

历史以 `LintRun` 为单位，关联 local/deep 子阶段、task ID、实际 provider、skill/schema version、snapshot、coverage、问题变化、fix plan、pre-checkpoint 和 final commit。主页面仅显示最近一次摘要，完整历史放独立 drawer/table。

### P2-4 多处读写错误被静默降级为“空内容/空历史”，健康结果不可信

**证据**

- local scan 读取 Markdown 存在 `unwrap_or_default`，不可读文件可能被当成空页继续产生次生诊断，而不是标记 scan incomplete。
- index regeneration 也会对读取失败使用默认空内容。
- ignore/history JSON 损坏主要通过 `eprintln` 后按空集合处理。
- UI 没有统一展示 partial scan、skipped file、corrupt metadata 或 recovery action。

**影响**

文件权限、编码、损坏 JSON 等基础设施错误被伪装成内容问题或零历史，用户无法判断报告是否完整。

**建议**

报告增加 `completeness`、`warnings`、`skipped_paths` 和 machine-readable error codes。无法读取输入时默认不宣告 clean；原子备份/恢复损坏 JSON，并在 UI 提供打开路径、重试和重建选项。

### P2-5 History 只截断索引，不清理 report 正文文件，长期运行会无限增长

**证据**

- `src-tauri/src/services/lint_service/reports.rs:12` 将 history index 限制为 50 条，`:129-135` 只截断 entry 数组。
- 每次 local run 都生成新的 UUID report，Deep 也写独立 `.app/lint-reports/<id>.json`（`:15-56`）；被 history 淘汰的正文文件没有 prune。

**影响**

频繁扫描会让 `.app/lint-reports/` 无界增长，且 Deep report 包含完整 raw model output，磁盘占用与本地敏感内容保留期都超出 UI 可见历史。

**建议**

定义与 history 一致的保留策略，在成功写入新索引后清理不再引用的 report；保留失败审计时也应有明确上限/设置。测试超过 50 次后文件数、被引用关系和清理失败告警。

## 7. 建议的前端信息架构

推荐将页面重排为以下结构，保持 Codex desktop 的紧凑工具感：

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ Lint   12 issues · 3 fixable   [All|Local|Agent]  Last 14:32  Run  Deep │ 52px
├───────────────────────────────────────────────┬──────────────────────────┤
│ [2 Error] [6 Warning] [4 Info] [173 Passed]   │ Selected issue           │
│ Coverage: 184/184 · Local + Agent · fresh     │ severity · type          │
├───────────────────────────────────────────────┤ path:line   [Open]       │
│ ERROR — Missing frontmatter            Fixable│                          │
│ concepts/foo.md:1 · local                     │ Explanation              │
├───────────────────────────────────────────────┤ Evidence / original span │
│ WARN — Weak cross reference                   │ Suggested plan           │
│ guides/bar.md:42 · agent · stale              │ Actual diff (stacked)    │
├───────────────────────────────────────────────┤                          │
│ ... virtualized issue list                    │ Safety                   │
│                                               │ checkpoint / affected    │
│                                               │ [Prepare fix plan]       │
├───────────────────────────────────────────────┴──────────────────────────┤
│ Task/progress · checkpoint · final commit · logs                         │ 28px
└──────────────────────────────────────────────────────────────────────────┘
```

关键视觉调整：

- 继续使用现有 token，不引入营销卡片、渐变或装饰图形。
- summary 保留权威设计中的四张紧凑卡片，但应放在恢复后的宽主列表内并控制高度；severity 只用克制的小图标、左边线或文字色，不铺大面积背景。
- 详情面板固定 320–360px；真实大 diff 使用 drawer/扩大详情，而不是永久牺牲列表宽度。
- 所有 icon-only 控件使用 Lucide 与 tooltip；文本尺寸继续遵循 13/12/11/10.5px 体系。
- 中英文都按最长文案验证；按钮不要用硬编码宽度，路径使用 mono + ellipsis + tooltip/copy。

## 8. 测试缺口

当前测试能证明部分规则函数工作，但还不能证明产品工作流安全。建议补齐以下测试层：

### 8.1 安全与 Git 集成

- 单项/批量修复成功后存在 final result commit，工作树干净。
- checkpoint/commit 包含目标页、`wiki/log.md` 和其他实际副作用；返回值列出全部 affected paths。
- final commit、日志写入或验证失败时的恢复/部分失败行为。
- 扫描后、确认前外部修改文件；旧 report/plan 必须失效。
- index 文件在确认后改变；旧 diff 不得直接应用。
- 在项目解析、checkpoint、hash、write、post-verify 各阶段注入失败；confirmation 必须可重试或给出明确恢复路径。

### 8.2 Agent 与 Skill contract

- 实际使用的 `wiki-lint/SKILL.md` 被复制/加载，项目定制与 bundled fallback 行为明确。
- 所有 provider 的 Lint argv/环境满足只读、临时和隔离契约。
- 注入带指令的恶意 Markdown 和 canary 环境 secret，证明 Agent 不能读取继承秘密、调用非只读工具、访问网络或修改项目；同时断言 argv 不含任何 wiki 正文。
- 无 JSON、截断 JSON、额外字段、拒答、未知 path、非六类 issue 均得到明确状态。
- 200/500 页项目的 prompt budget、分批覆盖率和取消。
- Agent fix plan 的路径验证、真实 diff、高风险确认与 post-verify。

### 8.3 前后端状态集成

- failed/cancelled Deep task 不读取缺失 report，保留原始原因。
- local + deep 同时展示；local 重验只把 Deep 标 stale，不删除。
- 项目切换发生在 start 响应、hash read、apply、history read 的不同时间点。
- 有效后端 task 始终进入全局任务事实，旧项目 drawer/notice 不出现。
- 快速打开两条历史时后发请求获胜。
- 延迟 `start_deep_lint` IPC 后连续点击，断言只创建一个 task；local/deep 同时完成的 barrier 测试断言 history 两条记录都保留。

### 8.4 规则与跨语言 contract

- `[[folder/page]]`、`.md`、alias、`#heading`、block ref、CJK、Unicode、大小写和歧义 stem。
- 缺失 `wiki/index.md`、合法 alias index、代码块假阳性、inline image/attachment 缺失。
- frontmatter 存在时报告的绝对文件行号。
- Rust enum、TS union、en/zh i18n key 的自动一致性。
- Ignore 对 local/deep 的 fingerprint 粒度、unignore、损坏文件恢复。
- frontmatter 修复对 YAML 特殊标量和非标准目录做 parse/serialize round-trip，并断言修复后重扫不会新增 InvalidPageType。
- 并发 add/remove ignore 的 barrier 测试，以及超过 50 次 run 后 report 文件按引用关系回收的保留策略测试。

### 8.5 前端视觉与可访问性

- not-run、clean、running、failed、cancelled、incomplete、stale、confirming 全状态截图。
- 1024px 窄窗口、长中文/英文、超长 CJK 路径和 1000+ findings。
- 纯键盘选择/操作、焦点顺序、屏幕阅读器名称、无嵌套 interactive controls。
- 高风险按钮文案与真实副作用一致；diff 在窄详情中可阅读。

## 9. 推荐整改顺序

### Phase 0：先修安全闭环

1. 引入 scan/report/plan ID 与 scan-time hash，服务端统一生成并校验修复计划。
2. 修复流程增加完整 affected paths、pre-checkpoint、post-verify、final commit 和失败恢复。
3. confirmation 改为执行成功后消费；Lint 接入共享 TaskLauncher/项目 epoch，消除跨项目任务与 IPC race。
4. Deep Agent 默认使用最小环境、no-tools/只读 allowlist 和安全输入通道；在证明隔离前不把不可信正文交给有写能力的 Agent。
5. 暂时移除或改写 UI 中尚未兑现的“修复后立即提交”等承诺。

### Phase 1：让结果诚实、状态可组合

1. Deep parser 严格失败语义、独立 issue enum、coverage/completeness。
2. 引入聚合 `LintRun`，同时保留 local/deep；失败/取消不读取 report。
3. 修复 Ignore fingerprint、管理与 Deep 一致性，并串行化 history/ignore 的 read-modify-write。
4. 增加 Deep `starting`/single-flight，按 history 引用回收 report 正文。
5. 统一 Rust/TS/i18n contract。

### Phase 2：补齐 Agent 修复能力

1. 真正加载 `wiki-lint` skill，并给所有 provider 定义只读 profile。
2. 实现结构化 Agent fix plan、真实 diff、风险分类、PendingAction 和执行验证。
3. 使用预算化、多阶段 Deep Lint，记录实际覆盖率。

### Phase 3：提高确定性规则质量

1. 统一 Wikilink AST/resolver 和绝对 span。
2. 使用统一 YAML serializer 重做 frontmatter safe fix，并保证 fix 后重扫不新增错误。
3. 重做 index membership 与非破坏式 index patch。
4. 扩展资源检查，增加 prerequisite/suppression，减少派生噪声。
5. 把输入错误显式标记为 incomplete。

### Phase 4：重排 UI 并补齐审计

1. 全宽 feature header，列表主栏 + 320–360px 详情栏。
2. 精简 summary/history 占位，增加健康度、freshness、coverage、route 和 last checked。
3. 修正按钮副作用、原生语义、键盘/屏幕阅读器、clean/failed 文案。
4. 虚拟化长列表，增强历史与 checkpoint/commit 可追踪性。

## 10. 验收门槛建议

Lint 在满足以下条件前，不宜对外宣称“安全自动修复”已完成：

- 任意修复都能证明它基于哪个 scan snapshot，外部编辑会使旧计划失效。
- 用户确认时看到的是真实 diff，且实际写入不超出确认的路径与内容。
- 成功结果同时存在 pre-fix checkpoint 和 final result commit；失败可恢复且不伪装成成功。
- Deep Lint 的无输出/坏输出不会显示为零问题；报告能说明覆盖率与实际 Agent/provider。
- 不可信 wiki 正文不能借 Agent 工具或继承环境读取秘密、访问非授权资源或修改项目，也不能出现在进程 argv。
- local/deep 结果可组合，任务在项目切换后仍可追踪而不会污染当前项目 UI。
- Agent 可修复 finding 进入结构化计划和安全执行，而不是永远 `fixability: none`。
- 前端遵循“列表为主、详情为辅”的权威布局，所有动作名称与副作用一致。

完成上述门槛后，再扩充更多规则才会产生净收益；否则新增规则只会放大现有的状态、安全和交互债务。
