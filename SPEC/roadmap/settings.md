# Settings 板块落差与实施计划

> 对照源：UI-Frontend-design/settings.html + assets/app.css + SPEC/PRD.md §9.11
> 当前实现：src/features/settings/、src/stores/settingsStore.ts、src-tauri/src/services/settings_service.rs、src-tauri/src/services/secret_service.rs

## 0. 现状摘要

Settings 板块已搭起骨架：左侧分页导航（通用/外观/语言/Agent/LLM Providers/安全/后台任务/更新）+ 右侧内容区，i18n、主题切换、密钥走系统钥匙串（keyring crate，windows-native/apple-native/sync-secret-service）、Agent 检测与默认绑定、BYOK 五个 Provider（OpenAI/Anthropic/Google/Ollama/Custom）的保存/测试/删除均已打通，密钥状态只回 `"configured"` 字符串不泄露明文（`settings_service.rs:101-109`、`llm_commands.rs:68-76`）。硬边界（API Key 只进钥匙串）已落实，`settings.json` 测试断言不含 `sk-` 前缀（`settings_service.rs:218`）。

但完成度约 **55%**，距设计稿差距集中三处：
1. **配置项覆盖严重不足**——通用（启动行为/默认项目位置/模板/外部编辑器/文件关联）、上下文窗口（温度/最大 tokens/滑块）、Agent（超时/安装行为/Skill 自动加载）、后台任务（系统通知/通知点击行为/并发上限）、更新（频率选择/下载安装确认/变更说明）、安全（Git 检查点开关/人工编辑保护/Raw Sources 不可变/沙箱说明）、关于——这些在设计稿中是独立 formrow，当前要么缺失、要么字段类型简化（例如 closeBehavior 只有 2 选项 vs 设计稿 3 选项，contextWindow 是下拉 vs 设计稿滑块 + 温度/max tokens）。
2. **视觉/交互落差大**——设计稿的 `formrow`（label/hint/control 三列网格）、`seg` 分段控件、`apikey-row`（图标+名称+key 末 4 位+"已配置"徽章+测试/编辑按钮）、toggle 开关、range 滑块、checkbox 列表基本都没实现；当前是 Tailwind 手搓的卡片/按钮，与设计 HTML 的密度和层级差距明显。设计稿的"配置概览"右面板（当前配置/配置文件路径/检查清单/快捷操作）完全缺失。
3. **PRD SET-005 更新流程未落地**——UpdateSettings 只是 mock，`latestVersion` 恒为 null，"立即检查"只读当前版本不真正检查更新源，下载安装是 `window.confirm` 假弹窗。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 设置整体布局（左 nav + 右 content + header "已自动保存" + "打开配置文件" 按钮） | settings.html:78-110，settings-side 分组（应用/AI/系统）+ is-active 高亮 + header badge | `SettingsView.tsx:127-244` 有左 nav + 右滚动区，但 nav 是扁平 8 项无分组、无图标、无 header badge/打开配置文件按钮 | 🟡部分实现 | P1 | `src/features/settings/SettingsView.tsx:128-164` |
| 分组导航（应用/AI/系统三组，每项带 Lucide 图标） | settings.html:89-108 三 `.settings-side__group` | nav 是单组扁平列表，无分组标题、无图标 | ❌缺失 | P1 | `src/features/settings/SettingsView.tsx:134-149` |
| **通用**（启动行为 select / 默认项目位置 input+选择按钮 / 默认项目模板 select / 外部编辑器 input / 文件关联 checkbox） | settings.html:113-178 五个 formrow | "通用"页只显示 projectRoot 路径卡 + scopeCopy 文本卡，无任何可编辑控件 | ❌缺失 | P1 | `src/features/settings/SettingsView.tsx:167-184` |
| **外观**（主题 seg 三段 / 界面密度 seg 三段 / UI/阅读/代码三类字体 select） | settings.html:181-248 | 只实现主题选择，且是 3 卡片预览而非设计稿 seg 控件；密度、字体选择完全缺失 | 🟡部分实现 | P1 | `src/features/settings/AppearanceSettings.tsx:20-62` |
| **语言**（界面语言 seg / Agent 输出语言 select 四选） | settings.html:251-282 | 只实现界面语言 2 卡片；"Agent 输出语言"下拉完全缺失 | 🟡部分实现 | P1 | `src/features/settings/LanguageSettings.tsx:11-41` |
| **Agent**（默认 Agent select / 任务超时 input 秒 / 安装行为 3 checkbox / Skill 自动加载 toggle + 计数提示） | settings.html:285-338 | `AgentSettings.tsx` 只渲染检测到的 agent 列表 + 设为默认/清除按钮；超时、安装行为、Skill 自动加载全部缺失 | 🟡部分实现 | P0 | `src/features/settings/AgentSettings.tsx:11-66` |
| **LLM Providers · BYOK**（5 个 `apikey-row`：图标+名称+模型·末 4 位·钥匙串+徽章+测试/编辑；底部提示条） | settings.html:341-404 | `LlmProviderSettings.tsx` 有 5 Provider 选择 + model/baseUrl/key 输入 + 保存/测试/删除，但布局是手搓按钮列表非 `apikey-row`；无末 4 位回显、无"未配置/服务未运行"状态徽章、无底部安全提示条 | 🟡部分实现 | P0 | `src/features/settings/LlmProviderSettings.tsx:22-86` |
| **上下文窗口**（最大 ctx range 4K-1M + K 标识 / 回答最大 tokens / 温度 range） | settings.html:406-452 独立 section | 完全缺失；contextWindow 被挪到"后台任务"页且实现为 8 档下拉（`BackgroundTaskSettings.tsx:11`），与设计稿独立 section + range 滑块 + 温度/max tokens 均不符 | ❌缺失 | P1 | `src/features/settings/BackgroundTaskSettings.tsx:50-66` |
| **安全**（自动 Git 检查点 toggle / 人工编辑保护 toggle / Raw Sources 不可变 toggle / API Key 存储说明 mono 卡 / 沙箱说明） | settings.html:454-515 五个 formrow | `SecuritySettings.tsx` 只列出 5 Provider 的密钥状态 + 清除按钮，与设计稿"安全"section 完全错位（设计稿不列 Provider，列的是 Git/编辑保护/Raw/钥匙串说明/沙箱） | ❌缺失 | P0 | `src/features/settings/SecuritySettings.tsx:14-49` |
| **后台任务**（关闭窗口 seg 三选 / 系统通知 4 checkbox / 通知点击行为 select / 并发任务上限 input） | settings.html:517-575 | closeBehavior 只 2 选项（缺"询问"）、contextWindow 错误并入此页；系统通知、通知点击行为、并发上限完全缺失 | 🟡部分实现 | P1 | `src/features/settings/BackgroundTaskSettings.tsx:13-69` |
| **更新**（当前版本+最新+检查时间+立即检查 / 自动检查频率 select / 下载安装 checkbox） | settings.html:577-617 | `UpdateSettings.tsx` 有版本显示 + 自动检查开关 + 检查/下载按钮，但 `checkNow` 是 mock（不真正查更新源）、下载用 `window.confirm` 假弹窗；自动检查频率 select、变更说明 checkbox 缺失 | 🟡部分实现 | P1（PRD-SET-005） | `src/features/settings/UpdateSettings.tsx:18-101` |
| **关于**（logo+名称+版本+描述+文档/GitHub/反馈/许可证链接） | settings.html:619-641 独立 section | 完全缺失 | ❌缺失 | P2 | `src/features/settings/SettingsView.tsx`（无 about 分支） |
| 右侧"配置概览"面板（当前配置/配置文件路径/检查清单/快捷操作 4 段） | settings.html:648-707 | 完全缺失 | ❌缺失 | P2 | `src/features/settings/SettingsView.tsx:127`（grid 只有 2 列） |
| 顶栏"打开配置文件"按钮、自动保存 badge | settings.html:81-84 | header 只有 loading/saving 文本，无 badge 无按钮 | ❌缺失 | P2 | `src/features/settings/SettingsView.tsx:154-163` |
| 设置搜索框（⌘K "搜索设置项…"） | settings.html:37-41 | 缺失 | ❌缺失 | P2 | — |
| 密钥存储：系统钥匙串（硬边界） | CLAUDE.md / PRD-SET-002 | ✅ 走 keyring crate（`Cargo.toml:24` 三平台 native feature），`SecretService` memory 仅测试用；UI 只显 "configured" | ✅已完成 | P0 | `src-tauri/src/services/secret_service.rs:21-83`、`src-tauri/src/commands/llm_commands.rs:68-76` |
| 密钥末 4 位回显（设计稿 `sk-ant-···8f3a`） | settings.html:349 | `secret_mask` 字段存在但 `llm_commands.rs:22,39,56` 恒返 None；`SecretService::mask`（secret_service.rs:71-83）有实现但未被调用 | 🟡部分实现 | P1 | `src-tauri/src/commands/llm_commands.rs:22` |

