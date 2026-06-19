---
title: "把Codex用到极致！OpenAI官方最佳实践完整解读"
url: "https://mp.weixin.qq.com/s/FDmkBYixEnMHppE04Z9NOQ"
source: "微信公众号"
author: "字节笔记本"
account: "字节笔记本"
pub_date: 2026-06-09
fetched: 2026-06-09
image_count: 5
---

# 把Codex用到极致！OpenAI官方最佳实践完整解读

**作者**: 字节笔记本 | **公众号**: 字节笔记本

OpenAI 的开发者文档发布一篇 Best Practices 的文章。

![](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1yrWg6gtVGTnqOm3ou3XeX6vrwdnTu6wR6c2DMOvMu9EeEwPskSwaCeaz9WIuh7enVTA9Bqs1ibRNxaWsWqP125ibyfAwxfF2XN8/640?from=appmsg)

这篇文档描述的 Codex 用法，和大多数人实际在用的方式，差距相当大，本文把官方文档的所有内容逐条拆解，补充具体操作案例。

* * *

## 六个支柱

其实整篇最佳实践的核心思路就是要把Codex当可配置的队友，给它建立对你的仓库、你的规范、你的工作习惯的持久记忆。

官方给出的六个支柱：

  * • 从正确的任务上下文出发
  * • 用 `AGENTS.md` 写入持久指引
  * • 通过配置文件统一行为
  * • 用 MCP 连接外部系统
  * • 把重复工作封装成 Skills
  * • 把稳定流程变成 Automations

后面的所有最佳实践都围绕这几点来展开。

* * *

## Prompt 要包含四个要素

> "Codex is already strong enough to be useful even when your prompt isn't perfect."

在大型仓库或者高风险任务里工作，清晰的 prompt 能让结果更稳定、更容易 review。

官方推荐的 prompt 四要素框架：

  * • **Goal** ：你想改什么、想做什么
  * • **Context** ：哪些文件、目录、文档、报错信息和这个任务相关（可以直接 @ 文件）
  * • **Constraints** ：Codex 需要遵守哪些规范、架构约定、安全要求
  * • **Done when** ：什么条件满足了才算完成（测试通过、行为符合预期、bug 不再复现）

这四个要素帮助 Codex 保持在范围内工作，减少假设，让你的 review 更轻松。

**差的写法和好的写法对比**

差：

> "帮我优化一下 checkout 流程"

好：

> **Goal** ：减少 `src/checkout/` 里重复的支付校验逻辑
> 
> **Context** ：相关文件在 `src/checkout/` 和 `src/orders/validation.ts`，后者有现成的 helper 可以参考
> 
> **Constraints** ：不改变公开 API 的响应格式，不引入新的依赖
> 
> **Done when** ：跑完 `pnpm test --filter checkout` 全部通过，diff 只涉及 `src/checkout/` 目录，最终回复列出改动文件和剩余风险

**关于推理级别的选择**

官方文档明确说，不同任务适合不同的推理强度：

  * • Low：快速、范围明确的任务
  * • Medium / High：复杂改动或者调试问题
  * • Extra High：长时间运行的 agentic 任务、需要深度推理的场景

