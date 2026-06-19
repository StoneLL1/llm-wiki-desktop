---
title: "这个周末我研究了 .claude，终于知道为什么别人用 Claude 比我强10倍"
url: "https://mp.weixin.qq.com/s/IOpZYjEJrVQda27-Ci6C0g"
source: "微信公众号"
fetched: 2026-04-18
sha256: eb7ba25aea45ebb3
---

大多数 Claude Code 用户把 .claude 文件夹当作一个“黑盒”。他们知道它的存在，也见过它出现在项目根目录中，但从未打开过，更别说理解里面每个文件的作用。

这其实错失了一个很大的机会。

.claude 文件夹是控制 Claude 在你的项目中如何行为的核心中枢。它包含你的指令、自定义命令、权限规则，甚至还有 Claude 在不同会话之间的记忆。一旦你弄清楚每个内容的位置及其作用，你就可以将 Claude Code 配置成完全符合团队需求的样子。

本文将带你全面了解这个文件夹的结构，从你每天都会使用的文件，到那些只需设置一次就可以长期不用管的部分。

两个文件夹，而不是一个

在深入之前，有一点需要提前了解：实际上存在两个 .claude 目录，而不是一个。

第一个位于你的项目中，第二个位于你的用户主目录中：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/Cic4GFNZLlxqdZ2JnRNNxGAsErYE6K9akP0y4nV0GT7x5D6zhSZBIMan3tpfylPkzsqBZSFAj0UlfU8bgibHHvVNA3yRSwS0SuBgamDGxxkRM/640?wx_fmt=png&from=appmsg)

项目级文件夹用于存放团队配置。你会把它提交到 Git 中，这样团队里的每个人都会使用相同的规则、相同的自定义命令以及相同的权限策略。

而全局的 ~/.claude/ 文件夹则用于存放你的个人偏好和本机状态，比如会话历史和自动记忆等。

**CLAUDE.md：Claude 的说明手册**

这是整个系统中最重要的文件。当你启动一个 Claude Code 会话时，它首先读取的就是 CLAUDE.md。它会将其直接加载进系统提示（system prompt）中，并在整个对话过程中持续参考。

简单来说：你在 CLAUDE.md 里写什么，Claude 就会遵循什么。

如果你告诉 Claude 在实现功能之前总是先写测试，它就会这么做。如果你写“永远不要用 console.log 做错误处理，必须使用自定义的 logger 模块”，它每次都会遵守。

最常见的做法是在项目根目录放一个 CLAUDE.md。不过你也可以在 ~/.claude/CLAUDE.md 中放一个全局配置，让它在所有项目中生效，甚至还可以在子目录中放置专门针对该文件夹的规则。Claude 会读取所有这些文件并将它们合并使用。

**CLAUDE.md 中应该写什么**

很多人要么写得太多，要么写得太少。以下是推荐内容：

可以写：

- 构建、测试和 lint 命令（如 npm run test、make build 等）

- 关键架构决策（例如“我们使用基于 Turborepo 的 monorepo”）

- 不明显的注意事项（例如“启用了 TypeScript 严格模式，未使用变量会报错”）

- 导入规范、命名模式、错误处理风格

- 主要模块的文件和目录结构

不要写：

- 本应写在 linter 或 formatter 配置里的内容

- 已经可以通过链接获取的完整文档

- 大段理论性解释

将 CLAUDE.md 控制在 200 行以内。文件太长会占用过多上下文，反而降低 Claude 对指令的遵循效果。

下面是一个简洁但高效的示例：

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`# 项目：Acme API``
``## 命令``npm run dev          # 启动开发服务器``npm run test         # 运行测试（Jest）``npm run lint         # ESLint + Prettier 检查``npm run build        # 生产环境构建``
``## 架构``- 基于 Express 的 REST API，Node 20``- 使用 Prisma ORM 连接 PostgreSQL``- 所有处理器位于 src/handlers/``- 共享类型在 src/types/``
``## 约定``- 每个处理器都使用 zod 做请求校验``- 返回结构统一为 { data, error }``- 不向客户端暴露堆栈信息``- 使用 logger 模块，不使用 console.log``
``## 注意事项``- 测试使用真实本地数据库，不使用 mock。先运行 `npm run db:test:reset```- 严格的 TypeScript：绝不允许未使用的导入`
```

大约 20 行左右。这已经为 Claude 提供了在该代码库中高效工作的全部必要信息，而无需不断来回确认细节。

有时候你会有一些只属于个人的偏好，而不是整个团队的需求。比如你可能更喜欢使用不同的测试运行器，或者希望 Claude 始终按照某种特定方式打开文件。

这时可以在项目根目录创建一个 CLAUDE.local.md。Claude 会和主 CLAUDE.md 一起读取它，而且该文件会被自动加入 git 忽略（gitignored），因此你的个人调整不会被提交到仓库中。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/Cic4GFNZLlxraPe6dfn83mAf6s41SosTia4Aof4bESPEDhTWaf54sNaDC7icpSgYsjzaUCI461JZQe5JQU7icpCj8L9Uzribibg8Z74rpicAQovYlE/640?wx_fmt=png&from=appmsg)

**rules/ 文件夹：可扩展的模块化指令**

CLAUDE.md 在单个项目中表现很好。但随着团队规模扩大，很容易变成一个 300 行的 CLAUDE.md——没人维护，也没人愿意看。

rules/ 文件夹正是为了解决这个问题。

在 .claude/rules/ 目录下的每一个 Markdown 文件，都会自动和 CLAUDE.md 一起被加载。你不再需要一个庞大的单一文件，而是可以按关注点拆分指令：

- 

- 

- 

- 

- 

`.claude/rules/``├── code-style.md``├── testing.md``├── api-conventions.md``└── security.md`
```

