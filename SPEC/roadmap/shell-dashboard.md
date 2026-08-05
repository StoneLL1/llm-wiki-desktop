# 应用壳 + 无项目工作台 + Dashboard 路线图

> 当前有效路线图。首次使用与“打开已有知识库”的产品、流程和安全语义，以 [`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) 为唯一权威。
>
> Workflows 的命名、入口和任务体验，以 [`../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) 为准。
>
> `UI-Frontend-design/dashboard.html` 与 `assets/app.css` 继续约束壳层结构、组件密度和视觉 token；其中旧启动页、Agent 主入口、三张快速操作卡等内容不再是产品行为依据。

## 0. 统一目标

应用从启动到日常使用始终处于同一套桌面工作台中：

- 没有打开知识库时，仍渲染完整的 TopBar、左侧栏、中央工作区、右侧上下文面板和底部状态栏。
- 无项目首屏只有两张主卡：`新建知识库`、`打开已有知识库`。
- 不展示最近项目画廊、第三个“导入资料”动作、模板墙、Agent/BYOK 检测卡或产品介绍 Hero。
- 新建知识库完成后进入 Import，引导用户获得首个可阅读 Source；不直接进入 Dashboard。
- 打开已有知识库先只读评估，再根据分类进入原生、兼容、受限、只读或恢复模式；普通资料文件夹不会被原地初始化。
- 原生、兼容与恢复模式共用 Dashboard 和应用壳，只通过状态、能力和下一步动作表达差异。
- 用户的首次价值是“提交了一个可阅读 Source”，不是首次编译、首张图谱或首次 Chat。

## 1. 当前实现事实

截至 2026-07-30，以下是迁移起点，不是目标行为：

- `src/app/App.tsx` 仍在 `AppShell` 与独立 `ProjectStartView` 之间二选一。
- `ProjectStartView` 仍带有启动页式 Hero、三个动作、最近项目网格和 Agent/BYOK/模板信息。
- 后端仍以 `is_wiki_project` 做二元判断，部分打开路径可能原地初始化普通文件夹并创建 Git 提交。
- `ProjectRegistry` 的路径登记不等同于用户信任；设置页仍保留多种启动策略和默认模板设置。
- Dashboard 已有基础状态与任务摘要，但还没有统一表达兼容、受限、只读、恢复和部分扫描状态。

实现时必须把这些事实作为需要迁移的旧行为，不能反向写回产品规范。

## 2. P0：无项目工作台

| 能力 | 目标 | 验收 |
|---|---|---|
| 持久壳层 | `AppShell` 始终存在，无项目状态由 Workspace 内容表达 | 清空当前项目不会跳到独立启动页；壳层尺寸、导航分组和状态栏不闪变 |
| 两张主卡 | 仅显示新建与打开已有知识库 | 键盘可达；中英文不截断；无第三张操作卡、最近项目网格或 AI 配置 |
| 无项目导航 | 项目型页面保留位置但呈现不可用原因或引导 | 不伪造项目上下文，不创建项目任务，不把用户送到空 Graph/Wiki/Chat |
| 右侧上下文 | 解释当前无项目状态和两条入口 | 不显示虚构的项目路径、索引、Git、Agent 或任务数据 |
| 顶栏项目入口 | 项目切换器可打开最近知识库/选择其他知识库 | “返回总览”回到当前项目 Dashboard，不回到独立启动页 |

## 3. P0：新建知识库

新建对话框包含：

- 名称；
- 父目录，首次默认 `Documents/LLM Wiki`，之后记住上次成功使用的父目录；
- 创建时模板：通用、研究、阅读、个人成长、商业，默认通用；
- 最终生成的子目录路径预览。

约束：

- 模板只在创建时选择，创建后不提供模板切换；
- 目标子目录非空时阻止创建并解释原因；
- 支持 CJK、Unicode 和 Windows/macOS/Linux 路径；
- 完成后进入 Import 页面，不自动弹系统文件选择器；
- 创建动作、目录写入、Git 初始化和失败回滚均由后端服务处理。

## 4. P0：打开已有知识库

打开流程必须是“选择目录 → 零写入快速评估 → 分类结果 → 直接打开或按需确认”。健康原生项目、健康且已信任的兼容项目，以及权威规范允许直接进入兼容 Dashboard 的 legacy / `nashsu` 项目不展示多余确认；只有信任、兼容启用、修复、歧义意图或写入动作需要用户决定。

### 4.1 分类

- 当前版本原生知识库；
- 旧版本原生知识库；
- nashsu/llm-wiki 结构；
- Obsidian vault；
- 通用 Markdown vault；
- 结构含糊；
- 普通资料文件夹；
- unknown。

health 独立返回 `healthy / repairable / recovery / unreadable`；损坏或无法安全读取不是 format 分类。

### 4.2 独立授权维度与派生能力

