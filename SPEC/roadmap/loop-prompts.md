# LLM Wiki Desktop — 板块修复 /loop 提示词集

> 历史提示词集：不得直接使用其中与 Import 相关的旧提示词恢复自动编译或编译期 OCR。Import 的当前决策以 [`../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为准。

> 本文档收录所有板块修复的 `/loop`（自定步调）提示词。每条提示词自包含，可直接整段贴进 `/loop`。
>
> 执行模型：`/loop` 自定步调（dynamic）—— Claude 自己用 `ScheduleWakeup` 续跑，全部 P0+P1 项 `verified` 后停止调度、自动结束 loop。
>
> 深度：每条 loop 修到 **P0 + P1**（不碰 P2）。
>
> 提交策略：**每完成并验证一项 → 对该项改动做一次 git commit**（conventional message，不 `--no-verify`，不 push）。已由用户明确授权，可在 loop 内自动提交。
>
> 拆分原则：后端有实质新工作的**大板块**拆成「后端命令」+「前端壳」两条独立 loop；后端工作轻或已就绪的板块用单 loop。

---

## 0. 强烈建议的执行顺序（避免文件冲突）

多个 loop **不要同时跑**——它们会抢改同一批文件（尤其 `src-tauri/src/services/*.rs` 的 prompt 构造、`src/stores/*`、`lib.rs`）。推荐串行：

1. **`cross-cutting-BE`**（最先；它会改 chat/compile/export/lint 5 个 prompt 文件 + `lib.rs` 托盘 + `confirmation.rs`）
2. `wiki-BE` → `wiki-FE`
3. `import-BE` → `import-FE`
4. `chat-BE` → `chat-FE`
5. `lint-BE` → `lint-FE`
6. **`cross-cutting-FE`**
7. `agent`（单 loop）
8. **`exports`（单 loop**）
9. `shell-dashboard`（单 loop；注意也会动 `lib.rs` 托盘，与第 1 步错开）
10. **`settings`（单 loop）**
11. **`graph`（单 loop）**

> 想并行只能用**独立 git worktree** 隔离，最后合并。

---

## 公共骨架（每条提示词都已内联，此处仅备查）

每条 loop 提示词都遵循同一结构：对照源 → 进度账本（`SPEC/plans/<file>.md`，每项 `pending|in_progress|done|verified`）→ 每次唤醒 8 步（找项→对设计稿→实施→done→test/lint→verified→commit→progress→续跑）→ 收敛终止 → 硬纪律 → 本 loop 特定说明。

---

## 一、拆分板块（BE + FE 各一条）

### 1A. wiki-BE — 后端命令

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 wiki 后端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src-tauri/（新增/扩展 Tauri 命令与 service），不动 src/ 前端 UI。

# 对照源（动手前必读）
- SPEC/roadmap/wiki.md（落差清单 + 文件:行号）
- SPEC/PRD.md（roadmap 第 2 节引用的 PRD-READ/WIKI 条目）
- UI-Frontend-design/wiki.html + assets/app.css（只读，禁改；用于理解字段需求）
- CLAUDE.md（硬边界 + 任务完成检查清单）

# 进度账本
SPEC/plans/wiki-be.md（不存在则创建：从 roadmap 摘本 loop 范围内全部 P0/P1 项，逐条建条目，含涉及文件+验收标准，顶部留"本轮计划"区）。每项 status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个 status != verified 的项；无 → 跳【收敛】。
2. 对照设计稿+PRD 确认该项意图，账本记一句话决策。
3. 实施。守硬边界（高风险写盘前 Git 检查点 / 密钥走 keyring / 路径安全+CJK / i18n）。
4. status→done，记改动 文件:行号。
5. cargo test（后端）+ npm run test + npm run lint 全绿（失败→修→重跑），确认无残留 console.log，status→verified。
6. 对【仅本项】改动的文件 git add + commit（conventional message，不 --no-verify，不 push）。
7. 追加一条 SPEC/progress.txt（时间倒序）。
8. 还有未 verified 项 → 按 /loop 规则安排下次唤醒；没有 → 进【收敛】。

# 收敛（全部 verified 后执行一次，然后终止 loop）
- 账本所有 P0/P1 项 status=verified
- 再跑一次完整 cargo test + npm run test + npm run lint 全绿
- 账本顶部写"✅ 本轮完成 @ 2026-06-21" + 摘要 + 改动文件清单
- 追加 progress.txt 里程碑
- 不再安排下次唤醒（结束 loop）

# 硬纪律
- 只修本 loop 范围内 P0+P1。别板块 / 本板块 P2 / 前端 UI → 在对应 roadmap 记一行，不动手。
- 不改 UI-Frontend-design/ 下任何文件。
- roadmap 与 PRD/设计稿冲突 → 以 PRD/设计稿为准并回改 roadmap。
- 删除/覆盖/批量替换前必须 Git 检查点。

# 本 loop 特定说明
scope（详见 roadmap）：
• 新增后端命令 create_wiki_page / rename_wiki_page / delete_wiki_page（src-tauri/src/commands/wiki_commands.rs 当前只有 scan/read/save/toggle_bookmark）
• rename 要同步更新全仓所有 wikilink 引用（[[old]]→[[new]]，含 alias）；delete 走 PendingAction 确认 + GitService 检查点
• FILE_HASH_MISMATCH 扩展：返回 baseline 文本，供前端做三路 diff（compile_commands.rs / wiki 保存路径）
• 全部新命令接 ProjectContext 路径安全校验，覆盖 CJK 文件名测试
⚠️ 本 loop 只交付后端命令 + 类型 + 测试，不写前端 UI（由 wiki-FE 消费）。
⚠️ 若 cross-cutting-BE 同时改 prompt 文件——先让 cross-cutting-BE 跑完。
```

### 1B. wiki-FE — 前端壳

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 wiki 前端壳 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/ 和 src/styles.css；不新增后端命令（依赖的后端能力须已由 wiki-BE 交付；若发现后端缺口的项，标 status=blocked 并在 roadmap 记一行，跳过该项继续下一个）。

# 对照源（动手前必读）
- SPEC/roadmap/wiki.md
- SPEC/PRD.md（PRD-READ/WIKI 条目）
- UI-Frontend-design/wiki.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/wiki-fe.md（不存在则从头做计划）。status: pending|in_progress|done|verified|blocked。

# 每次唤醒
1. 读账本，找第一个 status ∈ {pending,in_progress} 的项；无 → 跳【收敛】。
2. 对照 wiki.html + app.css + PRD 确认意图，账本记决策。
3. 实施（只动 src/ + styles.css）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿（失败→修→重跑），清残留 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1（非 blocked）verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。blocked 项在账本单独列出。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/后端缺口记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope（详见 roadmap）：
• P0 frontmatter 卡片化（当前 MarkdownReader.tsx:60-61 是裸 <pre> YAML）+ 把 app.css 的 .prose 全套排版迁到 src/styles.css 的 .wiki-prose；wikilink pill 样式落地
• P0 Milkdown 格式工具条（加粗/斜体/标题/链接/代码/引用/撤销重做）—— Milkdown 已真接入，只缺 UI
• P0 文件树新建/重命名/删除入口 UI（调用 wiki-BE 的新命令；删除/重命名经 ConfirmationDialog）
• P0 编译冲突 Markdown Diff 对话框 UI（消费 wiki-BE 返回的 baseline，做三路 diff + 三选项）
• P1 HTML 预览第三态（段控 preview 档 + 模板选择器 + iframe + sandbox）、右侧"操作"区 + citation 编号化 + 反链计数
⚠️ Milkdown 已真接入别重写。i18n：若 cross-cutting-BE 改了 prompt，不影响本前端 loop。
```

### 2A. import-BE — 后端

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 import 后端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src-tauri/。

# 对照源（动手前必读）
- SPEC/roadmap/import.md
- SPEC/PRD.md（PRD-IMP-001 / PRD-IMP-003）
- UI-Frontend-design/import.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/import-be.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 PRD + 设计稿确认意图，账本记决策。
3. 实施（只动 src-tauri/）。守硬边界。
4. status→done，记文件:行号。
5. cargo test + npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/前端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src/。冲突以 PRD/设计稿为准回改 roadmap。raw/sources 替换/删除前必须 Git 检查点。

# 本 loop 特定说明
scope：
• P0 PDF/Office 解析适配器（PRD-IMP-001）：ExtractionService 当前直接返回 Unsupported，需产出文本/页数/字数/图片落盘（可用 pdf-extract / docx 解析 crate；OCR/视觉理解不在本层）
• P0/P1 "打开文件夹为项目"后端支持（dlg-folder 对应的命令/校验）
• 导入后自动触发 Wiki 编译的后端 hook（可由 import-FE 触发，本 loop 只确保命令链路通）
⚠️ 硬约束：导入层只无损保留（原文件/提取文本/图片/来源元数据）；OCR/视觉理解交给编译 Agent/Skill，不在导入层判断图片价值。Readability URL 抓取已成熟，别重写。
```

### 2B. import-FE — 前端壳

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 import 前端壳 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/ 和 src/styles.css；后端缺口标 blocked。

# 对照源（动手前必读）
- SPEC/roadmap/import.md
- SPEC/PRD.md（PRD-IMP-001/003）
- UI-Frontend-design/import.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/import-fe.md（不存在则自己撰写计划）。status: pending|in_progress|done|verified|blocked。

# 每次唤醒
1. 读账本，找第一个 status ∈ {pending,in_progress} 项；无 → 【收敛】。
2. 对照 import.html + app.css + PRD 确认意图，账本记决策。
3. 实施（只动 src/ + styles.css）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1（非 blocked）verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。blocked 项单列。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/后端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 前端 UI 重构对齐设计稿：卡片网格 + 文件表 + 右面板 + 底部确认条（当前是 Tab + 单行输入 + 左右双栏）
• P0/P1 "打开文件夹为项目"对话框 UI（dlg-folder）+ 导入后自动触发编译的前端流
• 消费 import-BE 的 PDF/Office 解析结果做预览
⚠️ 导入层 UI 只做无损保留的呈现，不做图片价值判断。
```

### 3A. chat-BE — 后端（流式）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 chat 后端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src-tauri/。

# 对照源（动手前必读）
- SPEC/roadmap/chat.md
- SPEC/PRD.md（PRD-CHAT-XXX）
- UI-Frontend-design/chat.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/chat-be.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 PRD + 设计稿确认意图，账本记决策。
3. 实施（只动 src-tauri/）。守硬边界。
4. status→done，记文件:行号。
5. cargo test + npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/前端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 BYOK 流式 API：LlmService::complete 改流式（SSE/流式 channel），compile/chat 消费（当前一次性 POST）
• P0 Agent 终态改流式呈现（run_task_streaming 透传增量行）
• route 路由：Auto/Agent/BYOK 的后端判别与可显式指定
⚠️ i18n（prompt 注入语言偏好）归 cross-cutting-BE，本 loop 不动 prompt 的语言指令，只做流式与路由。
⚠️ 必须在 cross-cutting-BE 之后再跑（避免抢改 chat_service.rs）。
```

### 3B. chat-FE — 前端壳

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 chat 前端壳 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/ 和 src/styles.css；后端缺口标 blocked。

# 对照源（动手前必读）
- SPEC/roadmap/chat.md
- SPEC/PRD.md（PRD-CHAT-XXX）
- UI-Frontend-design/chat.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/chat-fe.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified|blocked。

# 每次唤醒
1. 读账本，找第一个 status ∈ {pending,in_progress} 项；无 → 【收敛】。
2. 对照 chat.html + app.css + PRD 确认意图，账本记决策。
3. 实施（只动 src/ + styles.css）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1（非 blocked）verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。blocked 项单列。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/后端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 消息渲染富化：Markdown 渲染 + avatar + time + model badge + citation <sup> 角标 + 消息内 citation 卡片（当前纯文本 whitespace-pre-wrap）
• P0/P1 Agent/BYOK route segment 切换器（消费 chat-BE 的显式路由）
• P0 流式 UI：消费 chat-BE 的流式 channel，逐字呈现 + cursor
• P1 右面板补"原始资料 / 执行路径 / 操作（复制 MD/卡片/标记问题）"三段
⚠️ 硬约束：普通全局搜索不调模型；自然语言问答才走 Chat。
```

### 4A. lint-BE — 后端

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 lint 后端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src-tauri/。

# 对照源（动手前必读）
- SPEC/roadmap/lint.md
- SPEC/PRD.md（PRD-LINT-XXX）
- UI-Frontend-design/lint.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/lint-be.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 PRD + 设计稿确认意图，账本记决策。
3. 实施（只动 src-tauri/）。守硬边界。
4. status→done，记文件:行号。
5. cargo test + npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/前端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src/。冲突以 PRD/设计稿为准回改 roadmap。自动修复前必须 Git 检查点。

# 本 loop 特定说明
scope：
• P0 severity 分级：后端真正发 error 级（死链当前是 warning）——死链/index 漂移等定性为 error，frontmatter 缺失等为 warning
• P0 批量自动修复编排：新增 apply_fixes_batch（一次 Git 检查点 + 批量 manifest 执行 + 回滚），当前只能逐条 apply（每条各做一次 checkpoint）
• P0/P1 lint-ignore 持久化：.app/lint-ignore.json，记录被忽略的 issue（key=path+rule），扫描时排除
⚠️ 硬约束：Lint 双层（本地确定性 + Agent 深度 wiki-lint）；自动修复前 Git 检查点。i18n prompt 归 cross-cutting-BE，须在其后跑。
```

### 4B. lint-FE — 前端壳

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 lint 前端壳 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/ 和 src/styles.css；后端缺口标 blocked。

# 对照源（动手前必读）
- SPEC/roadmap/lint.md
- SPEC/PRD.md（PRD-LINT-XXX）
- UI-Frontend-design/lint.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/lint-fe.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified|blocked。

# 每次唤醒
1. 读账本，找第一个 status ∈ {pending,in_progress} 项；无 → 【收敛】。
2. 对照 lint.html + app.css + PRD 确认意图，账本记决策。
3. 实施（只动 src/ + styles.css）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1（非 blocked）verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。blocked 项单列。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/后端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 批量"自动修复 (N)"主 CTA UI（调用 lint-BE 的 apply_fixes_batch），确认对话框提示 Git 检查点
• P1 摘要卡四宫格（按 severity 计数，消费 lint-BE 的 error 级）/ 已通过区 / 模式分段（本地 vs Agent 深度）
• P1 issue tags / 内联修复按钮 / 修复方案 radio / 安全检查 checkbox / lint-ignore"忽略本次"UI
⚠️ 自动修复前 UI 必须提示 Git 检查点（硬边界）。
```

### 5A. cross-cutting-BE — 后端（最先跑）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 cross-cutting 后端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块（本板块天然横跨多视图后端文件，属正常）。
范围：只动 src-tauri/。

# 对照源（动手前必读）
- SPEC/roadmap/cross-cutting.md（第 2 节逐条落差）
- SPEC/PRD.md + CLAUDE.md「必读硬边界」全文
- UI-Frontend-design/ 相关页 + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/cross-cutting-be.md（不存在则从 roadmap 第 2 节摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 PRD + CLAUDE.md 硬边界确认意图，账本记决策。
3. 实施（只动 src-tauri/）。守硬边界。
4. status→done，记文件:行号。
5. cargo test + npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/前端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope（详见 roadmap 第 2 节）：
• P0 i18n 生成内容语言偏好：5 个 prompt 构造点（chat_service / compile_service / export_service / lint_service / compile_commands::provider_prompt）读 SettingsService::language，注入"按用户语言输出"指令；确定性字段（schema/path/枚举/frontmatter key）保持英文。违反 CLAUDE.md 硬约束，是红线。
• P0 PendingAction 增 checkpoint_hash: Option<String> 字段（confirmation.rs），compile 冲突登记时填入（compile_commands.rs）；后端 DTO 透传到前端
• P0/P1 托盘菜单 i18n（lib.rs:35-82 的 Show/Hide/Quit + tooltip 按 settings.language 本地化）
• P1 BYOK compile 流式进度（每 ≤2s append "Generating..." 或接 chat-BE 的流式）
⚠️ 本 loop 最先跑——它改的 5 个 prompt 文件 + lib.rs 是其它 BE loop 的共享文件。
```

### 5B. cross-cutting-FE — 前端壳

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 cross-cutting 前端 的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/；后端缺口标 blocked。

# 对照源（动手前必读）
- SPEC/roadmap/cross-cutting.md（第 2 节）
- SPEC/PRD.md + CLAUDE.md
- UI-Frontend-design/dashboard.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/cross-cutting-fe.md（不存在则从 roadmap 摘前端相关 P0/P1 建条目）。status: pending|in_progress|done|verified|blocked。

# 每次唤醒
1. 读账本，找第一个 status ∈ {pending,in_progress} 项；无 → 【收敛】。
2. 对照 PRD + 设计稿确认意图，账本记决策。
3. 实施（只动 src/）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1（非 blocked）verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。blocked 项单列。

# 硬纪律
只修本 loop 范围 P0+P1；别板块/P2/后端记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 AppShell 项目切换时触发一次 detect_agents（避免首屏误显"未检测"）
• P0 ConfirmationDialog 按 action.checkpoint_hash 显示 checkpoint 状态（消费 cross-cutting-BE 透传的字段；当前硬编码 checkpointExists={false}）
• P1 ConfirmationDialog destructive 按钮 variant 与样式对齐
• P1 删 AppShell 的 250ms 轮询，统一改事件驱动
⚠️ checkpoint_hash 字段依赖 cross-cutting-BE；若未交付则该项 blocked。
```

---

## 二、单 loop 板块（后端轻 / 已就绪）

### 6. agent（前端壳为主）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 agent 板块的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：主要动 src/；仅在本板块需要时做必要的 src-tauri 类型/接线微调（后端 spawn/checkpoint/cancel 已就绪，勿重写）。

# 对照源（动手前必读）
- SPEC/roadmap/agent.md
- SPEC/PRD.md（PRD-AGENT-XXX / PRD-WIKI-XXX）
- UI-Frontend-design/agent.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/agent.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 agent.html + app.css + PRD 确认意图，账本记决策。
3. 实施。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿（若动了 src-tauri/ 则加 cargo test），清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本板块 P0+P1；别板块/P2 记 roadmap 不动手。不改 UI-Frontend-design/。不自动改后端核心 service（spawn/checkpoint/cancel 已就绪）。冲突以 PRD/设计稿为准回改 roadmap。不静默安装 Agent。

# 本 loop 特定说明
scope：
• P0 "运行 Agent"对话框：Skill 选择 / 执行路径 / Git 检查点提示 / 后台 toggle（当前 AgentView 只有一个"编译 Wiki"按钮直触发）
• P0 核心操作四宫格 + BYOK 卡片化 + 右面板 Agent 配置区（设计稿核心三块全缺）
• P0/P1 CLI 行 / 任务行样式族（.cli-row / .ingest-card / .dotstatus / .sumcard）；terminal 日志 level 染色 + 复制/清空/全屏；进度条补 aria 属性
⚠️ 后端 Agent CLI spawn / Git 检查点 / 冲突确认 / 取消全已就绪，本 loop 主要补前端壳。
```

### 7. exports（FE + 轻后端）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 exports 板块的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：动 src/ 和 src-tauri/（轻后端：prompt 接收模板参数）。

# 对照源（动手前必读）
- SPEC/roadmap/exports.md
- SPEC/PRD.md
- UI-Frontend-design/exports.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/exports.md（不存在则自己做计划）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 exports.html + app.css + PRD 确认意图，账本记决策。
3. 实施。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿（动 src-tauri/ 加 cargo test），清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本板块 P0+P1；别板块/P2 记 roadmap 不动手。不改 UI-Frontend-design/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 新建导出对话框：源页浏览 / 模板下拉 / 执行路径 / 4 选项（当前用户只能用顶栏内联裸 input）
• P0/P1 已生成列表表格化 + 失败状态徽章（ExportStatus::Failed 已建模但前端不渲染）+ 重试按钮
• P1 模板选择端到端：prompt 接收用户选择的模板参数（改 src-tauri export prompt，但别破坏 template.html 的"不含 schema/lint"回归测试）
⚠️ 硬约束：HTML/卡片/报告全部通过 skills/html-* 驱动；模板只影响输出样式，不改 schema/Lint/Agent。须在 cross-cutting-BE 之后跑（避免抢改 export_service prompt）。
```

### 8. shell-dashboard（FE + 微后端）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 shell+dashboard+启动页 板块的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：主要动 src/；微后端（lib.rs 托盘 close 接线）按需。

# 对照源（动手前必读）
- SPEC/roadmap/shell-dashboard.md
- SPEC/PRD.md
- UI-Frontend-design/index.html、launch.html、dashboard.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/shell-dashboard.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照设计稿 + PRD 确认意图，账本记决策。
3. 实施。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿（动 src-tauri/ 加 cargo test），清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本板块 P0+P1；别板块/P2 记 roadmap 不动手。不改 UI-Frontend-design/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 Dashboard 信息密度：健康行 / 统计六宫格 / 活动时间线 / 快速操作四象限 / 主题分布 / 图谱预览（当前 DashboardView 退化为状态表）
• P0 启动页三栏布局：项目卡网格 / 快速操作 / Agent 检测侧栏 / 模板侧栏（当前 ProjectStartView 只是居中表单）
• P0 关闭主窗口最小化到托盘的 on_window_event 接线
⚠️ 关于托盘 close 拦截：shell-dashboard 报告说"断链"、cross-cutting 报告说"已接线"。动手前先读 src-tauri/src/lib.rs:31-100 核实真实状态，再决定改代码还是回改 roadmap 文档。
⚠️ 本 loop 也会动 lib.rs，须与 cross-cutting-BE（托盘 i18n）错开，不能并行。
```

### 9. settings（FE + 轻后端）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 settings 板块的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：动 src/ 和 src-tauri/（更新检查真查源）。

# 对照源（动手前必读）
- SPEC/roadmap/settings.md
- SPEC/PRD.md（PRD-SET-XXX）
- UI-Frontend-design/settings.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/settings.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 settings.html + app.css + PRD 确认意图，账本记决策。
3. 实施。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿（动 src-tauri/ 加 cargo test），清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本板块 P0+P1；别板块/P2 记 roadmap 不动手。不改 UI-Frontend-design/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 section 内容归位：安全 / 上下文窗口 / Agent 超时 / Skill 等多个 section 当前错位或缺失
• P0 样式族：apikey-row / formrow / seg / toggle（当前是手搓卡片，与设计 token 脱节）
• P0 更新检查去 mock（PRD-SET-005，当前 window.confirm 假弹窗）—— 后端真查更新源
⚠️ 硬边界红线：API Key 只走 keyring，绝不写项目文件/JSON/日志，UI 只显"已配置"不回显完整密钥。任何违规直接 P0。不静默安装 Agent。
```

### 10. graph（纯前端）

```
你在 /loop 自定步调模式下工作。本轮唯一目标：把 graph 板块的 P0+P1 全部修复并通过验证。不碰 P2，不碰其它板块。
范围：只动 src/ 和 src/styles.css。

# 对照源（动手前必读）
- SPEC/roadmap/graph.md
- SPEC/PRD.md（PRD-GRAPH-001..005）
- UI-Frontend-design/graph.html + assets/app.css（只读，禁改）
- CLAUDE.md

# 进度账本
SPEC/plans/graph.md（不存在则从 roadmap 摘 P0/P1 建条目）。status: pending|in_progress|done|verified。

# 每次唤醒
1. 读账本，找第一个非 verified 项；无 → 【收敛】。
2. 对照 graph.html + app.css + PRD 确认意图，账本记决策。
3. 实施（只动 src/ + styles.css）。守硬边界。
4. status→done，记文件:行号。
5. npm run test + npm run lint 全绿，清 console.log，status→verified。
6. 对【仅本项】改动 git add + commit（conventional，不 --no-verify，不 push）。
7. 追加 SPEC/progress.txt。
8. 未完 → 安排下次唤醒；完 → 【收敛】。

# 收敛
全部 P0/P1 verified + 完整 test/lint 全绿 + 账本顶部"✅ 本轮完成 @ 2026-06-21"+摘要+文件清单 + progress 里程碑 + 不再调度。

# 硬纪律
只修本板块 P0+P1；别板块/P2 记 roadmap 不动手。不改 UI-Frontend-design/。不自动改 src-tauri/。冲突以 PRD/设计稿为准回改 roadmap。

# 本 loop 特定说明
scope：
• P0 画布内悬浮层：左下图例 graph-legend + 右上信息卡 graph-info（当前只有控件层）
• P1 6 类型 checkbox 筛选 + 度数阈值滑块（当前仅模糊搜索框）
• P1 SVG/PNG 导出、选中节点相邻列表、画布网格底纹/圆角视觉
• 控件版式：设计稿左上纵向悬浮 vs 当前顶部横条 —— 对齐设计稿，改动大则记决策
⚠️ 硬约束：图谱首版"每页一节点、边统一表示'相关'，不实现复杂关系类型和证据系统"。不要超范围加关系类型/证据系统。
```

---

## 附：使用速查

- **跑法**：`/loop`，选自定步调（无 interval），把对应代码块整段贴进去。
- **进度账本**：每条 loop 在 `SPEC/plans/<file>.md` 自建，是续跑的"断点"。中断后重贴同一提示词即可从断点继续。
- **提交**：每项验证通过后自动 commit 一次，不 push。可用 `git log --oneline` 复盘。
- **结束**：全部 P0+P1 verified 后 loop 自行停止调度。
- **遇到 blocked**（前端 loop 发现后端缺口）：该项标 `blocked` 跳过，账本单列，继续下一项；loop 结束后人工决定是否补跑对应 BE loop。
- **改文档**：实施中若发现 roadmap 与 PRD/设计稿不符，以 PRD/设计稿为准并回改 roadmap 对应板块文件。