每个文件都保持聚焦、易于更新。负责 API 规范的成员只需要维护 api-conventions.md，负责测试规范的人维护 testing.md，互不干扰。

真正强大的地方在于“按路径生效”的规则。你可以在规则文件中添加一个 YAML 前置块（frontmatter），让它只在 Claude 处理匹配路径的文件时才生效：

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`---``paths:``  - "src/api/**/*.ts"``  - "src/handlers/**/*.ts"``---``# API 设计规则``
``- 所有处理器返回 { data, error } 结构``- 使用 zod 进行请求体校验``- 永远不要向客户端暴露内部错误细节`
```

当 Claude 在编辑 React 组件时，它不会加载这个文件；只有在处理 src/api/ 或 src/handlers/ 下的文件时才会启用这些规则。没有 paths 字段的规则则是“全局生效”，每次会话都会加载。

当你的 CLAUDE.md 开始变得臃肿时，这才是正确的组织方式。

**hooks 系统：对 Claude 行为的确定性控制**

CLAUDE.md 中的指令很好用，但它们本质上只是“建议”。Claude 大多数时候会遵守，但并非每一次都绝对执行。

你不能指望一个语言模型始终：

- 自动运行 lint

- 永远避免执行危险命令

- 或在完成任务时稳定地通知你

hooks 可以让这些行为变得“确定”。

它们是绑定在 Claude 工作流程特定节点上的事件处理器。一旦触发，你的 shell 脚本就会被执行——每一次都执行，没有例外。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/Cic4GFNZLlxoGztolIqavYYATk2r6LrfKEdIvfMqiccBG6YwjcFb6SV6MibK3PArdcAb78yxC3FsRvSvZ96hskPFnS08UCSj6kolH3as0JQRWs/640?wx_fmt=jpeg&from=appmsg)

所有 hooks（钩子）的配置都放在 settings.json 中的 hooks 字段下。Claude Code 会在会话开始时对配置做一次快照；当某个事件触发时，它会通过 stdin 接收一个 JSON 负载，并根据退出码（exit code）来决定接下来怎么做。

**关键点一定要记住：**

- 
退出码 0：成功

- 
退出码 1：报错，但**不会阻止执行**

- 
退出码 2：**阻止执行**，并将 stderr 返回给 Claude 用于自我修正

很多人最常犯的错误，是在安全类 hook 中使用 exit 1——这只会记录错误日志，但不会真正阻止任何操作。

- 

- 

- 

- 

- 

- 

`.claude/``├── settings.json              # hooks 配置在这里（hooks 字段下）``└── hooks/                     # 存放 hook 脚本（约定俗成，不是必须）``    ├── bash-firewall.sh       # PreToolUse：拦截危险命令``    ├── auto-format.sh         # PostToolUse：自动格式化文件``    └── enforce-tests.sh       # Stop：确保测试通过`
```

`
`
```

**最常用的事件类型：**

- 
PreToolUse：在工具执行前触发（安全关卡）

- 
PostToolUse：工具执行成功后触发（用于格式化、lint）

- 
Stop：Claude 完成任务时触发（质量检查，比如“必须通过测试”）

- 
UserPromptSubmit：你按下回车时触发（用于提示词校验）

- 
Notification：桌面通知

- 
SessionStart / SessionEnd：注入上下文或清理环境

对于工具类事件，可以用 matcher（正则）限制触发范围：

- 
`"Write|Edit|MultiEdit"`：针对文件修改

- 
`"Bash"`：针对命令行操作

- 
不写 matcher：匹配所有工具

