---
title: Claude Code Hooks System
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, engineering, agent]
sources:
  - raw/articles/2026-04-18-claude-research-10x-better.md
---

# Claude Code Hooks System

## Overview

Claude Code Hooks 是一种确定性的行为控制机制，绑定在 [[claude-code]] 工作流的特定节点上。与 [[claude-md|CLAUDE.md]] 中的"软约束"（模型大多数时候会遵守但非绝对）不同，Hooks 一旦触发就会**确定性执行**——每一次都执行，没有例外。

Hooks 配置位于 `.claude/settings.json` 的 `hooks` 字段下。Claude Code 在会话开始时对配置做一次快照；当事件触发时，通过 stdin 接收 JSON 负载，根据退出码（exit code）决定后续行为。

## 退出码语义

| Exit Code | 含义 | 行为 |
|-----------|------|------|
| 0 | 成功 | 正常继续 |
| 1 | 报错 | 记录但不阻止执行 |
| 2 | 阻止 | 阻止执行，stderr 返回给 Claude 用于自我修正 |

常见错误：在安全类 hook 中使用 exit 1——这只会记录日志但不会阻止操作。安全拦截必须用 exit 2。

## 事件类型

| 事件 | 触发时机 | 典型用途 |
|------|----------|----------|
| PreToolUse | 工具执行前 | 安全关卡、拦截危险命令 |
| PostToolUse | 工具执行成功后 | 自动格式化、lint |
| Stop | Claude 完成任务时 | 质量检查（如"必须通过测试"） |
| UserPromptSubmit | 用户按下回车时 | 提示词校验 |
| Notification | 桌面通知 | 任务完成通知 |
| SessionStart | 会话开始 | 注入上下文 |
| SessionEnd | 会话结束 | 清理环境 |

对于工具类事件，可以用 `matcher`（正则）限制触发范围：`"Write|Edit|MultiEdit"` 针对文件修改，`"Bash"` 针对命令行操作。

## 配置示例

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "jq -r '.tool_input.file_path' | xargs npx prettier --write 2>/dev/null"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/bash-firewall.sh"
          }
        ]
      }
    ]
  }
}
```

## 注意事项

- Hooks 在会话中**不会热更新**——修改后需重新启动会话
- PostToolUse 无法"撤销"操作（已执行完毕），需要拦截请用 PreToolUse
- Hooks 对子 Agent 也会递归触发
- Hooks 以用户权限执行，没有沙箱保护——必须对 shell 变量加引号、校验 JSON 输入、使用绝对路径
- Stop hooks 需检查 `stop_hook_active` 标志以避免死循环

## 与 Harness Engineering 的关系

Hooks 是 [[harness-engineering]] 六大支柱中「架构约束」和「自验证循环」的具体实现。在 [[claude-code]] 源码的 512K 行代码中，query() 循环的 16 个步骤里，hooks 相关的验证逻辑占了 3 个步骤（后采样 Hooks、停止 Hooks、中断处理）。

## Relationships

- 确定性控制层，补充 [[claude-md]] 的"软约束"
- 配置在 `.claude/settings.json` 中，与 [[claude-code]] 权限系统协同工作
- 是 [[harness-engineering]] 架构约束支柱的具体实现
- 与 [[skills]] 的「按需钩子」模式互补

## See Also

- [[claude-code]] — Hooks 运行的平台
- [[claude-md]] — 项目级"软约束"配置
- [[harness-engineering]] — Hooks 的理论基础
- [[skills]] — 可复用的按需工作流
