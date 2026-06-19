---
title: "Claude Code + Agent Teams，并行任务的最佳实践"
url: "https://mp.weixin.qq.com/s/_qlK7sP3V_5P-z6XaHc2sA"
source: "微信公众号"
author: "鲁工"
account: "AI编程实验室"
fetched: 2026-04-27
sha256: 590400b5d99a452a
---

# Claude Code + Agent Teams，并行任务的最佳实践

**作者**: 鲁工 | **公众号**: AI编程实验室

大家好，我是鲁工。

Agent Teams不算啥新功能了，我很早就在Claude Code里面用上了。之前有Agent swarm、Agent group等叫法，但本质上都是并行驱动多个Agent来完成任务，其实就是开团。

我上周末把Claude Code中有关Agent teams的文档仔细研读了下，发现这个东西能挖的东西挺多。

这篇文章按官方文档的逻辑顺序串起来写一下：先讲清楚和subagents的区别、什么场景该用、怎么启动、怎么控制、怎么算才算最佳实践，再用三个我自己跑过的实测案例落到具体场景。看完这篇基本能从零上手。

## Agent Teams vs. Subagents

Claude Code里做多任务并行，之前主要靠Subagents（子智能体）或者git worktrees。这块我之前写过一些文章，具体可参考：

- [用Subagents打造Claude Code专业开发团队](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247483687&idx=1&sn=7e14d5bd5dafc9e8db0181687d3da5bc&scene=21#wechat_redirect)
- [Claude Code + Git Worktrees，并行开发最正确的打开方式](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247484869&idx=1&sn=1d723494a0cef93f4a21d1d1e5913f8f&scene=21#wechat_redirect)

Subagents能有独立的context window、配专用的系统提示、选不同的工具和模型。但它有个天花板：只能向主代理汇报结果，彼此之间没有直接对话的通道。

Agent Teams把这个限制打掉了。

三个维度看差别：

| 维度 | Subagents | Agent Teams |
|------|-----------|-------------|
| 通信 | 只向主代理汇报 | 队友之间直接收发消息 |
| 协调 | 主代理统一管理 | 共享任务列表+自协调 |
| Context | 自己的窗口，结果回主会话 | 自己的窗口，完全独立 |
| Token成本 | 较低（结果汇总） | 较高（每个队友是独立Claude实例） |
| 适用场景 | 结果导向的专注任务 | 需要讨论和质疑的复杂工作 |

总结下就是，Subagents是实习生各自给领导交作业，Agent Teams是小组同事坐一起能互相讨论。

## Agent Teams什么时候启用

不是所有任务都适合开团。因为开团的token成本是真的高，一个3人团队的token消耗大概是单会话的3到4倍，盲目滥用会很疼。

官方文档列了几个最强用例：

- **研究和审查类任务**。多个队友可以同时调查问题的不同方面，然后互相分享和质疑彼此的发现。比如审一个PR，三个队友分别负责安全、性能、测试覆盖三个角度。
- **新模块或功能开发**。每个队友拥有一个独立的部分，不会相互干扰。前提是任务可拆，文件不冲突。
- **带竞争假设的Bug调试**。多个队友各拿一个理论并行测试，能更快收敛到根因。这个对抗"找到一个看似合理的解释就停下来"的锚定偏见特别有用。
- **跨层协调**。前端、后端、测试改动同时进行，每个队友负责一个方面。

反过来不适合的场景也很明确：顺序依赖强的任务（B必须等A完成）、同一个文件多人编辑（容易覆盖）、依赖关系特别多的工作。这几种场景下要么用单会话顺序处理，要么用subagents就行，强行开团反而是负担。

## 如何启用Agent Teams

Agent Teams功能在Claude Code中并不是默认开启的，开团有两个前置条件。

一是Claude Code v2.1.32以上（这个大家基本都满足，Native安装方法会自动更新Claude Code版本）。二是在 `~/.claude/settings.json` 里加一个环境变量：

```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

启动方式很简单：直接用自然语言告诉Claude你想要什么团队、做什么任务。Claude自己负责创建team lead、生成teammates、协调工作。

官方文档里给的标准例子：

> I'm designing a CLI tool that helps developers track TODO comments across their codebase. Create an agent team to explore this from different angles: one teammate on UX, one on technical architecture, one playing devil's advocate.

这条prompt官方原话是"This example works well"，三个角色独立、不需要互相等待，开团效果最好。

**触发关键词很关键**。prompt里必须显式出现 `agent team` / `teammate` / `team lead` / `shared task list` 这类词，Claude才会走Agent Teams分支。中文口语里说的"开两个agent并行"、"派两个子代理"这种写法有概率会被识别成subagents，可能不会创建team。

**指定队友数和模型**。Claude会根据任务自己决定开几个队友，但你也可以直接指定：

> Create a team with 4 teammates to refactor these modules in parallel. Use Sonnet for each teammate.

也可以让不同队友用不同模型，比如lead用 Opus、teammate用Sonnet控制成本。

**Claude主动提议团队**。如果你给的任务确实受益于并行，Claude会主动建议开团，但不会未经你同意就创建。最终决定权在你手里。

判断team是否真的被创建有个硬标准：执行后看 `~/.claude/teams/` 目录下有没有新生成的team目录（带 `config.json` ）。有目录才是真的开团了；没有就是走的subagents。

## 如何控制Teammates

团队跑起来之后，你不是只能等结果，整个过程都可以介入。先看显示模式，Agent Teams功能支持in-process和split panes两种显示模式：

- **In-process**：所有队友都跑在主终端里，用 `Shift+Down` 循环切换。任何终端都能用，不用额外装东西。
- **Split panes**：每个队友一个独立窗格，输出全程可见，点击哪个窗格就直接进到对应队友的会话。需要tmux或iTerm2支持。

启用方式三种，从大到小：

1. 全局config，在 `~/.claude.json` 里设：

```json
{
  "teammateMode": "tmux"
}
```

2. 单次启动flag：`claude --teammate-mode tmux`

3. 默认 `auto` 模式：我们已经在tmux session里打开Claude Code，Claude会自动切split panes，否则回到in-process。

split panes的两种入口：tmux（macOS下最稳定）；iTerm2，需要安装it2 CLI，并在Settings → General → Magic开Enable Python API）。

每个队友都是完整独立的Claude Code会话，可以直接给它们发消息：

- In-process模式：`Shift+Down` 切到目标队友，输入消息直接发；`Enter` 进入队友会话查看详情，`Escape` 中断当前轮次；`Ctrl+T` 调出共享任务列表。
- Split-pane模式：直接点击窗格就能进对应队友的会话，每个队友有完整终端视图。

共享任务列表协调整个团队的工作。任务有三种状态：pending、in progress、completed，还可以设置依赖关系（一个任务blocked by另一个时，依赖任务不解除就不能认领）。

两种分配方式：lead显式指派（"把这个任务给researcher队友"），或者队友完成自己的任务后自动认领下一个未分配的。

复杂或风险高的任务可以要求队友先做planning，lead审过才动手：

> Spawn an architect teammate to add structured logging middleware. Require plan approval before any changes. Reject any plan that does not include rollback strategy and test coverage.

关键：在prompt里给lead写清楚批准准则（"必须包含测试覆盖"、"必须有回滚方案"），不然lead会按自己的判断批。被拒绝的队友会留在plan模式，按反馈修订重新提交。

### 关闭和清理

关闭单个队友：

> Ask the researcher teammate to shut down

队友收到关闭请求可以批准或拒绝。

整个团队跑完，跟lead说一句：

> Clean up the team

这会删除共享团队资源。如果还有队友在跑，清理会失败，先关闭它们再清。这一步必须由lead执行，队友自己跑clean会出问题。

### Hooks强制质量门

Agent Teams配套了三个新的hook事件：`TeammateIdle`（队友空闲时）、`TaskCreated`（任务创建时）、`TaskCompleted`（任务完成时）。在hook里以exit 2退出可以阻断对应行为，stderr内容会作为反馈传给teammate。

适合把团队规范硬编码进去，比如"任务标记完成前必须跑过测试"、"不允许创建修改数据库schema的任务"。比prompt里反复叮嘱可靠得多，prompt是软约束模型可能忘，hook是硬约束直接拦住。

## 两个实测玩法

下面给两个Agent Teams的玩法，一个是常规启用玩法，一个是tmux启用玩法。

### 案例一：一句话调研开团

最常规的玩法，给一个调研任务+大致角色分工，剩下交给Claude。

比如：

启用Agent Teams帮我调研Claude Code中MCP、Skills、Plugins、Hooks、Agent teams的核心区别与联系。

跑下来Claude会自动创建team lead加5个teammates，各跑各的方向，最后lead综合输出。

### 案例二：使用tmux体验分窗并行

我们可以把上述常规的Agent teams启用方法在tmux下重试一遍，感受下tmux对多个teammates的自动分窗并行效果。

## 并行工作的最佳实践

跑过几个团队之后总结的几条经验，结合官方建议。

- **给队友足够的context**。队友自动加载项目级的CLAUDE.md、MCP servers和skills，但不会继承lead的对话历史。在生成prompt里要把任务相关的关键信息写清楚，特别是文件路径、模块约定、技术栈细节。

- **团队规模3到5个最合适**。再多协调开销和token成本就指数上升，3到5个队友做5到6个任务每人是经过验证的甜蜜点。如果有15个独立小任务，3个队友是一个好起点，让队友各自轮转处理。teammates多了，即使是Max 20x订阅，也很容易触发五小时限额。

- **任务粒度要恰到好处**。每个任务应该独立可完成，有明确的输入和输出。太粗的任务需要队友做过多规划决策，太细的任务增加lead的协调成本。

- **依赖管理要显式**。任务间如果有依赖，lead会自动设blocked by，但最好在初始prompt里就说清楚哪些任务依赖哪些。不要让Claude自己去猜。

- **利用plan-approval机制做质量门**。对高风险任务，在prompt里加一句"Require plan approval before any changes"。这比事后review成本更低。

- **合理选择模型**。lead用最强模型（Opus），teammates可以用轻量模型（Sonnet/Haiku）控制成本。用 `claude --model sonnet --teammate-mode tmux` 可以全局指定teammate模型。

- **关注hooks做硬约束**。团队规范写进hook比写进prompt更可靠。特别适合CI/CD类硬性要求。

- **不要在prompt里写sub-agent相关词**。如果本意是开团，prompt里应使用 `agent team`、`teammate` 等关键词。写 `sub-agent`、`subagent`、`delegate` 可能被识别为subagents模式，不会创建team。

- **观察 `~/.claude/teams/` 目录**。判断是否真的走了Agent Teams，最直接的方式就是看这个目录下有没有新的team目录。有目录才是真开团。

- **团队跑完记得清理**。`Clean up the team` 会释放资源。不清理的话下次开团可能会有残留影响。

## 最后

希望这篇文章能帮你全面理解Claude Code的Agent Teams功能。

AI编程不在于写代码本身，而在于能否驾驭工具、编排工具、让工具替你完成创造。Agent Teams是这个理念下的一个重要能力：把单点作战升级为团队作战。

我是鲁工，持续关注我，获取更多AI编程实战经验。

- Claude Code多任务并行，从入门到精通
- Agent Teams极大拓展了Claude Code的能力边界
- 实测验证，开团效果远超单会话顺序处理