**一个典型的 hooks 配置示例：（自动格式化所有修改的文件，并阻止危险命令）**

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`{``  "hooks": {``    "PostToolUse": [``      {``        "matcher": "Write|Edit|MultiEdit",``        "hooks": [``          {``            "type": "command",``            "command": "jq -r '.tool_input.file_path' | xargs npx prettier --write 2>/dev/null"``          }``        ]``      }``    ],``    "PreToolUse": [``      {``        "matcher": "Bash",``        "hooks": [``          {``            "type": "command",``            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/bash-firewall.sh"``          }``        ]``      }``    ]``  }``}`
```

`bash 防火墙脚本会从 stdin 读取命令，匹配诸如：`
```

- 
`rm -rf /`

- 
`git push --force main`

- 
`DROP TABLE`

一旦匹配，就返回 exit 2，从而阻止执行。

**Stop hooks 同样非常强大。**比如一个运行 `npm test` 的脚本，如果测试失败返回 exit 2，就能阻止 Claude 提前宣布“完成”。

有一个坑需要注意：一定要检查 JSON 负载中的 `stop_hook_active` 标志。否则会出现死循环——hook 阻止 Claude，Claude 重试，hook 再阻止，如此反复。正确做法是允许第二次通过。

**桌面通知**可以通过在 ~/.claude/settings.json 中配置 Notification hook 实现：

- 
macOS：使用 osascript

- 
Linux：使用 notify-send

### 使用 hooks 时需要注意的几点

- 
hooks 在会话中**不会热更新**

- 
PostToolUse 无法“撤销”操作（因为已经执行了），需要拦截请用 PreToolUse

- 
hooks 对子 agent 也会递归触发

- 
hooks 以你的用户权限执行，没有沙箱保护。必须：

- 
对 shell 变量加引号

- 
校验 JSON 输入

- 
使用绝对路径

## skills/ 文件夹：可复用的按需工作流

Skills（技能）是 Claude 可以根据上下文**自动调用的工作流**。当任务符合描述时，它会主动触发。

每个 skill 都在独立目录中，并包含一个 SKILL.md：

- 

- 

- 

- 

- 

- 

- 

- 

`.claude/skills/``├── security-review/``│   ├── SKILL.md``│   └── DETAILED_GUIDE.md``└── deploy/``    ├── SKILL.md``    └── templates/``        └── release-notes.md`
```

SKILL.md 使用 YAML 前置块定义触发条件：

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`---``name: security-review``description: 全面安全审计。在审查代码漏洞、部署前或用户提到安全问题时使用``allowed-tools: Read, Grep, Glob``---``分析代码库中的安全漏洞：``1. SQL 注入和 XSS 风险``2. 凭证或密钥泄露``3. 不安全配置``4. 认证与授权缺陷``输出包含严重级别和具体修复建议。``参考 @DETAILED_GUIDE.md 获取安全标准。`
```

`当你说“帮我做一次安全审查”时，Claude 会自动匹配并调用该 skill。你也可以手动使用 ``/security-review` 调用。
```

```

与 commands 的关键区别：
```

- 
commands：单个文件

- 
skills：可以打包多个辅助文件（如示例中的 DETAILED_GUIDE.md）

个人 skills 可以放在 ~/.claude/skills/，在所有项目中通用。

## agents/ 文件夹：专用子代理（子人格）

当任务复杂到需要“专家”处理时，可以在 .claude/agents/ 中定义子代理（subagent）。

- 

- 

- 

`.claude/agents/``├── code-reviewer.md``└── security-auditor.md`
```

示例：code-reviewer.md

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`---``name: code-reviewer``description: 代码审查专家。在 PR 审查、查 bug 或合并前验证时主动使用``model: sonnet``tools: Read, Grep, Glob``---``你是一名资深代码审查专家，关注正确性和可维护性。``审查时：``- 指出 bug，而不仅是风格问题``- 提供具体修改建议``- 检查边界情况和错误处理``- 仅在有规模影响时指出性能问题`
```

`当 Claude 需要做代码审查时，会启动这个 agent，在独立上下文中完成任务，然后压缩结果返回。这样主对话不会被大量中间分析过程淹没。`
```

**tools 字段用于权限控制：**例如安全审计 agent 只需要读取权限，不应该有写入能力——这是有意设计的。

**model 字段用于成本优化：**

- 
Haiku：适合快速只读分析

- 
Sonnet / Opus：用于复杂任务

个人 agents 可以放在 ~/.claude/agents/，在所有项目中使用。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/Cic4GFNZLlxq7IascgXG9Uu0SGHjWM1ECiaRx3Cnt3AhEeMqnwcuUIyzNOLb1c86hicK4bKqWSAab5G39AouiakkQGpTh0Zje6YG5INjdY39pMQ/640?wx_fmt=jpeg&from=appmsg)

**settings.json：权限与项目配置**

位于 .claude/ 目录下的 settings.json 用于控制 Claude **可以做什么、不能做什么**。这里同时也是配置 hooks 的地方，你可以在这里定义 Claude 能运行哪些工具、可以读取哪些文件，以及在执行某些命令前是否需要询问你。