- `trust: trusted | untrusted` 与 `filesystemAccess: writable | read_only` 独立；trusted read-only 与 untrusted read-only 都必须可表达。
- `health: healthy | repairable | recovery | unreadable` 独立于 format 与 trust；Recovery Dashboard 不伪装成健康项目。
- `restricted` 只是 untrusted 与当前 capabilities 派生出的 UI 摘要，不是后端授权 enum。它允许本地阅读、搜索、内存图、只读盘点与 Local Quick，禁止外部 AI/Agent/Skill 和项目写入。
- trusted read-only 可在明确数据边界下运行不需要项目写入的能力，包括 ephemeral Chat 与具备路线时的 Complete Check；结果标注 non-persistent。
- 只有 trusted + writable 且 layout/Git capability 满足时，才能执行对应兼容、修复或业务写入。

信任按规范化后的目录身份保存在全局应用状态中，不写入用户知识库。快速评估启动返回 `assessmentOperationId`，完成后返回短期 `assessmentId`；确认信任、启用兼容或修复时，后端必须重新校验目录身份与评估版本。

### 4.3 兼容层与修复

- 兼容元数据只写入 `.app/compat/`，至少包括 `purpose.md` 和 `schema.md`；
- 不创建 `.app/project.json`，不覆盖根目录同名文件；
- 仅在用户确认启用兼容时才考虑 Git；没有 Git 时可默认初始化，已有脏仓库不自动清理、暂存、提交或 stash；
- 旧版本迁移、含糊结构修复和损坏恢复都必须先展示计划、受影响路径、Git 检查点和可撤销性，再由用户确认；
- 大型仓库的深度扫描作为可取消后台任务运行，快速阶段允许先以部分结果进入工作台；
- 根目录符号链接先规范化身份；仓库内部链接必须保持在根目录内，外部链接不跟随；大小写和 Unicode 冲突只报告，不自动重命名。

普通资料文件夹永不原地初始化、移动或改写。用户确认后，在独立位置新建知识库并通过 Import 复制资料，原目录保持不变。

## 5. P0：确定性启动

- 最新历史条目有效时，自动打开该知识库并进入 Dashboard；无历史时展示无项目工作台；最新路径缺失或不可访问时展示同一工作台与路径错误，不静默打开更旧项目。
- 历史为空、目录失效、权限变化或评估不能安全恢复时，进入无项目工作台。
- 不再提供“总是显示启动页 / 打开最近项目 / 记住上次页面”等启动策略。
- 创建模板与上次父目录是不同概念：模板在创建对话框默认通用；父目录记住上次成功选择。

## 6. P1：Dashboard 统一状态

Dashboard 保持紧凑工作台而非营销卡片墙，优先补齐：

1. 项目状态行：布局类型、信任、读写、兼容/恢复、索引与 Git 能力，以及最重要的下一步动作。
2. 统计摘要：Source、Wiki 页面、链接、图谱节点/边、待处理问题，缺数据时显示原因而不是零值幻觉。
3. 最近活动：导入、编辑、Workflows、Lint、Git 检查点、导出与恢复事件。
4. 快速操作：Import、更新 Wiki、Health Check、Chat/Export；受限状态下替换为“信任知识库”或只读操作。
5. 图谱预览：只有存在可读 Markdown 时渲染；受限模式只用内存索引且不落盘；深度扫描未完成时显示“部分结果”。

原生、兼容与恢复模式使用相同的信息结构。恢复模式把修复计划与诊断置顶，但不创建独立产品壳。

## 7. P1/P2：壳层与可访问性收口

- TopBar 项目切换器显示当前项目、路径摘要和切换菜单；无项目时显示“未打开知识库”。
- 左侧栏使用“知识处理 / Workflows”命名，Workflows 使用 Lucide `Workflow` 且无 badge；底部 Agent 状态脚保持技术状态，不承担导航。
- 右侧面板和底部状态栏只显示后端提供的真实能力与状态。
- 补齐 skip link、`focus-visible`、焦点陷阱、屏幕阅读器状态文本、`prefers-reduced-motion` 和 200% 缩放。
- 保持设计基线：TopBar 48px、主区头 52px、状态栏 28px、导航 30px、13px UI 正文、稀疏 teal 强调、无装饰渐变。

## 8. 建议实施顺序

1. 定义 `ProjectOpenAssessment`、项目访问策略、全局信任/最近路径/上次父目录和持久化契约。
2. 把无项目状态迁入 `AppShell`，落实两张主卡与新建对话框。
3. 实现快速评估、分类结果、受限进入、信任确认和启动恢复。
4. 实现兼容启用、Git 策略、修复确认、深度扫描与恢复模式。
5. 统一 Dashboard 的状态 DTO、首要动作、活动流和部分结果语义。
6. 删除旧 `ProjectStartView` 分支、第三动作、启动页 AI/模板墙和旧启动策略。
7. 完成视觉、国际化、路径边界、无障碍和跨平台回归。

## 9. 完成标准

- 首屏始终像已进入桌面工作台，且只有新建/打开两条主路径。
- 新建后准确进入 Import，并能在不配置 AI 的情况下获得首个可阅读 Source。
- 任意外部目录在用户确认前零写入；普通资料原目录始终不变。
- 原生、旧版本、nashsu、Obsidian、Markdown vault、含糊、普通资料和损坏结构都有确定状态与下一步。
- 信任、兼容、修复、Git、符号链接、外部链接、大小写和 Unicode 冲突都有后端校验和覆盖测试。
- 空 Wiki、空 Graph 或上下文不足的 Chat 都解释依赖并给出下一步，不让用户误以为产品失败。