## 2. 功能落差（PRD 对照）

- [ ] **PRD-SET-001 LLM Provider 配置完整化**：5 Provider 的配置已在后端落库，但前端 `apikey-row` 布局/末 4 位/状态徽章/编辑态切换未实现；Ollama "服务未运行/启动" 检测分支缺失。→ 目标：按设计稿重写 `LlmProviderSettings`，后端 `list_llm_providers` 调用 `SecretService::mask` 回填 `secret_mask`。→ `src/features/settings/LlmProviderSettings.tsx`、`src-tauri/src/commands/llm_commands.rs:19-26`。验收：每个 Provider 行显示模型名 + 末 4 位 + 状态徽章；Ollama 服务可用性实时反映。
- [ ] **PRD-SET-005 更新检查真正落地**：`UpdateSettings.checkNow` 只读 `get_app_summary`，`latestVersion` 恒 null，下载走 `window.confirm`。→ 目标：接入 Tauri updater 插件或自建 release 检查 endpoint；下载/安装必须用户二次确认（不能 `window.confirm`，要用应用内对话框）。→ `src/features/settings/UpdateSettings.tsx:25-56`、新增 `src-tauri/src/services/update_service.rs`。验收：能查到最新版；下载前弹应用内确认对话框；安装前展示变更说明。
- [ ] **Agent 配置补全（超时/安装行为/Skill）**：设计稿要求任务超时（秒）、安装行为三 checkbox（允许执行安装/仅显示命令复制/检测新 Agent 提示）、Skill 自动加载 toggle + 计数。→ 目标：扩展 `Settings` 类型 + 后端 `SettingsService` 持久化；`AgentSettings.tsx` 加 3 formrow。→ `src/types/settings.ts:9`、`src-tauri/src/models/settings.rs`、`src/features/settings/AgentSettings.tsx`。验收：配置变更落 `.app/settings.json` + 生效。
- [ ] **安全 section 内容对齐设计稿**：当前"安全"页错位为 Provider 密钥列表（应由 LLM Providers 页承担）。→ 目标：改为 Git 检查点 toggle / 人工编辑保护 toggle / Raw Sources 不可变 toggle / 钥匙串说明 mono 卡 / 沙箱说明。Provider 密钥清除移到 LLM Providers 页。→ `src/features/settings/SecuritySettings.tsx`、`src/types/settings.ts`（加 security toggles）。验收：5 个 formrow 全部可配并持久化。
- [ ] **上下文窗口独立 section**：contextWindow 错误并入"后台任务"，且设计稿还要求"回答最大 tokens"和"温度"。→ 目标：新建 `ContextWindowSettings.tsx`（range 滑块 4K-1M + max tokens input + 温度 range），从"后台任务"页移除。→ `src/features/settings/SettingsView.tsx:231-238`、新增 `ContextWindowSettings.tsx`。验收：三项可调并影响 Agent/BYOK 调用。
- [ ] **通用 section 补全**：启动行为/默认项目位置/默认项目模板/外部编辑器/文件关联 5 项。→ 目标：扩展 `Settings` + 后端持久化 + formrow。→ `src/features/settings/SettingsView.tsx:167-184`、`src-tauri/src/services/settings_service.rs`。验收：5 项可编辑并持久化（文件关联属 OS 级，MVP 可只存偏好）。
- [ ] **后台任务 section 补全**：closeBehavior 加"询问"第三项；加系统通知 4 checkbox / 通知点击行为 select / 并发任务上限 input。→ `src/features/settings/BackgroundTaskSettings.tsx`、`src/types/settings.ts:6`。验收：全部配置生效（系统通知需接 Tauri notification）。
- [ ] **外观/语言补齐**：外观加界面密度 seg + 三类字体 select；语言加"Agent 输出语言"四选。→ `AppearanceSettings.tsx`、`LanguageSettings.tsx`、`src/types/settings.ts`。验收：密度切换影响间距 token；字体切换落 `--font-*`。