完整示例如下：

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`{``  "$schema": "https://json.schemastore.org/claude-code-settings.json",``  "permissions": {``    "allow": [``      "Bash(npm run *)",``      "Bash(git status)",``      "Bash(git diff *)",``      "Read",``      "Write",``      "Edit"``    ],``    "deny": [``      "Bash(rm -rf *)",``      "Bash(curl *)",``      "Read(./.env)",``      "Read(./.env.*)"``    ]``  }``}`
```

**各部分说明：**

- `$schema：用于在 VS Code 或 Cursor 中启用自动补全和校验（强烈建议保留）`

- `allow（允许列表）：这些操作无需确认即可执行。一般建议包括：`

- `Bash(npm run *)或 Bash(make *)：允许自由运行脚本`

- `Bash(git *)：允许只读 Git 操作`

- `Read / Write / Edit / Glob / Grep：文件操作`

- `deny（禁止列表）：这些操作完全禁止。建议包括：`

- 危险命令（如 `rm -rf`）

- 网络命令（如 `curl`）

- 敏感文件（如 `.env`、`secrets/`）

如果某个操作既不在 allow，也不在 deny 中，Claude 会**先询问再执行**。这是一种有意设计的“安全缓冲区”。

类似 CLAUDE.local.md，你可以创建 `.claude/settings.local.json` 来存放**个人权限配置**，它会被自动 git 忽略，不会提交到仓库。

## 全局 ~/.claude/ 文件夹

这个目录你不需要频繁操作，但了解它很有帮助：

- `~/.claude/CLAUDE.md 会加载到所有项目的会话中，适合放你的个人编码习惯、风格偏好等。`

- `~/.claude/projects/ 存储每个项目的会话记录和自动记忆。Claude 会自动记录：`

- 常用命令

- 代码模式

- 架构理解
可通过 `/memory` 查看和编辑

- `~/.claude/commands/ 和 ~/.claude/skills/ 存放全局可用的命令和技能`

通常你不需要手动管理这些内容。但当你发现 Claude “记住了你没说过的东西”，或者想清空某个项目的记忆时，这里就很关键。

## 全局结构总览

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

- 

`your-project/``├── CLAUDE.md                  # 团队指令（提交到仓库）``├── CLAUDE.local.md            # 个人覆盖（git 忽略）``│``└── .claude/``    ├── settings.json          # 权限、hooks、配置（提交）``    ├── settings.local.json    # 个人权限覆盖（git 忽略）``    │``    ├── hooks/                 # hook 脚本``    │   ├── bash-firewall.sh``    │   ├── auto-format.sh``    │   └── enforce-tests.sh``    │``    ├── rules/                 # 模块化规则``    │   ├── code-style.md``    │   ├── testing.md``    │   └── api-conventions.md``    │``    ├── skills/                # 自动触发的工作流``    │   ├── security-review/``    │   │   └── SKILL.md``    │   └── deploy/``    │       └── SKILL.md``    │``    └── agents/                # 专用子代理``        ├── code-reviewer.md``        └── security-auditor.md``
``~/.claude/``├── CLAUDE.md                  # 全局指令``├── settings.json              # 全局设置 + hooks``├── skills/                    # 全局技能``├── agents/                    # 全局代理``└── projects/                  # 会话历史 + 自动记忆`
```

## 一个实用的入门配置流程

如果你从零开始，可以按这个步骤来：

**Step 1**：在 Claude Code 中运行 `/init 自动生成初始 CLAUDE.md，然后精简成核心内容`

**Step 2**：创建 `.claude/settings.json 至少含有：`

- allow：运行命令

- deny：禁止读取 `.env`

**Step 3**：创建 1–2 个常用 commands。比如代码审查、修复 issue

**Step 4**：当 CLAUDE.md 变臃肿时，拆分到 `.claude/rules/`，并按路径作用域管理

**Step 5**：创建 `~/.claude/CLAUDE.md，写你的个人偏好，例如：`

- “先写类型再写实现”

- “优先函数式而不是类”

对于 95% 的项目，这已经完全够用。

skills 和 agents 是在你有复杂、重复流程时再引入的进阶工具。

.claude 文件夹，本质上是一套“协议”，用于告诉 Claude：

- 你是谁（你的习惯）

- 项目是做什么的（架构与规则）

- 应该遵守哪些约束（权限与流程）

你定义得越清晰：你花在纠正 Claude 上的时间就越少，它花在做有价值工作的时间就越多。

**优先把 CLAUDE.md 写好。这是杠杆最高的部分，其它都是优化。**

从简单开始，逐步迭代，把它当作项目基础设施的一部分来维护——一旦配置好，它每天都会为你持续产生价值。