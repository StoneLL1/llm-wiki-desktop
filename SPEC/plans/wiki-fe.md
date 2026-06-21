# Wiki 前端壳 P0 + P1 修复账本

> 执行日期：2026-06-21  
> 唯一范围：Wiki 前端壳 P0 + P1。实现文件仅限 `src/` 与 `src/styles.css`；账本、roadmap 与 progress 仅用于过程记录。  
> 对照优先级：`SPEC/PRD.md` → `UI-Frontend-design/wiki.html` + `assets/app.css` → `SPEC/roadmap/wiki.md`。

## 状态摘要

- P0：3 项 pending，1 项 verified，0 项 blocked
- P1：2 项 pending，0 项 verified，0 项 blocked
- blocked：无

## 执行项

### WIKI-FE-01 — P0 阅读态 frontmatter 与 prose 对齐

- status: verified
- 意图：把裸 YAML `<pre>` 改为设计稿 `.frontmatter` / `.frontmatter__row` 两列卡片；把 `app.css` 的 `.prose` 全套规则等价迁移到 `.wiki-prose`；落实 wikilink pill 与 missing 状态。
- 决策：不增加 YAML 依赖；前端只做安全、容错的展示级解析，保留未知字段顺序，复杂缩进行以原始文本降级，避免改变文件内容。
- 验收：常见数组、标量和未知字段均可读；无 frontmatter 时不渲染卡片；标题、段落、列表、代码、引用、链接、表格、分隔线和 wikilink 与设计 token 对齐。
- 预计文件：`src/features/wiki/MarkdownReader.tsx`、`src/features/wiki/wiki.test.tsx`、`src/styles.css`
- 完成位置：`src/features/wiki/MarkdownReader.tsx:31,92-107`；`src/styles.css:253-424`；`src/features/wiki/wiki.test.tsx:257-293`

### WIKI-FE-02 — P0 Milkdown 格式工具条

- status: pending
- 意图：增加 28px 工具条：加粗、斜体、标题、链接、代码、引用、撤销、重做。
- 决策：通过 Milkdown editor action/command 接口执行，不绕过 Milkdown 直接改 DOM；按钮提供 tooltip、aria-label 与正确禁用态。
- 验收：各按钮可触发对应 Milkdown command；editor 未就绪或命令不可用时禁用；保存/取消状态保持现状。
- 预计文件：`src/features/wiki/WikiEditor.tsx`、`src/features/wiki/wiki.test.tsx`、`src/styles.css`、`src/i18n/locales/*.json`
- 完成位置：待记录

### WIKI-FE-03 — P0 页面新建、重命名、删除入口

- status: pending
- 意图：树顶栏增加新建入口，文件行提供重命名/删除入口；调用 wiki-BE 已交付命令；重命名和删除都经过 `ConfirmationDialog`，删除确认沿用后端 PendingAction 与 Git checkpoint。
- 决策：前端不自行写文件或创建 checkpoint；所有生命周期动作由 Zustand store 调用 typed Tauri IPC，成功后重扫并保持合理选中态。
- 验收：可新建空页面；重命名确认后刷新到新路径；删除先展示后端影响范围与 checkpoint 状态，再确认执行；取消不写入。
- 预计文件：`src/features/wiki/WikiTree.tsx`、`src/features/wiki/WikiView.tsx`、`src/features/wiki/wikiStore.ts`、`src/features/wiki/wiki.test.tsx`、`src/types/wiki.ts`、`src/types/backend.ts`、`src/i18n/locales/*.json`
- 完成位置：待记录

### WIKI-FE-04 — P0 编译/保存冲突三路 Markdown Diff

- status: pending
- 意图：消费 `FILE_HASH_MISMATCH.details.baselineContent`，展示 original（打开时版本）/ current（磁盘外部版本）/ agent-editor（当前 draft）三路内容与差异，提供保留 current、使用 draft、手动合并三种选择。
- 决策：`baselineContent` 按 wiki-BE 实际契约视为当前磁盘文本；原始打开版本来自 `page.rawMarkdown`，待写版本来自 `draft`。覆盖外部版本必须再次通过后端安全写入路径，不在前端直接落盘。
- 验收：冲突 dialog 有 `role=dialog` / `aria-modal`；三路文本清晰；保留 current 会重载，使用 draft 与手动合并均经过明确确认/安全写入。
- 预计文件：`src/features/wiki/ConflictDiffDialog.tsx`、`src/features/wiki/WikiEditor.tsx`、`src/features/wiki/WikiView.tsx`、`src/features/wiki/wikiStore.ts`、`src/features/wiki/wiki.test.tsx`、`src/i18n/locales/*.json`、`src/styles.css`
- 完成位置：待记录

### WIKI-FE-05 — P1 HTML 预览第三态

- status: pending
- 意图：补 read/edit/preview 第三档、四模板选择器、安全 iframe 和重新生成/打开位置/复制路径操作。
- 决策：复用已交付的 `start_export`、`list_exports`、`read_export_preview`、`open_export_folder` 与现有 export store；不新增后端命令。外部浏览器若无已交付命令则记为 blocked 子项，不越界补后端。
- 验收：选择模板后启动后台任务；生成记录可加载到 sandbox iframe；模板与输出路径可见；可重新生成、打开位置和复制路径。
- 预计文件：`src/features/wiki/GenerateHtmlDialog.tsx`、`src/features/wiki/HtmlPreviewPane.tsx`、`src/features/wiki/WikiView.tsx`、`src/features/wiki/wiki.test.tsx`、`src/i18n/locales/*.json`、`src/styles.css`
- 完成位置：待记录

### WIKI-FE-06 — P1 右侧操作、citation 与反链计数

- status: pending
- 意图：右侧增加四项操作；引用来源编号化并与正文 citation 角标对应；元数据显式显示引用/反链总数；相关页面显示每页指向当前页的链接次数和“查看全部”。
- 决策：反链次数从现有 `wikilinks` 在前端确定性计算；“在图谱中查看”复用 navigation/graph store；生成动作复用 HTML 生成入口；不实现 P2 编辑历史。
- 验收：citation 编号和正文角标可互相定位；反链总数与逐页次数正确；复制 wikilink 写入剪贴板；图谱动作切换视图并选中页面。
- 预计文件：`src/features/wiki/MarkdownReader.tsx`、`src/features/wiki/RelatedPagesPanel.tsx`、`src/components/app/RightContextPanel.tsx`、`src/features/wiki/wiki.test.tsx`、`src/i18n/locales/*.json`、`src/styles.css`
- 完成位置：待记录

## Blocked

- 暂无。发现后端契约缺口时在此记录，并同步向 `SPEC/roadmap/wiki.md` 追加一行，不实现替代后端。

## 收敛清单

- [ ] 所有非 blocked P0/P1 项为 verified
- [ ] `npm run test` 全绿
- [ ] `npm run lint` 全绿
- [ ] `src/` 无非预期 `console.log`
- [ ] import 路径解析通过
- [ ] 双重代码审查完成且有效问题已修复
- [ ] `SPEC/progress.txt` 有最终里程碑
- [ ] 顶部写入 `✅ 本轮完成 @ 2026-06-21`、摘要与文件清单
