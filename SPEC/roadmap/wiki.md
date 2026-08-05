# Wiki 板块落差与实施计划

> 对照源：UI-Frontend-design/wiki.html + assets/app.css + SPEC/PRD.md
> 当前实现：src/features/wiki/、src/types/wiki.ts
> 项目布局与访问边界：[`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。原生、兼容、受限、只读和恢复模式共用 Wiki 视图；页面根与元数据由后端 `ProjectContext.layout`/capabilities 提供，不能假定所有知识库都有原生 `wiki/`、根 `purpose.md` 或 `schema.md`。所有 mutation 需要 trusted + writable，并遵守 Git/确认策略。

## 0. 现状摘要

Wiki 板块已具备核心骨架但远未对齐设计稿。左侧文件树（`WikiTree.tsx`）实现了扫描、筛选、类型 pill、文件夹展开与选中态，但没有新建/重命名/删除入口。中间区只做了"阅读 (`MarkdownReader.tsx`) / 编辑 (`WikiEditor.tsx`)"两态切换，设计稿第三态 **HTML 预览** 完全缺失，顶栏的"生成 HTML"按钮、模板选择对话框、面包屑右侧操作组、HTML 预览 iframe 也全部没有。编辑器是真 Milkdown（`@milkdown/kit` + commonmark + gfm + history + listener + nord 主题），不是 textarea 占位，这点符合硬边界；但阅读渲染虽接入了 `remark-gfm + remark-math + rehype-katex + rehype-highlight` 与 wikilink 解析，**frontmatter 只用 `<pre>` 裸吐 YAML**，未渲染成设计稿的 `frontmatter__row` 两列卡片。右侧 `RelatedPagesPanel.tsx` 实现了元数据/标签/反链/来源，但没有"引用来源带编号""编辑历史""操作区（生成 HTML / 卡片 / 图谱中查看 / 复制 wikilink）"。

三个最突出缺口：
1. **HTML 预览态完全缺失**（设计稿第三态 + 模板选择器 + `exports/html/` 生成交互）。
2. **文件树没有新建/重命名/删除/星标右键操作**，与 PRD-READ-001 的"可浏览"达到最低线，但与设计稿顶栏"+"按钮和右侧面板"操作"区脱节。
3. **frontmatter 阅读态与设计稿严重不一致**（裸 YAML pre 块 vs. 两列卡片），且缺少"上次编译时间/Git 提交"页脚与 citation-ref 角标渲染。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 左侧 wiki 文件树（扫描/折叠/选中/计数） | 树状展示 `wiki/` 子目录，文件夹带 chevron + 文件夹 icon + 子文件数；文件按类型区分图标（md / pdf / link / doc）；`index.md` 用 list icon、`log.md` 用 history icon | 已实现扫描、折叠、选中、文件计数；index/log 图标特例已做；但 pdf/link/doc 等来源类型图标未做 | 🟡部分实现 | P1 | `src/features/wiki/WikiTree.tsx:20-29,181-258` |
| 树顶栏：筛选框 + 新建页面按钮 | 26px 筛选 input + 旁边 `+` 按钮（新建页面） | 筛选框有；"+"位被替换成 RefreshCw 刷新图标，**没有新建页面入口** | 🟡部分实现 | P0 | `src/features/wiki/WikiTree.tsx:80-88` |
| 类型快速筛选 pills（全部/实体/概念/来源/综合/对比/查询） | 横排 20px 高 pill，激活态 accent-soft | 已实现，`WIKI_PAGE_TYPES` 驱动 | ✅已完成 | P1 | `src/features/wiki/WikiTree.tsx:90-104` |
| 中间区顶栏（面包屑 + 保存状态 + read/edit/preview 段控 + 生成 HTML + 星标 + 复制路径 + 更多） | 完整操作组，面包屑用 `font-mono 12px`；段控三档含 "HTML 预览"；"生成 HTML" primary 按钮 | 面包屑、保存状态、read/edit 两档段控、星标按钮已实现；**段控缺 preview 档**、**缺 "生成 HTML" 按钮**、缺"复制路径"和"更多" | 🟡部分实现 | P0 | `src/features/wiki/WikiView.tsx:66-145` |
| 阅读视图 prose 排版（标题/frontmatter/段落/表格/blockquote/code/hr/参考资料/页脚） | `.prose` + `.frontmatter`（two-column grid `frontmatter__row`） + `citation-ref` 角标 + wikilink pill + 末尾"上次编译"页脚 | react-markdown + remark-gfm 已渲染基础排版；**frontmatter 仅 `<pre>` 裸 YAML**（`MarkdownReader.tsx:60-61`）；**citation-ref 角标不支持**；**末尾编译页脚没有**；wikilink 样式需 styles.css 补 `.prose .wikilink`（目前依赖全局） | 🟡部分实现 | P0 | `src/features/wiki/MarkdownReader.tsx:47-97` |
| frontmatter 渲染 | `.frontmatter` 卡片，`frontmatter__row` 120px grid，key 灰、value 主色 | 仅 `<pre className="wiki-frontmatter">{frontmatterYaml}</pre>` | ❌缺失 | P0 | `src/features/wiki/MarkdownReader.tsx:59-62` |
| 数学公式（KaTeX） | 阅读视图支持行内/块级数学 | 已接入 `remark-math + rehype-katex` | ✅已完成 | P0 | `src/features/wiki/MarkdownReader.tsx:6-7,63` |
| 代码高亮 | 阅读视图代码块语法高亮 | 已接入 `rehype-highlight` | ✅已完成 | P0 | `src/features/wiki/MarkdownReader.tsx:6,64` |
| 双向链接（wikilink） | `[[Target]]` 与 `[[Target|Alias]]` 渲染为 accent-soft pill，点击跳转；未解析的标 missing | 已实现：预处理 `[[..]]` → `wikilink://` scheme，大小写无关、按 stem/title/alias 解析；missing 带 `wikilink--missing` class；测试覆盖 | ✅已完成 | P0 | `src/features/wiki/MarkdownReader.tsx:18-94` |
| Milkdown WYSIWYG 编辑器 | 技术栈硬约束指定 Milkdown（ProseMirror），工具条加粗/斜体/标题/链接/代码/引用/撤销/重做 + "未保存/保存/取消" 按钮 | 真接入 `@milkdown/kit/core` + commonmark + gfm + history + listener + nord；保存/取消/重载按钮有；**工具条完全缺失**（加粗/斜体/标题/链接/代码/引用/撤销/重做都没有 UI）；Cmd/Ctrl+S 快捷键已做 | 🟡部分实现 | P0 | `src/features/wiki/WikiEditor.tsx:48-69,87-163` |
| HTML 预览态（第三档段控 + 模板选择器 + iframe 预览） | 段控 `preview` 档；`dlg-gen-html` 模板卡片网格（beautiful-read/knowledge-card/concept-map/project-report）；chrome 模拟框 + iframe；重新生成/打开位置/外部浏览器/复制路径 按钮 | **完全未实现** | ❌缺失 | P1 | `src/features/wiki/WikiView.tsx:111-124` |
| 外部修改冲突对话框（Markdown Diff + 保留/使用/手动合并） | `dlg-conflict` 三路 diff + 三选项按钮 | 仅在编辑器内做 `saveState === "conflict"` banner + "重新加载"按钮；**没有 diff 视图、没有三选项确认对话框** | ❌缺失 | P0 | `src/features/wiki/WikiEditor.tsx:119-127,143-147`；`src/components/app/ConfirmationDialog.tsx`（通用确认） |
| 右侧元数据面板（路径/类型/创建/更新/字数/引用/反链） | `rightpanel__meta` 两列 `<dl>`，含"引用 3 / 反链 12" | 路径/类型/创建/更新/字数/文件大小已做；**缺"引用"计数**（sources 列表在，但没有计数显式标注）；反链计数在 backlinks 段标题；文件大小用 formatBytes，设计稿没有此项（多余/可保留） | 🟡部分实现 | P1 | `src/features/wiki/RelatedPagesPanel.tsx:47-75` |
| 右侧标签 pills | 灰色 pill | 已实现 | ✅已完成 | P2 | `src/features/wiki/RelatedPagesPanel.tsx:77-93` |
| 右侧"引用来源"（编号 citation 列表） | 每条带 `citation__idx` 编号 + 截断标题 | 仅以灰色 mono 文字列出 `page.sources`，**无编号 citation 视觉** | 🟡部分实现 | P1 | `src/features/wiki/RelatedPagesPanel.tsx:121-134` |
| 右侧"相关页面"反链列表 | `relpage` 行带 icon + 标题 + 反链次数 | 有 backlinks 列表，**缺"反链次数"计数**；缺"查看全部 N 个反链"链接 | 🟡部分实现 | P1 | `src/features/wiki/RelatedPagesPanel.tsx:95-119` |
| 右侧"编辑历史" | 时间 + 作者（claude/你）+ 摘要 + 增删行数 | **完全未实现** | ❌缺失 | P2 | 无 |
| 右侧"操作"区（生成 HTML / 生成卡片 / 图谱中查看 / 复制 wikilink） | 4 个 block 按钮 | **完全未实现**（生成 HTML 入口在中间顶栏也缺失） | ❌缺失 | P1 | 无 |
| 页面级操作：新建/重命名/删除/星标/复制路径 | "+" 新建、星标按钮、复制路径按钮；重命名/删除（设计稿未直接画出，但 PRD-READ-004 / Git 检查点硬边界隐含） | 后端 `create_wiki_page` / `rename_wiki_page` / `request_delete_wiki_page` 已实现并在 `lib.rs` 注册，`wikiStore` 也有对应调用；星标已实现。**剩余缺口是文件树/页面操作 UI 接线与复制路径入口** | 🟡部分实现 | P0 | `src/features/wiki/WikiTree.tsx:80-88`、`src/features/wiki/wikiStore.ts:398-457`、`src-tauri/src/commands/wiki_commands.rs:73-182`、`src-tauri/src/lib.rs:164-166` |
| 搜索快捷键 ⌘K | 顶栏全局搜索框带 `⌘K` 提示 | 不在 wiki 板块内部（在 `AppShell` 顶栏）；wiki.html 设计稿把搜索放在 topbar | 🟡部分实现 | P1 | `src/components/app/AppShell.tsx`（顶栏） |
| 兼容 / 访问模式 | 同一 Wiki UI 浏览原生与兼容 Markdown；restricted/read-only 只读；recovery 显示诊断 | 当前树与命令主要硬编码原生 `wiki/` 结构，未消费 layout/access/capabilities | ❌缺失 | P0 | `WikiTree.tsx`、`wikiStore.ts`、`wiki_commands.rs`、项目上下文 DTO |

## 2. 功能落差（PRD 对照）

- [ ] **项目布局与访问模式（P0）**：树扫描、读取、反链与搜索通过后端解析的 page roots 工作，兼容 Obsidian/Markdown vault 时不要求根 `purpose.md`/`schema.md`；restricted 与 trusted read-only 隐藏/禁用新建、保存、重命名、删除、生成和修复，并给出“信任知识库”或权限说明；recovery 把诊断与备份/修复入口置顶。所有写命令在后端重验 canonical identity、trust、writability、hash 与 Git policy。
- [x] **PRD-READ-001 后端部分（create/rename/delete_wiki_page 命令）** @ 2026-06-21：后端三命令已落地（`create_wiki_page` / `rename_wiki_page` / `request_delete_wiki_page` + `confirm_pending_action` 的 `DeleteWikiPage` 分支），rename 同步重写全仓 `[[old]]`→`[[new]]`（含 alias/anchor/CJK/嵌套括号）+ 前置 Git 检查点，delete 走 PendingAction + 双 checkpoint（pre HighRiskOperation + post FinalResult）+ 失败回滚，FILE_HASH_MISMATCH 返回 baselineContent。**前端 UI（树右键菜单/`+` 按钮/ConfirmationDialog 接线）仍缺，属前端板块，不在本 loop 范围。** 涉及后端文件：`src-tauri/src/commands/wiki_commands.rs`、`src-tauri/src/services/search_service/pages.rs` (`SearchService::create_page` / `rename_page`)、`src-tauri/src/utils/markdown_utils.rs`、`src-tauri/src/services/file_store.rs`。
- [ ] **PRD-READ-001 前端部分（文件树新建/重命名/删除 UI）**：现状 = 树只读 + 只能通过编辑器覆盖现有页面 → 目标 = 顶栏"+"按钮新建页面（带模板选择），右键行项重命名/删除，删除前创建 Git 检查点并经用户确认 → 涉及文件 = `src/features/wiki/WikiTree.tsx`、`src/features/wiki/wikiStore.ts`（接已落地的 `create_wiki_page` / `rename_wiki_page` / `request_delete_wiki_page` + `confirm_pending_action`）→ 验收 = 能新建空页面、重命名会同步更新所有 wikilink 引用、删除走 `ConfirmationDialog` + Git 检查点。
- [ ] **P2 wikilink 重写边界（`rewrite_wikilinks` 已知限制，与本 loop `extract_wikilinks` 行为一致）** @ 2026-06-21：`rewrite_wikilinks` 与 `extract_wikilinks` 同源，共享以下已知限制（非本 loop P0/P1 范围，记录待后续统一处理）：①不跳过代码块/行内 code span 中的 `[[old]]`，会重写代码示例内容；②不识别路径式 wikilink `[[concepts/old]]`（只按 stem 精确匹配，Obsidian 两者都解析到同一文件）；③rename 引用重写用 `std::fs::write` 非原子写（file_store 用 `write_atomic`）。涉及文件 = `src-tauri/src/utils/markdown_utils.rs`、`src-tauri/src/services/search_service/pages.rs` (`SearchService::rename_page`)。验收 = 代码块内的 wikilink 不被重写；`[[dir/old]]` 也能被重写；引用重写走原子写。
- [ ] **PRD-READ-002 frontmatter 卡片化渲染**：现状 = `<pre>` 裸 YAML（`MarkdownReader.tsx:60-61`） → 目标 = 解析 YAML 后按 `.frontmatter` + `.frontmatter__row` 两列 grid 渲染，120px key 列 → 涉及文件 = `src/features/wiki/MarkdownReader.tsx`、`src/styles.css`（复用设计稿 `.frontmatter` token） → 验收 = type/tags/aliases/created/updated/sources 等字段以卡片样式显示，未知字段也能优雅降级。
- [ ] **PRD-READ-003 编辑器格式工具条**：现状 = Milkdown 已挂载但无 UI 工具条，用户只能靠键盘/鼠标选区操作 → 目标 = 顶部 28px 按钮组（加粗/斜体/标题/链接/代码/引用 + 分隔 + 撤销/重做），调用 Milkdown commands → 涉及文件 = `src/features/wiki/WikiEditor.tsx:87-142`、可能需要 `@milkdown/kit/preset/commonmark` 的 `toggleStrong` / `toggleEmphasis` / `wrapInBlockquote` 等 command → 验收 = 点击工具条按钮能对选区生效；禁用态正确；按钮样式匹配 `.editor__toolbar`。
- [ ] **HTML 结果预览态 + 统一生成入口（设计稿第三态的收敛方案）**：现状 = 段控只有 read/edit 两档；无“生成内容”入口；无 iframe 结果预览 → 目标 = “生成内容”按钮携带当前 Wiki 页进入统一 Workflows Generate Content preparation，由其选择内建类型、范围与本次执行路径；Wiki 不新建 `GenerateHtmlDialog`，也不直接调用 Skill / compile command。任务完成后可进入 Exports 结果页或在 Wiki 的 preview 态读取该 Exports record，提供“重新生成 / 打开位置 / 外部浏览器 / 复制路径” → 涉及文件 = `src/features/wiki/WikiView.tsx`、可选 `HtmlPreviewPane.tsx`、Workflows 导航参数与 Exports record API → 验收 = trusted writable 项目从当前文章进入同一准备模型，用户再次明确开始后生成；输出落在 layout-defined export root（原生为 `exports/html/`），结果仍由 Exports 管理。
- [ ] **PRD-READ-005 相关页面反链计数与"查看全部"**：现状 = 反链列表无每页反链次数、无"查看全部"链接 → 目标 = 每条 `relpage` 右侧显示 mono 计数；列表底部显示"查看全部 N 个反链 →" → 涉及文件 = `src/features/wiki/RelatedPagesPanel.tsx:95-119` → 验收 = 计数正确；点击全部打开图谱或反链总览。
- [ ] **引用编号化（citation 列表）**：现状 = sources 纯文字列表 → 目标 = `citation__idx` 圆形编号 + 截断标题，且与正文 `citation-ref` 角标对应 → 涉及文件 = `src/features/wiki/RelatedPagesPanel.tsx:121-134`、`src/features/wiki/MarkdownReader.tsx`（正文需识别 `[^1]` 或 `[1]` 角标 → 渲染 `citation-ref` 圆形上标） → 验收 = 编号样式与设计稿一致；点击角标滚动到 citation。
- [ ] **PRD-WIKI-004 编译冲突 Markdown Diff 对话框**：现状 = 仅编辑器内 banner + reload 按钮 → 目标 = 三路 diff 视图（baseline / current 外部修改 / agent ingest）+ 三选项（保留当前 / 使用 Agent / 手动合并）；与 `src/components/app/ConfirmationDialog.tsx` 协同 → 涉及文件 = 新建 `src/features/wiki/ConflictDiffDialog.tsx`；后端需返回 baseline 与三路文本（目前 `FILE_HASH_MISMATCH` 只给错误码） → 验收 = 用户能看到 diff、能选择合并策略。
- [ ] **PRD-LINT-001 死链/孤立/缺 frontmatter 在阅读视图可视化**（跨板块）：现状 = wiki 阅读视图对 lint 问题无视觉提示 → 目标 = missing wikilink 已标红，但孤立页面/缺 frontmatter/重复文件名等无标记 → 涉及文件 = `src/features/wiki/MarkdownReader.tsx` + `src/features/lint/` → 验收 = lint 结果能在阅读视图用边距图标提示。
- [ ] **右键/更多菜单（复制路径 / 在图谱中查看 / 复制 wikilink / 星标）**：现状 = 仅有星标按钮 → 目标 = 中间顶栏右侧"更多"下拉 + 右侧"操作"区 4 个按钮 → 涉及文件 = `src/features/wiki/WikiView.tsx:90-144`、`src/features/wiki/RelatedPagesPanel.tsx` → 验收 = 复制路径能写入剪贴板；"图谱中查看"切换到 graph 板块并选中节点。

## 3. 视觉 / 设计 token 落差

- **frontmatter 卡片**：`src/features/wiki/MarkdownReader.tsx:60-61` 使用 `<pre className="wiki-frontmatter">`，设计稿要求 `.frontmatter`（surface 底 + border-subtle + radius-md + sp-3/sp-4 padding + mono 11.5px + 120px grid row）。styles.css 中也没有补 `.wiki-frontmatter` 样式 → 与设计 token 完全脱节。
- **wikilink pill**：设计稿 `.prose .wikilink` = accent-soft 底 + accent-border + radius-sm + 0.92em；当前 ReactMarkdown 输出 `<a class="wikilink">` 但 styles.css 中没有对应规则（仅在 `app.css` 里定义，未迁移到应用 `src/styles.css`）。实际渲染会退化为普通链接颜色。
- **`.prose` 排版**：设计稿有完整 `.prose` 段落 / 标题 / blockquote / table / hr 样式（`app.css:1100-1168`）；应用 styles.css 中没看到等价的 `.wiki-prose` 全套规则，阅读区排版粗细/间距/字号会与设计漂移。需要在 `src/styles.css` 中补 `.wiki-prose` 对应 `.prose` 的迁移版本。
- **顶栏段控**：设计稿段控按钮图标 + 文字两档高度未明确但同 `seg`，当前实现 `ModeButton` 26px，与设计稿其它段控一致；但缺第三档 preview。
- **编辑器工具条**：设计稿 `.editor__toolbar` 28x28 按钮 + 18px 分隔条；完全没实现。
- **citation-ref 角标**：设计稿 16px 圆形 accent-soft + 9.5px mono + 上标；完全没实现。
- **rightpanel meta dt/dd**：设计稿用两列 grid；当前实现 `grid-cols-[auto_1fr] gap-x-3`，视觉接近，但缺"引用 3 / 反链 12"两条独立行。
- **HTML 预览 chrome（红黄绿圆点 + mono 路径）**：完全缺失。
- **模板卡片 `.tmpl-card` 网格**（96px thumb + accent-border 选中态）：完全缺失。

## 4. 交互 / 可访问性落差

- **键盘导航**：当前 `Cmd/Ctrl+S` 已接入保存（`WikiEditor.tsx:150-155`），但缺 `Cmd/Ctrl+E` 切换 edit/read、缺 `Cmd/Ctrl+P` 切换 preview、缺 `Cmd/Ctrl+N` 新建页面、缺 `Esc` 取消编辑回到阅读。
- **focus-visible**：设计稿 `.tree__row:focus-visible`、`.relpage:focus-visible` 都有 outline 样式（`app.css:1960-1965`）；应用实现中 `WikiTree` 的行用 `<button>` 自带 focus 环，但 `RelatedPagesPanel` 的 relpage 也是 button，缺少与设计 token 一致的 focus 样式。
- **aria 角色**：阅读区 `<article className="wiki-prose">` 没有 `role="article"` / `aria-label="页面标题"`；段控 `ModeButton` 没有 `role="tab"` / `aria-selected`；类型 pill 没有单选/多选语义。
- **missing wikilink 无障碍**：`.wikilink--missing` 仅靠颜色区分，缺 `aria-label` 或 `title` 暗示"此页面不存在"（目前 title 是 "链接页面不存在" 但只在 missing 时给）。
- **冲突可访问性**：banner 用 `var(--warning)` 纯色，缺 `role="alert"`；未来 diff 对话框需 `role="dialog" aria-modal`。
- **iframe 安全**：未来 HTML 预览 iframe 需 `sandbox` 属性限制脚本（设计稿直接 `srcdoc`，需在实现时加 sandbox="allow-same-origin"）。
- **loading 态**：文件树空态、页面 loading 已有 LoaderCircle；但 frontmatter 解析失败、YAML 格式错误时无降级 UI 提示。
- **右键菜单**：设计稿未直接画右键菜单，但 PRD 的"重命名/删除"需要一个入口；建议右键菜单 + 顶栏"+"双路径。
- **i18n**：`wiki.*` key 覆盖良好，但未来 "生成 HTML"、"模板名"、"预览"等 key 需补全。

## 5. 建议实施顺序

1. **P0 - 布局与访问策略**：先让扫描、读取与所有 mutation 消费后端 page roots/access/capabilities，覆盖兼容、受限、只读与恢复模式。
2. **P0 - frontmatter 卡片化 + wikilink 样式落地**（`MarkdownReader.tsx` + `styles.css`）：同时把 `.prose` 全套排版迁到 `.wiki-prose`。
3. **P0 - Milkdown 编辑器格式工具条**（`WikiEditor.tsx`）：加粗/斜体/标题/链接/代码/引用/撤销/重做。
4. **P0 - 新建/重命名/删除页面前端 UI 接线**：只在后端确认 trusted writable 后开放；删除走 `ConfirmationDialog`，后端负责 Git checkpoint。
5. **P0 - 外部修改冲突 Diff 对话框**（`ConflictDiffDialog.tsx` + 后端扩 `FILE_HASH_MISMATCH` 返回 baseline/agent 文本）。
6. **P1 - HTML 结果预览态 + 共享生成入口**：Wiki 只拥有结果预览与携带当前页进入 Workflows 的入口；模板/制品类型与执行路径只在共享 Generate Content preparation 中选择。外部 Skill/Agent 路径受 trust gate，写入 Exports 受 writable/Git 策略。
7. **P1 - 右侧“操作”区 + citation 编号化 + 反链计数**。
8. **P2 - 编辑历史时间线 + 右键菜单 + 键盘快捷键体系**。
9. **P2 - 阅读视图 lint 可视化叠加**。

## 6. 2026-06-21 wiki-fe loop 后端缺口

- **P1 HTML 预览“外部浏览器打开” blocked**：当前已交付导出命令仅有 `start_export` / `regenerate_export` / `list_exports` / `read_export_preview` / `open_export_folder`，没有安全打开生成 HTML 的外部浏览器命令；本前端 loop 不新增后端命令，故跳过该按钮，其余 preview 第三态不受影响。