## 3. 视觉 / 设计 token 落差

- **`formrow` 三列网格未实现**：设计稿 `.formrow`（`app.css:1635-1647`）是 label/hint 左列 + control 右列、底边框分隔的行式表单。当前全用 `grid gap-3` 卡片堆叠，密度过高、信息层级丢失。→ `src/features/settings/*.tsx` 全部。
- **`seg` 分段控件缺失**：主题/密度/界面语言/关闭窗口设计稿都用 `.seg`（按钮组 + is-active），当前主题和语言改成大卡片、closeBehavior 改成 2 卡片。→ `AppearanceSettings.tsx:30-58`、`LanguageSettings.tsx:21-37`、`BackgroundTaskSettings.tsx:30-47`。
- **`toggle` 开关缺失**：设计稿自动 Git 检查点、人工编辑保护、Raw Sources 不可变、Skill 自动加载都用 `.toggle` 滑块开关，当前实现里这些 formrow 都还不存在。→ 待新建 formrow 时一并引入 Toggle 组件。
- **`apikey-row` 卡片样式缺失**：设计稿（settings.html:12-26 inline + app.css）是 5 列 grid（图标+名称/提示+徽章+测试+编辑），当前是 3 列粗按钮。→ `LlmProviderSettings.tsx:62-68`。
- **Provider 字母图标缺失**：设计稿每个 Provider 行有 32×32 圆角图标（Anthropic 黑底 A、OpenAI O、Gemini G、Ollama cpu svg、Custom link svg），当前实现无图标。→ `LlmProviderSettings.tsx`。
- **状态徽章缺失**：设计稿 `badge--success`（已配置，带绿点）/ `badge--outline`（未配置）/ `badge--warn`（Ollama 服务未运行），当前只用文字"已配置/未配置"。→ 全板块。
- **左 nav 无图标无分组**：设计稿 `settings-side__title`（应用/AI/系统）+ 每项前 Lucide svg，当前是扁平文本按钮。→ `SettingsView.tsx:134-149`。
- **range 滑块样式未实现**：上下文窗口设计稿是 `input[type=range]` + 刻度标尺 + K/M 格式化，当前是 8 档 select。→ 待新建。
- **字号/间距**：当前已用 `text-[12px]/[13px]` 等绝对 px，基本对齐；但 section 标题用 `text-[16px]` 缺 `letter-spacing:-0.01em`；desc 缺 `--sp-4` 下间距。

