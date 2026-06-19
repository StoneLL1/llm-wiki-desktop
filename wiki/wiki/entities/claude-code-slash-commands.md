---
title: Claude Code Slash Commands
created: 2026-05-20
updated: 2026-05-22
type: entity
tags: [tool, tutorial]
sources:
  - raw/articles/2026-05-19-claude-code-slash-commands-guide.md
  - raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
  - raw/articles/2026-04-18-claude-code-hidden-commands.md
  - raw/articles/2026-04-18-claude-code-session-management.md
---

# Claude Code Slash Commands

## Overview

[[claude-code]] 内置 50+ 斜杠命令，覆盖会话管理、上下文控制、并行协作等。可直接用中文描述需求，Claude 自动匹配对应命令。完整命令列表可通过 `/help` 或输入 `/` 查看。

## 会话管理

|| 命令 | 用途 |
|------|------|
| `/clear` | 清空当前对话，开始新会话（旧对话可通过 `/resume` 找回） |
| `/compact [指令]` | 压缩上下文，释放空间；可附加指令指定保留重点（建议上下文用量 >80% 时主动执行） |
| `/resume [名称/ID]` | 恢复历史会话，Ctrl+A 可切换查看所有项目的历史 |
| `/branch` / `/fork` | 从当前对话分支出新会话，原对话不受影响 |
| `/rewind` | 回退到之前的检查点（对话 + 文件改动一起回退） |
| `/recap` | 生成当前会话一句话摘要 |
| `/btw` | 快速插入不相关问题，不污染主上下文 |
| `/copy [N]` | 复制最近第 N 条回复到剪贴板 |
| `/export [文件名]` | 导出对话为纯文本 |
| `/exit` | 退出会话 |

## 信息与诊断

|| 命令 | 用途 |
|------|------|
| `/usage` | 查看当前会话 token 用量和费用 |
| `/context [all]` | 可视化上下文窗口使用情况，配合 `/compact` 使用 |
| `/diff` | 交互式 diff 查看器，查看 Claude 的代码改动 |
| `/status` | 显示版本号、模型、账户和连接状态（不阻塞当前回复） |
| `/insights` | 生成使用分析报告（常用项目、交互模式、问题点） |
| `/help` | 显示所有可用命令列表 |

## 模型和模式控制

|| 命令 | 用途 |
|------|------|
| `/plan [任务描述]` | 进入规划模式，先制定方案再执行 |
| `/goal [完成条件]` | 设定自动执行目标，Claude 持续工作直到条件满足（建议加熔断限制，如 "20 轮后停止"） |
| `/model` | 切换 AI 模型 |
| `/effort` | 调整思考力度（low→max 五档，推荐 high/xhigh） |
| `/fast` | 快速模式，约 2.5x 速度，质量不变但单价更贵 |

## 配置与扩展

|| 命令 | 用途 |
|------|------|
| `/config` | 打开设置界面（主题、模型、输出风格等） |
| `/mcp` | 管理 [[mcp]] 服务器连接 |
| `/skills` | 管理 [[skills]] 技能列表 |
| `/plugin` | 管理插件（多 skill + 主题 + hook 的打包集合） |

## 代码审查

|| 命令 | 用途 |
|------|------|
| `/review [PR号]` | AI 审查 PR，启动多子代理并行检查 Bug 和逻辑错误 |
| `/ultrareview` | 云端多代理深度审查（额外消耗 usage credits） |
| `/simplify [方向]` | 审查最近修改，从复用/质量/效率三维度直接修复代码 |

## 子代理和并行

|| 命令 | 用途 |
|------|------|
| `/agents` | 查看和配置子代理 |
| `/tasks` | 查看正在运行的后台子代理任务 |
| `/background [指令]` | 将会话放到后台运行，释放终端（用 `claude agents` 监控） |
| `/loop [间隔] [任务]` | 定时循环执行任务（如 `/loop 5m 检查部署状态`），默认读取 `.claude/loop.md` |

## 远程访问与多端

|| 命令 | 用途 ||
||------|------|
|| `/teleport` | 把云端会话拉到本地继续（或用 `claude --teleport` 启动） ||
|| `/remote-control` | 手机远程操控本地正在跑的 Claude Code ||
|| `/voice` | 在 CLI 启动语音输入，按住空格键说话（暂不支持中文） ||

## 交互增强

|| 命令 | 用途 ||
||------|------|
|| `/rename` | 给当前会话起有意义的名字，方便后续搜索和恢复 ||
|| `/btw` | 在 Claude 执行任务时插一个不相关问题，复用 prompt cache，几乎零 token 消耗 ||
|| `/powerup` | Claude Code 版「多邻国」，10 关交互课程教你核心技巧 ||
|| `/insight` | 生成过去一个月的使用习惯 HTML 报告，推荐自定义命令和 Skill ||
|| `Ctrl+R` | 搜索历史会话（类似 shell 反向搜索） ||
|| `Ctrl+G` | 打开 vi 编辑器写 prompt，适合长指令和语音输入改错 ||
|| 双击 Esc | 打开回退菜单，选择只回退代码还是连对话一起回退 ||

## 并行与批量

|| 命令 | 用途 ||
||------|------|
|| `claude -w` | 在 git worktree 里启动新会话，多 Claude 并行互不干扰 ||
|| `/batch` | 自动拆分任务分发到多个 worktree Agent 并行执行（适合大型迁移） ||
|| `--bare` | 非交互调用（`-p` 或 SDK）时跳过配置加载，启动提速 10 倍 ||
|| `--agent <名字>` | 启动自定义 Agent（定义在 `.claude/agents/` 目录） ||

## 自动化与 Hook

|| 命令 | 用途 ||
||------|------|
|| `/loop [间隔] [任务]` | 定时循环执行，7 天后自动过期（如 `/loop 5m /babysit`） ||
|| `/schedule` | 云端定时任务，不取消就持续运行 ||
|| Hooks | 在 Agent 生命周期插入自定义逻辑（SessionStart、PreToolUse 等），可在 `.claude/hooks/` 配置 ||

## 会话管理决策（Thariq 框架）

参见 [[claude-code-session-management]] 中的「五条岔路」决策框架：
- 同一任务 context 健康 → 继续对话
- Claude 走错路 → Rewind (双击 Esc)
- context 被调试塞满 → `/compact` + 方向提示
- 全新任务 → `/clear` + 手写简报
- 大量中间输出 → 子 Agent

## 使用建议

- `/compact` 主动使用优于等自动触发，建议配合 `/context` 在用量 >80% 时执行
- `/goal` 适合有明确终止条件的批量任务（模块迁移、批量重构、issue 清理），务必加熔断限制
- `/background` 适合长时间任务，释放终端去做其他事
- `/review` + `/simplify` 组合用于提交前的代码质量保障
- 第三方工具 CC Switch 提供可视化的模型切换和参数管理界面

## See Also

- [[claude-code]] — Claude Code 主页面
- [[context-engineering]] — 上下文管理方法论
- [[claude-md]] — 项目级配置文件
- [[mcp]] — 外部工具连接协议
- [[skills]] — Skill 技能体系
