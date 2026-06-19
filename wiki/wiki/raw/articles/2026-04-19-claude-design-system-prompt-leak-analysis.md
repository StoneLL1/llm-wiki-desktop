---
title: "暴击设计行业的 Claude Design，系统提示词在 GitHub 上泄露了"
url: "https://mp.weixin.qq.com/s/56ABHcANBsrAQjqxUgMQeQ"
source: "微信公众号「逛逛GitHub」"
fetched: 2026-05-27
sha256: 0718e87dbad90605
image_count: 13
---

打开 Claude 的更新日志，你会看到一条密集得令人窒息的时间线：

**1 月 12 日，Claude Cowork 上线： 一个能直接操作你文件系统的桌面级 Agent，不是聊天机器人，是真正干活的虚拟同事。****2 月 5 日，Opus 4.6 发布： 100 万 token 上下文窗口，14.5 小时任务完成时间，当时所有前沿模型之最。****3 月更不用说了 ，Claude Memory 全量开放、100 万 token 上下文标准定价上线、Cowork 插件生态成型。****还有 Claude Code Channels 发布：通过 Telegram 和 Discord 给编程 Agent 下指令，手机上发条消息，回来就是成品。****4 月份，Claude Code 还来了一波大更新 ：协作工具、Agent Teams，多人并行开发。****前两天 Opus 4.7 上线了，软件工程能力大幅跃升，新增 xhigh effort 等级。**

然后是 **4 月 17 日** 。

Anthropic 发布了 **Claude Design** ：一个用自然语言生成原型、PPT、一页纸文档的设计工具。

发完 Figma 和 Adobe 的股价狂跌。

这个 Claude Design 面向的不是设计师，而是创始人、产品经理这些有想法但不会用 Figma的人。

TechCrunch 的标题写得很直接： _"Anthropic launches Claude Design, a new product for creating quick visuals."_

发布不到 24 小时，安全研究员 **Pliny the Liberator** 就在 GitHub 的 CL4R1T4S 仓库放出了 Claude Design 的完整系统提示词。

3000 多词，信息密度极高。

01

**提示词拆解**

① AI 是专家设计师，你是经理

提示词开篇就定调：

> "You are an expert designer working with the user as a manager."

不是 AI 助手帮你做 PPT。

是你雇了一个资深设计师，你负责提需求，TA 负责交付。

HTML 是这个设计师的工具，但产出形式多种多样：PPT、交互原型、动画、一页纸文档。

一种媒介，多种形态。

提示词还特别强调了一点：**"Avoid web design tropes and conventions unless you are making a web page."**

也就是说，这不是在做网页，是在做设计。这个区分很重要。

② 反 AI 味清单

提示词里有一段专门列出了AI slop tropes，也就是 AI 味设计的黑名单：

  *   *   * **禁止圆角 + 左边框强调色的容器，这大概是 AI 生成 UI 最经典的视觉指纹**
  * **禁止 SVG 画图，没有素材就用占位符，不要勉强生成假图**
  * **禁止 Inter、Roboto、Arial 等过度使用的字体**

核心原则只有一个：**占位符优于垃圾的实现****。**

**没素材就用空白框，没图标就用占位符。别试着用 SVG 画一个 差不多的图标，那反而看起来更傻。**

③ 提示词里藏着一整套产品系统

读得越深越发现，这不仅是提示词，而是一套完整的设计工程体系。

**Starter Components** ：设备边框、舞台、动画引擎、设计画布，设计师不需要从零开始画 iPhone 外壳或 PPT 框架，系统直接提供。

**React + Babel 锁版本** ：固定 CDN 版本 + integrity hash。不是"用最新版"，而是精确到 `react@18.3.1`，连哈希值都写死了。

这是工程纪律，不是随意为之。我理解应该是保持渲染一致性？

**Tweaks 机制** ：内置参数调节面板。

用户可以实时改颜色、字体、间距，改动会持久化到文件里。这是一个完整的设计系统，不是一个一次性的生成器。

