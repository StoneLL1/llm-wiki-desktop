---
title: "一篇最全Agent Harness综述来了！"
url: "https://mp.weixin.qq.com/s/pG39PRnZFjSIxwYcPKD47A"
source: "微信公众号"
author: "Datawhale"
account: "Datawhale"
pub_date: 2026-05-27
fetched: 2026-05-27
image_count: 9
---

# 一篇最全Agent Harness综述来了！

**作者**: Datawhale | **公众号**: Datawhale

分享目前看到最系统、也最工程化的一篇 Agent Harness 综述，CMU、Yale、JHU、Virginia Tech、Amazon 等联合出品：《Agent Harness Engineering: A Survey》。

![论文封面](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS8NIWoAfPgbAibamC2iaRjn2yU07PlVHrYwmgQLl3gMOg8V1TctOb4jYmicTGxvGAoj3zOC8GZ6cKylOG4blfniaia4MH0kSo9ZynWQ/640?wx_fmt=png&from=appmsg)

论文主页地址：https://picrew.github.io/LLM-Harness/

这篇论文把 Agent 真正跑起来时，包在模型外面的那层工程系统讲透了。

它用 ETCLOVG 七层框架拆解 Agent Harness，覆盖执行环境、工具接口、上下文管理、生命周期编排、可观测性、验证评估和安全治理。同时梳理了 170+ 个开源 Agent Harness 项目，串起从 Prompt Engineering、Context Engineering 到 Harness Engineering 的工程演进。

我们在不改变原意的情况下，做了如下整理。

论文开头就提出了一个判断：学术界长期把 Agent 研究重点放在模型上。

模型能不能规划？能不能调用工具？能不能记住上下文？能不能和其他 Agent 协作？这些当然重要。

但问题是，当 Agent 开始进入长任务、真工具、真实环境之后，失败往往不是因为模型"不够聪明"，而是因为系统没把它管好。

论文列了几组结果：有研究只改了编辑工具格式和周边 harness，不改模型本身，编码 benchmark 上最高带来 10 倍提升。还有一个固定的 GPT-5.2-Codex Agent，通过重构系统 prompt、加入中间件上下文注入、自验证 hooks，在 Terminal-Bench 2.0 上从 52.8% 提升到 66.5%。Meta-Harness 则通过自动优化 harness，在 Terminal-Bench-2 上做到 76.4%，超过手工设计方案。

这些数字当然还要看具体实验设置，但它们指向同一个现象：

很多团队还在把问题归因于"模型不够强"。真实情况可能是：模型已经够强了，是你的工具接口、上下文、沙箱、验证和权限系统太弱。

Agent 工程经历了三次迁移

这篇综述有一个很适合中文读者理解的框架：Agent 工程从 2022 到 2026，大概经历了三个阶段。

![论文 Figure 1：Prompt、Context 和 Harness Engineering 的区别。](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSibhxtZqDicYwUNVPLaG5ib2diaIiaButLf94tj6DInvUb3oTGVINTWfCmOclNy91m5mcCKlsIsyRWPt6FVpljP22nOiaIicD29Bpofxg/640?wx_fmt=png&from=appmsg)

第一阶段是 Prompt Engineering。那时大家主要卷提示词。怎么写 system prompt，怎么放 few-shot，怎么让模型按步骤推理。工程对象很窄，就是把一段输入文本调好。

第二阶段是 Context Engineering。Agent 开始跑更长的任务后，问题变成：模型每一步到底该看见什么？不是所有资料都塞进去，而是要决定哪些信息该进上下文，哪些记忆要检索，哪些工具结果要压缩，窗口满了怎么办，长期任务中哪些状态要保留。

第三阶段，就是 Harness Engineering。当模型已经能处理更复杂任务时，瓶颈转到模型外部：谁来维护状态？谁来调工具？谁来限制权限？谁来注入反馈？谁来验证进度？谁来记录 trace？谁来在失败后恢复？

Prompt Engineering 解决的是"怎么跟模型说话"。Context Engineering 解决的是"模型该看见什么"。Harness Engineering 解决的是"怎么让模型在真实世界里可靠干活"。

一个 Harness 到底包括什么？