## 4. 交互 / 可访问性落差

- **键盘导航**：左 nav button 可聚焦但无 `aria-current="page"`；设计稿 is-active 状态应映射到 `aria-current`。→ `SettingsView.tsx:135-148`。
- **自动保存反馈**：设计稿 header 有 `badge--success` "已自动保存" 持续可见反馈；当前只有 `saving/loading` 瞬时文本，无持久 badge。→ `SettingsView.tsx:159-162`。
- **密钥输入无障碍**：`LlmProviderSettings.tsx:75` 有 `aria-label="API key"` 但 `type="password"` + 无 "显示/隐藏" 切换；无 `required` + 错误关联 `aria-describedby`。→ `LlmProviderSettings.tsx:75`。
- **Toggle/checkbox 缺 label 关联**：待新增的 toggle/checkbox 须用 `<label>` 包裹或 `htmlFor`+`id`，避免点击区域丢失。→ 待新建 formrow。
- **`window.confirm` 反模式**：`UpdateSettings.tsx:50` 用浏览器原生 confirm，应改为应用内 `ConfirmationDialog`（与项目其他高风险确认一致，`CLAUDE.md` 要求高风险操作走 `PendingAction`）。→ `UpdateSettings.tsx:42-56`。
- **错误展示无 `role="alert"`**：`SettingsView.tsx:165` error div 缺 `role="alert"`，屏幕阅读器不播报。→ `SettingsView.tsx:165`。
- **Focus visible**：app.css:1969 有 `.settings-side__item:focus-visible` 样式，当前 button 缺等效 focus ring。→ `SettingsView.tsx:139-143`。

## 5. 建议实施顺序

1. **P0 硬边界复核（1h）**：确认生产构建不走 `SecretService::memory()`（仅测试用），`AppState` 初始化用默认 `SecretService`（走 keyring）。补一条集成测试断言 `.app/settings.json` 不含 key 明文。→ 已基本达成，补测试即可。
2. **P0 Provider 行 UX 对齐（0.5d）**：重写 `LlmProviderSettings` 为 `apikey-row` 栅格；后端 `list_llm_providers` 调 `SecretService::mask` 回填末 4 位；加状态徽章；把 `SecuritySettings` 里的密钥清除功能并入此页。
3. **P0 安全 section 对齐（0.5d）**：`SecuritySettings.tsx` 重写为 Git 检查点/编辑保护/Raw 不可变 toggle + 钥匙串 mono 说明 + 沙箱说明；扩展 `Settings` 类型 + 后端持久化。
4. **P0 Agent section 补齐（0.5d）**：超时 input + 安装行为 checkbox + Skill 自动加载 toggle；扩展 `Settings` 与 `SettingsService`。
5. **P1 上下文窗口独立 section（0.5d）**：新建 `ContextWindowSettings.tsx`（range + max tokens + 温度），从"后台任务"移除 contextWindow。
6. **P1 通用/后台任务/更新/外观/语言 字段补齐（1.5d）**：按设计稿 formrow 逐项补齐；UpdateSettings 接真实更新源 + 改应用内确认对话框。
7. **P1 视觉对齐（1d）**：引入 `formrow`/`seg`/`toggle`/`apikey-row`/badge 样式（可在 `src/styles.css` 或新增组件），左 nav 加分组 + Lucide 图标 + `aria-current`。
8. **P2 关于 section + 配置概览右面板 + 设置搜索（1d）**：收尾，对齐设计稿完整布局。