**双阶段验证** ：先用 `done` 命令检查控制台错误，确保用户看到的页面不崩溃。

然后调用 `fork_verifier_agent`，这是一个后台子代理，在独立 iframe 中截图检查布局问题。先修明显的 bug，再做细节审查。

**上下文压缩** ：`snip` 工具可以在对话过程中标记不再需要的上下文，防止 context 溢出。设计师做了很多探索性的尝试后，旧版本会被自动清理。

好设计不从零开始

这是整篇提示词中最重要的理念：

> "Good hi-fi designs do not start from scratch — they are rooted in existing design context."

提示词要求 AI 在开始设计前，必须花时间获取设计上下文，比如 UI Kit、设计系统、品牌文件、现有代码。

如果找不到，就去问用户要。

> "Mocking a full product from scratch is a LAST RESORT and will lead to poor design."

"从零开始 Mock 一个完整产品是最后手段，会导致最后的设计效果很垃圾。"

它还要求每次设计给出 **3 个以上的变体** ，从保守到创意，让用户混搭。

在动手之前，至少要问 **10 个问题** ，比如关于受众、风格、约束、期望。

这不是一个给我个 PPT的工具。这是一个有设计方法论的产品。

④ 隐藏的架构能力

还有一些藏在细节里的能力，比如完整导出链：PDF、PPTX、Canva、独立 HTML 文件

整个系统形成了一个闭环：获取上下文 → 生成设计 → 实时调整 → 导出交付。
```
    开源地址：https://github.com/elder-plinius/CL4R1T4S/blob/main/ANTHROPIC/Claude-Design-Sys-Prompt.txt
```

02

**真正的差距不在技术，在方法论**

大部分传统互联网公司的产品流程是这样的：PM 写 PRD → 设计出图 → 方案宣讲 → 排期 → 开发 → 测试 → 上线。

一个循环 2-4 周起步，最终等上线，可能被人家产品都迭代好几轮了。

Claude Code 的负责人 **Boris Cherny** 在 Lenny's Newsletter 的播客中透露，他的团队**不先写 PRD，而是先建几百个可运行的原型，然后挑值得发布的** 。

他个人每天合并 20-30 个 PR，同时跑 5 个 Claude 实例。

整个 Cowork 产品，大约 **10 天就做出来了** 。

而且，我感觉 Anthropic 组织内沟通的方式、产品迭代的方式一定也和普通科技公司很不一样。

比如每个人都有一个 Agent 和自己精心维护的属于自己的上下文。

人和人沟通很多都交给 Agent 之间的沟通（我的猜测）

在互联网公司待过就知道。

最恶心最累的不是 Coding 或出方案，而是变来变去的方向和拉各方对齐信息的过程。

Boris 在播客里提到他会故意给团队**不足的资金和无限的 token** ，逼迫他们用 AI 来放大产出，而不是堆人。

回到国内大厂。

大部分团队还在走 PRD → 设计 → 宣讲 → 排期 → 开发 → 测试的流水线。

一个功能从想法到上线，少则一个月，多则一个季度。

这不是技术问题。这是组织问题。

很多组织的惯性更大。

这些公司有充足的资金、顶尖的技术人才、成熟的用户基础，但它们缺的不是这些。

它们缺的是一种重塑**组织和重塑协作流程的勇气 。**

各个业务都在说 AI 转型，我觉得最重要是：如果这个业务负责人在 AI 上的思考和了解不是业界领先的，先要把之前的业务负责人干掉，这才是转型的第一步。

Anthropic 的核心竞争力不是 Claude Design 这个功能，也不是 Opus 4.7 这个模型。

是它可能已经找到了 **AI 时代产品迭代的方法论。**  

03

**

这个公众号历史发布过很多有趣的开源项目，如果你懒得翻文章一个个找，你直接关注微信公众号：逛逛 GitHub ，后台对话聊天就行了：