论文提出了一个七层分类，叫 ETCLOVG。名字有点拗口，但拆开看很实用。

![论文 Figure 2：Agent Harness Engineering 的七层结构。](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS94V5F1KYYnY1yQatnRbTruBXxtUicVYCyJoo38OzWiaxXIpkUSQdbxpsjuYJfZbLv3vzfbpiaCqmy6FrFNFURyibsv33zODYiaH198/640?wx_fmt=png&from=appmsg)

- **Execution**：执行环境。Agent 在哪里跑？本地、容器、浏览器、桌面、远程沙箱？边界在哪里？
- **Tooling**：工具接口。工具怎么描述，怎么发现，怎么调用，怎么防止模型乱选工具？
- **Context**：上下文和记忆。短期上下文、会话状态、长期记忆怎么管理？
- **Lifecycle**：生命周期和编排。一个 Agent 是单轮执行，还是多轮循环？是一个 Agent 干到底，还是 planner、executor、reviewer 分工？
- **Observability**：可观测性。每次模型调用、工具调用、检索、报错、重试、token 成本、延迟，都要能追踪。
- **Verification**：验证和评估。结果对不对？失败到底是模型错了、工具错了、上下文错了，还是测试环境错了？
- **Governance**：治理和安全。Agent 有什么权限？能不能发邮件、改代码、调 API、读私有数据？谁来审批？谁来审计？

这七层合在一起，才是一个能跑长任务的 Agent 系统。

很多人理解 Agent，还停在"模型 + 工具调用"这一层。但论文的意思很明确：工具调用只是其中一层。真正的 Agent 产品，要有执行环境、上下文、编排、监控、验证和治理。

否则它很容易变成一个会动的 demo。能演示，不等于能上线。能跑一次，不等于能长期稳定跑。

## 为什么可观测性和治理要单独拿出来？

这篇综述一个重要贡献，是把 Observability 和 Governance 从"附属功能"里拎出来，作为独立层。

过去很多 Agent 框架会把日志、监控、权限、审计当成周边功能。先把 Agent 跑起来，再补一点 logging，再加一点 approval。

但真实生产里，这两件事不是装饰。

Agent 会调用工具、执行 shell 命令、修改代码、读写数据库、发邮件、访问第三方 API。它不是普通聊天机器人。它在行动。

一旦 Agent 开始行动，你就必须知道两件事：第一，它到底做了什么。第二，它到底被允许做什么。前者是可观测性，后者是治理。

没有可观测性，Agent 失败了你不知道为什么。没有治理，Agent 成功了你也不一定敢用。

## 有了Harness，Agent 的评价方式也要变

这篇论文还有一个判断很值得写进文章：**Agent 评估不能只看最终成功率。**

过去我们习惯看 benchmark：这个模型 pass rate 多少？那个 Agent 排名第几？SWE-bench 上涨了几个点？但对长任务 Agent 来说，这还不够。

因为一个 Agent 的最终结果，背后混着很多变量：模型、提示词、工具、上下文、沙箱、测试、重试策略、权限、评估器。

同样一个成功率，可能代表完全不同的系统质量。一个 Agent 可能靠疯狂重试刷过任务，成本很高；一个 Agent 可能走了危险路径，最后结果对了，但过程不合规；一个 Agent 可能 benchmark 过了，却是利用了测试漏洞；一个 Agent 可能失败了，但失败原因是环境缺包，不是模型不会。

所以论文主张：评估要 trace-native。也就是把完整执行轨迹作为评估对象。

![论文 Figure 11：Agent 验证与评估的 task-to-feedback 生命周期。](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSibVTu0zicfuuwialAI3afd4LcV1dy93UIQTTK04u4DQuxIic6BWGeXGOLONiaR1mxH36ATvcLiaHjszrqj4OukInjA94OSJN8Um6kq4/640?wx_fmt=png&from=appmsg)

要记录模型输出、工具调用、工具返回、环境状态变化、上下文快照、错误、重试、恢复动作、token 使用、延迟和成本。然后再判断三件事：结果是否正确，路径是否合理，评估器本身是否可信。

这会把 Agent 评估从"排行榜机制"拉回"质量控制机制"。排行榜回答的是：谁分高？质量控制回答的是：为什么失败？该改哪一层？