不是所有任务都需要最高推理级别，会明显影响速度和成本，我个人推荐使用Medium，兼具速度和性价比：[拿什么拯救你，我的 Codex 额度！](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247517707&idx=1&sn=ee7ebe78be5e5a738412666547d078a4&payreadticket=HGrSSjJsm1TC8KkbuHztDZxtPs34UhR3PTufTz2ROYtKfyDnNalxnQ8RhJeWzC020BKw1fM&scene=21#wechat_redirect>)

**一个冷门技巧**

在 Codex App 里可以用语音输入代替打字来描述任务。这在需要描述复杂上下文的时候比打字快很多。

![](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1wkhOP7VNgTsNc4ZQxMiaJ0B2frNmK2TOiagZ4icKicLcktDiaqPf1tzWIouffkMFQ1Kht1fUjY0r2utibAd4IBWLArjSENLmNF9gAfY/640?from=appmsg)

* * *

## 复杂任务，先规划再动手

这是官方文档里我认为最值得深挖的部分之一，官方提供了三种规划方式，很多人只知道第一种。

**方式一：Plan mode（最常用）**

Plan mode 是最简单、最有效的选项。它让 Codex 先收集上下文、问清楚问题、建立更完整的计划，然后再开始写代码。

切换方式：CLI 里用 `/plan`，或者按 `Shift` \+ `Tab`。

**方式二：让 Codex 来问你**

如果你脑子里有个大概的方向，但不确定怎么清晰地描述，可以直接对 Codex 说：

> "我想做 X，但还没想清楚。先问我几个问题，挑战一下我的假设，帮我把这个模糊的想法变成可以写代码的具体需求，然后再动手。"

这种方式在需求探索阶段特别有用。相当于把 Codex 临时变成一个产品讨论伙伴，而不是直接执行者。

**方式三：PLANS.md 模板**

更高级的工作流，在仓库里维护一个 `PLANS.md` 或者执行计划模板，配置 Codex 在处理长期任务或多步骤工作时按照这个模板执行。

**一个典型案例：跨模块重构**

错误方式：直接告诉 Codex "重构一下用户权限系统"，结果它改了一堆文件，你发现方向完全不对，全部回退。

正确方式：

第一条 prompt：

> "先读一下 `src/auth/` 和 `src/permissions/` 的结构，不要改任何文件。解释一下当前的权限检查流程是怎么走的，列出如果我要把角色检查统一到 middleware 层，需要改动哪些文件，然后给一个最小化的实现方案。"

等它回复，你 review 方案，确认方向，再发第二条：

> "按照上面确认的方案实现。保持现有 API 接口不变。改完之后跑权限相关的测试，把没有覆盖到的测试盲区也列出来。"

两步走比一步走多花两分钟，但省掉了可能几小时的返工。

* * *

## AGENTS.md，持久指引的核心

一旦某个 prompt 模式有效了，下一步就是不要每次重复它，把它写进 `AGENTS.md`。

`AGENTS.md` 是 agent 的 README。它会自动加载到上下文里，是编码你和团队想要 Codex 如何在仓库里工作的最佳位置。

**一个好的 AGENTS.md 覆盖的内容**

官方列出了几个核心类别：

  * • 仓库结构和重要目录
  * • 如何运行这个项目
  * • 构建、测试、lint 命令
  * • 工程规范和 PR 要求
  * • 约束和"不能做"的边界
  * • 验收标准和验证方式

**三个层级，优先级从高到低**

  * • `~/.codex/` 里的全局 `AGENTS.md`：个人默认值
  * • 仓库根目录的 `AGENTS.md`：团队共享标准
  * • 子目录里的 `AGENTS.md`：局部规则，优先级最高

离当前工作目录越近的文件，优先级越高，你可以给前端目录和后端目录各写一套不同的规范。

**快速起点**

在 CLI 里跑 `/init`，会自动在当前目录生成一个 `AGENTS.md` 的初始版本。但官方提醒：这只是起点，要根据你团队实际的构建、测试、review、发布方式来修改它。

**怎么让 AGENTS.md 保持实用**

官方给了一条非常好的原则：**短而准确的 AGENTS.md，比长而模糊的更有用** 。先写基础部分，只有在你发现重复错误之后再增加新规则。

如果文件开始变得太长，主文件保持精简，把具体任务的指导（比如 code review 规范、架构说明）抽成独立的 markdown 文件，在 AGENTS.md 里引用。

**让 AGENTS.md 随时间进化的方法**

当 Codex 对同一个问题犯了两次错，让它做一个复盘，然后把纠正的规则更新进 AGENTS.md。

比如 Codex 两次在 TypeScript 文件里引入了 `any` 类型：
```
      修复：不要使用 `any` 类型，用 `unknown` 加类型守卫替代
```

把这条加进 AGENTS.md，这个问题就不会再出现了。AGENTS.md 是你和 Codex 之间不断积累的"工程协议"。

* * *

## 配置文件，让行为跨会话保持一致

配置是让 Codex 行为在不同会话和不同入口之间保持一致的核心手段。

可以配置的内容包括：模型选择、推理强度、沙箱模式、审批策略、profiles、MCP 设置等。

**配置分层**

官方推荐的模式：

  * • `~/.codex/config.toml`：个人默认值（从 Codex App 的 Settings → Configuration 打开）
  * • `.codex/config.toml`：仓库专属配置
  * • 命令行参数：只用于一次性的临时调整（如果用 CLI 的话）

`config.toml` 是定义持久偏好的地方：MCP 服务器、profiles、multi-agent 设置、实验性功能等。可以手动编辑，也可以直接让 Codex 帮你更新。

* * *

## 测试和 Review，不要只看最终回复

不要满足于让 Codex 做出改动，要让它在必要时写测试、运行相关检查、确认结果、在你接受之前 review 工作。

但 Codex 只有知道"好"是什么样的，才能自己做这个循环。这个信息要么来自 prompt，要么来自 AGENTS.md。

**让 Codex 完成完整循环的内容**

  * • 为改动编写或更新测试
  * • 运行对应的测试套件
  * • 检查 lint、格式、类型
  * • 确认最终行为符合请求
  * • Review diff，找 bug、回归、有风险的 pattern

**Codex App 的 diff 面板**

Codex App 有专门的 diff 面板，可以直接在本地 review 改动。点击 diff 里的某一行，可以直接给出反馈，这个反馈会作为上下文传给 Codex 的下一轮。这是一个比在聊天框里打字反馈更高效的方式。

**`/review` 命令**

`/review` slash command，这个比很多人知道的要全：

  * • 和 base branch 对比，做 PR 风格的 review
  * • Review 未提交的改动
  * • Review 某个特定 commit
  * • 使用自定义的 review 指令

如果你和团队维护了一个 `code_review.md` 文件，在 AGENTS.md 里引用它，Codex 在 review 的时候就会遵循这套规范。

* * *

## Skills，把可复用的方法封装起来

一旦某个工作流变得可重复，就不要再依赖长 prompt 或者反复来回了，用 Skill 来封装指令。

Skill 的核心是一个 `SKILL.md` 文件，加上上下文和支持 Codex 稳定执行的辅助逻辑。Skills 在 CLI、IDE 插件和 Codex App 里都可以用。

**怎么设计一个好的 Skill**

官方的建议：

  * • 每个 Skill 聚焦一件事
  * • 从 2-3 个具体的使用案例出发
  * • 定义清晰的输入和输出
  * • Description 要说清楚这个 Skill 做什么、什么时候用，包括用户会说的触发词
  * • 不要一开始就覆盖所有边界情况

一个实用的经验法则：**如果你反复在用同一个 prompt，或者反复在纠正同一个工作流，那它就应该变成一个 Skill。**

**典型的 Skill 候选**

  * • 日志分类分析
  * • 发版说明自动生成
  * • 按 checklist 做 PR review
  * • 迁移方案规划
  * • 遥测数据或故障摘要整理
  * • 标准调试流程

**如何创建 Skill**

推荐用 `$skill-creator` Skill 来生成第一版框架，用 `$skill-installer` Skill 来安装到本地。

**Skills 的存储位置**

  * • 个人 Skills：`$HOME/.agents/skills`
  * • 团队共享 Skills：仓库里的 `.agents/skills` 目录（可以提交进 git）

团队共享 Skills 对新成员 onboarding 很有帮助，clone 仓库就可以用团队积累的所有 Skills。

* * *

## Automations，让稳定的工作流自动跑

一旦工作流变得稳定，就可以让 Codex 在后台自动帮你跑它。

在 Codex App 的 Automations 标签里，

可以配置选择哪个项目、用什么 prompt（可以调用 Skills）、跑的频率、在专用 git worktree 还是本地环境里运行。

**适合自动化的任务**

  * • 总结最近的 commit
  * • 扫描可能的 bug
  * • 起草发版说明
  * • 检查 CI 失败
  * • 生成每日站会摘要
  * • 按计划跑重复性分析

Automations 不只是用来执行的，也可以用来反思和维护。让 Codex 定期 review 最近的 session、总结重复出现的摩擦点、更新 prompt 或者工作流配置。

**Skills 和 Automations 的分工**

Skills 定义方法，Automations 定义节奏。如果一个工作流还需要大量人工干预，先把它做成稳定的 Skill，稳定之后再自动化。

* * *

## Session 管理，别让一个线程干所有事

这部分是官方文档里很多人没注意到的内容，但对日常使用影响很大。

Codex 的 session 不只是聊天记录，而是包含了上下文、决策过程和操作历史的工作线程。怎么管理它，直接影响输出质量。

**Codex App 里的管理方式**

App 里可以置顶线程，也可以创建 worktree。

**CLI 里的关键 slash 命令**

官方文档列出了这些：

  * • `/experimental`：开关实验性功能，同时写入 `config.toml`
  * • `/resume`：恢复一个保存过的会话
  * • `/fork`：基于当前线程创建新线程，同时保留原始记录
  * • `/compact`：当线程变得很长时，生成一个摘要版本的上下文（Codex 也会自动 compact）
  * • `/agent`：在并行运行多个 agent 时，切换当前活跃的 agent 线程
  * • `/theme`：选择语法高亮主题
  * • `/apps`：直接在 Codex 里使用 ChatGPT apps
  * • `/status`：查看当前会话状态

**关于 /fork 的使用时机**

如果工作还属于同一个问题，留在同一个线程里通常比新开一个好，因为它保留了推理上下文。只有当工作真正分叉成两个方向的时候，再用 `/fork`。

**Worktree 和并行工作**

Codex 的 multi-agent 工作流可以让你把一些有边界的工作从主线程里分离出去。

主 agent 专注核心问题，子 agent 负责探索、写测试、分类问题等。

并行工作有效的前提是：写入范围不冲突。

好的并行拆分：

  * • 后端改动 + 文档更新
  * • 一个 agent 写测试，另一个查根本原因
  * • 多个 agent 分别提出备选方案
  * • 一个 agent 实现，另一个只做 review

不好的并行拆分：

  * • 多个 agent 重构同一批文件
  * • 需求还没确认就跑多个实现
  * • schema 和调用方同时改动但没有协调

**每个任务一个线程，而不是每个项目一个线程。** 把一个项目的所有工作都放在一个线程里，会导致上下文膨胀，输出质量随时间下降。

* * *

## 八个官方明确点名的错误用法

官方文档在最后总结了一组常见错误。

**错误一：把持久规则堆在 prompt 里**

每次 prompt 都把项目规范、代码风格、约束条件重新描述一遍，而不是把它们写进 AGENTS.md 或者 Skill。

结果：上下文浪费，而且一旦忘了写就失效。

正确做法：持久规则进 AGENTS.md，一次写入，永久生效。

**错误二：不告诉 Codex 怎么运行和测试代码**

给 Codex 一个任务，但没有告诉它构建和测试命令是什么，等于让它在黑盒里工作。

结果：Codex 猜不出验证方式，只能靠"写完了感觉对了"来判断完成。

正确做法：AGENTS.md 里写清楚验证命令，每个任务的 Done when 里也要有具体的检查步骤。

**错误三：复杂任务跳过规划**

直接让 Codex 开工，没有先让它理解系统、提出方案。

结果：它套用了一个"看起来合理"的方案，但不适配你实际的代码结构。

正确做法：复杂任务先用 Plan mode 或者要求 Codex 先解释再实现。

**错误四：没弄清楚工作流就给了完整权限**

一开始就给 Codex 对电脑的完整访问权限，觉得权限越大越好用。

结果：可能出现意外的全局改动，或者影响到不该影响的文件。

正确做法：从默认的收紧权限开始，理解了工作流之后再针对性地放宽。

**错误五：不用 worktree 在同一批文件上同时跑多个线程**

多个 Codex 会话同时修改同一批文件，没有隔离。

结果：改动冲突，搞不清楚哪个版本是对的。

正确做法：用 git worktree 隔离并行工作，每个线程在独立的 worktree 里操作。

**错误六：还没手动验证可靠就把工作流自动化**

急着把一个不稳定的工作流变成 Automation。

结果：自动化把不稳定的行为规模化了，反而比手动更难处理。

正确做法：先把工作流做成稳定的 Skill，跑几次验证它是可靠的，再设置自动化。

**错误七：像盯着初级员工一样盯着 Codex**

每一步都在旁边看着，不敢让它跑完再检查结果。

结果：Codex 的异步和并行优势完全发挥不出来，效率和直接自己写差不多。

正确做法：启动任务之后去做自己的工作，回来 review 结果。把 Codex 当后台任务，不是实时对话。

**错误八：每个项目一个线程**

在一个线程里积累整个项目的所有工作记录。

结果：上下文越来越膨胀，Codex 越来越难从噪音里找到相关信息，输出质量随时间劣化。

正确做法：每个任务或者每个独立工作单元开一个新线程。用 `/fork` 在保留上下文的情况下创建分支线程。

* * *

## 一个完整的工作流

把官方文档的所有建议串起来，一个成熟的 Codex 工作流大概是这样的：

**初始化阶段（一次性）**

  1. 1\. 在仓库里跑 `/init` 生成 AGENTS.md 初始版本
  2. 2\. 修改 AGENTS.md 写入实际的构建命令、验证命令、约束和 pattern
  3. 3\. 在 `~/.codex/config.toml` 里配置个人默认值
  4. 4\. 接入高价值的 MCP 服务器（一两个就够）

**日常任务流程**

  1. 1\. 每个任务开一个独立线程
  2. 2\. 按四要素框架写 prompt（Goal + Context + Constraints + Done when）
  3. 3\. 复杂任务先用 Plan mode 或者让 Codex 先问你
  4. 4\. 启动之后去做自己的其他工作
  5. 5\. 回来检查结果，用 diff 面板 review
  6. 6\. 发现稳定的 pattern 就封装成 Skill

**持续改进**

  1. 1\. Codex 犯了同一个错误两次，更新 AGENTS.md
  2. 2\. Skill 稳定了，设置 Automation
  3. 3\. 定期让 Codex 回顾最近的 session，更新配置和 AGENTS.md

* * *

## 最后说一句

官方文档里有一句话我觉得很准：

> "Codex shouldn't just generate code. With the right instructions, it can also help test it, check it, and review it."

把这个理解吃透，Codex 就从一个写代码的工具变成了一个完成整个开发循环的系统。