## 生产 Agent 最大的矛盾：能力越强，控制越难

这篇综述还把几个跨层矛盾说得很清楚。

第一个是成本、质量、速度三角。你想让 Agent 更安全，就要更强的沙箱、更细的权限、更完整的 trace。你想让它更可靠，就要更多验证、更复杂回归测试、更长上下文。但这些都会增加成本和延迟。

第二个是能力和控制的矛盾。你给 Agent 更多工具，它能做更多事，但也更容易选错工具，prompt injection 面也更大。你给它长期记忆，它能连续工作，但也会带来隐私、过期信息、来源不明的问题。你给它更开放的执行环境，它更有用，但失控半径也更大。

第三个是 harness coupling，外壳耦合问题。Harness 的每一层不是独立的。工具描述会占上下文窗口，影响模型行为；执行环境会影响评估结果，因为包版本、重置机制、延迟都不一样；可观测性 trace 如果没有记录身份和权限状态，就不能成为治理证据。

所以改 prompt、改工具、改 memory、改 sandbox、改 verifier，都不是局部优化。它们可能改变整个系统行为。

## 从 Agent Framework 到 Agent Platform

如果只看工具生态，这篇论文也给出一个趋势判断：Agent 正在从 framework 走向 platform。

Framework 解决的是局部抽象：agent、tool、memory、loop。Platform 要解决的是完整生产系统：durable workspace、managed sandbox、identity、billing、observability、evaluation、governance、human handoff。

早期大家拼的是：谁能最快搭一个 Agent loop。现在拼的是：谁能让这个 loop 长期可靠运行。

![论文 Figure 3：2022-2026 Agent Harness 系统演进时间线。](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS8Zmbjd2OkteoiaXhQeRHawEOP7weKkEanW00hibZ3ib8E7Lcn1xZ6sFxjPK6MtwAcibXUkMAXlkJNKFslUGywAJJ2RAd1YPhvbPns/640?wx_fmt=png&from=appmsg)

所以 Agent 平台的竞争，可能不会只发生在模型层，也不会只发生在开发框架层，而会发生在整套 harness 能力上。

谁的执行环境更稳，谁的工具协议更清晰，谁的上下文更不容易漂，谁的 trace 更好用，谁的验证更接近真实任务，谁的权限和审计更可控，谁就更可能把 Agent 送进真实生产流程。

## 下一阶段，Agent 要学会"少加脚手架"

不过这篇论文并不是简单鼓励大家给 Agent 套更多东西。

随着模型变强，Harness 也要重新评估。每一个 wrapper、reset、verifier、planner、memory rule、permission gate，本质上都代表一个假设：模型自己做不好，所以我在外面加一层控制。

但如果模型能力变了，这些控制可能就不再必要，甚至会拖后腿。论文提到 Anthropic 的一个例子：在长时间应用开发任务中，某些 context reset 对旧模型有用，但对更强模型已经可以去掉，去掉之后成本下降，质量没有变差。

这其实说明，Agent 工程不是越复杂越好。好 Harness 不只是会加控制，还要知道什么时候删控制。

## 结语：Agent 的下一场竞争，是模型外面的工程外壳

如果用一句话概括这篇 71 页综述：

```
Agent 的下一场竞争，不只是模型能力，而是模型外面的工程外壳。
```

过去几年，我们先学会了写 prompt。后来开始学会管理 context。现在，真正要补的是 harness。

因为 Agent 一旦从聊天框走向真实任务，它就不再是一个"会回答问题的模型"。它变成一个会读上下文、调工具、改环境、执行动作、留下痕迹、接受验收的系统。

系统就需要系统工程。

对开发者来说，别再只问"哪个模型更强"。你还要问：它在哪个环境里跑？工具接口是不是为 Agent 设计的？上下文会不会漂？状态能不能跨轮次恢复？失败能不能从 trace 里定位？结果有没有 verifier？权限、身份、审计有没有闭环？

Prompt Engineering 是把模型叫醒。Context Engineering 是让模型看见正确的信息。Harness Engineering 是让模型在真实世界里可靠行动。

Agent 要从玩具变成基础设施，差的就是这层外壳。
