---
title: "OpenAI 官方分享：如何榨干 Codex"
url: "https://mp.weixin.qq.com/s/tFcvY4Pgfq2P3qoc_7jlVA"
source: "微信公众号"
author: "Jason Liu"
account: "AGI Hunt"
fetched: 2026-05-27
sha256: cf731376627e2364
image_count: 19
---

前两天，OpenAI 内部的一位工程师 Jason Liu 发了一篇长文，**Getting the most out of Codex** （如何把 Codex 榨干）。  

![Getting the most out of Codex](https://mmbiz.qpic.cn/mmbiz_jpg/ZKqVLiaIpzFnfm9z00Ziaqcy4ictPL847AcXyicv56H8FspHkhZbib6OaH2zLQibOPuX2NashQWo9v0xY4HIbVj4TTicG0G6kRMs6mmYVeAAicRbZW8/640?from=appmsg)Getting the most out of Codex

算是官方下场，手把手教你怎么把 Codex 的能力压榨到极限。

01

## 关于作者

Jason Liu（@jxnlco），目前是 **OpenAI Codex 团队的开发者体验工程师** 。

![Jason Liu](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFkrWQGrebdgA3zG2IPzDOY5bmlnXMicjc3M9ghib71gfbt0uLvekmW6Al1W2ZM8ToFjDKrjzu0KhCZFlzoqzLib0icNVdcQA5njA6I/640?from=appmsg)Jason Liu

他出生在中国北方，靠近蒙古草原的边境地带，自称是「北方华裔蒙古人」。从小在加拿大长大，在安大略省的一所公立艺术学校读了四年高中，学数字动画和设计。本科考进了滑铁卢大学读计算数学和统计，最早学的其实是数学物理，后来才转向计算机。

他职业生涯的主要轨迹是：Meta 做内容审核算法，然后去了 Stitch Fix（美国一家时尚电商）做了五年机器学习，一路做到 Staff Engineer。在 Stitch Fix 的时候，他搞了一套多模态嵌入系统（ResNet-50、CLIP+GPT-3），还开发了个叫 Flight 的内部框架，每天处理 3.5 亿次请求，内部采用率 80%。

离开 Stitch Fix 之后，他创办了 567 Studios 做独立咨询，客户包括 Zapier、HubSpot、Weights & Biases、Pydantic 这些公司。同时还在 Maven 上开课教 RAG 和 AI Agent，学员来自 OpenAI、Anthropic、Google、微软等 50 多家公司。

![职业轨迹](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFl0Wc36iagKqhtxEibRZLTeiasQjekLOvuCeXwW6TQgpAPS7FdS1HYS6cQicaVbPAbJ0HD1NyyoLcsvuTRgPlicyW0Ug9CJUx99aEKw/640?from=appmsg)职业轨迹

不过他最广为人知的身份，可能还是开源项目 **Instructor** 的作者。

这个库有 1.3 万 GitHub 星标，月下载量超过 600 万，能用 Pydantic 从 LLM 输出中提取结构化数据。OpenAI 官方后来推出的 Structured Outputs 功能，明确表示受到了 Instructor 的启发。

他此前曾发过一条推，调侃在 OpenAI 的经历：

> “ 我申请 OpenAI 的时候，以为自己会做 evals。签合同的时候，以为会做 agents。入职的时候，以为会做 Codex。工作一个月后，以为会做知识工作。结果现在……我在做动态图形。

总之，这位在开发者工具和 AI 应用领域浸泡了快十年的人，现在专门负责 Codex 的开发者体验。他基于内部视角对外写的这篇指南，值得我们一读。

02

## 持久线程

文章开头提出了一个核心概念：**Durable Threads（持久线程）** 。

Codex 的线程不是一次性的短对话，它是一个**持久化的工作空间** ，关掉再打开，之前的决策、偏好和工作上下文都还在，不需要从头来过。

Jason 建议用户把不同类型的工作分配到不同的固定线程中：

•  **首席幕僚线程** ，处理日常杂务、收发邮件、安排优先级 

•  **发布管理线程** ，追踪版本发布进度 

•  **文档审查线程** ，持续审核和更新文档 

•  **外部监控线程** ，跟踪外部信息变化 

![持久线程](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlxcvXZ1GiaCF8IWn9xZxT3Ljl6mC8kX9wb6hVKGFpQdEPGicb8NGiaM3vvqT6sf150SibxliaCbibQvVH5oubNMwQzVmnLOCnzhOHHw/640?from=appmsg)持久线程

用 `Command-1` 到 `Command-9` 快捷键可以直接跳到对应线程，把它们当作**常驻工作台** 来用。

这和 Claude Code 的 memory 系统有异曲同工之处，只是 Codex 选择了一条更「显式」的路：你自己决定哪些上下文需要保留，而不是让模型自动记忆。

03

## 语音输入

语音输入我一直就在使用，叫：[我做了一个 AI 时代的效率神器，已开源](<https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481122&idx=1&sn=d5650b5cd406dee007b4fa243fa394fe&scene=21#wechat_redirect>)。语音输入这个功能乍一看可能平平无奇，但用过之后你就再也回不去了。

Jason 分享的用法是：在想法还没成形的时候，先用语音把粗糙的念头倒出来。

比如这样说一句：

> “ 我记得有个叫 Ben 的人在 Slack 里提了这个事，具体细节我忘了，你去找找。

![语音输入](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFk2AcvFdHt2tLvdPNcK14xkLoaNSmjlNrvwhP0mDQfnzOZcEtwdrXBg0iaW6gnvJiaicQbeWykwFpMnUh9E6eqbcf24tnp7Rs9wks/640?from=appmsg)语音输入

语音的好处在于，它保留了思考中的不确定性和强调重点。两三分钟的语音倾倒，比花五分钟写一段精确的 prompt 效率要高许多。

而且原始的语音转录稿（包括犹豫、强调、没说完的半句话）比整理过的摘要信息量更大。开会的时候直接把会议录音喂给 Codex，比自己写会议纪要效率要高得多。

04

## 实时干预

这部分 Jason 提出了两个控制机制，解决的是同一个问题：**人不需要等 Agent 做完才能参与** 。

**Steering（实时干预）** ，在 Agent 执行过程中随时打断纠正。比如看着它做网页，直接说「这个间距不对」「这段文案不对」。不用等它做完再推翻重来。

![实时干预](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkrfAAlo7OP4bZibBjHcP5v4hFJZzhPXRO21GQTzy0MAxejUgh05cysJNPyWRd21hkmAtE1Vda7P4HEAQRL6KU8XIuqicibUUkXpk/640?from=appmsg)实时干预

**Queuing（任务排队）** ，不打断当前任务，直接追加后续指令：「做完之后，把预览链接发给 Slack 里的审阅者。」

一个是纠偏，一个是追加。两者配合的效果是：你可以一边看着 Agent 干活，一边实时调整方向，同时把后续任务排好队。整个过程不需要停下来重新写 prompt。

05

## 从编程到万能

接下来是 Codex 能力边界的扩展，这部分是整篇文章最关键的信息。

Jason 把 Codex 的操作范围分成了几个层次：

•  **内置浏览器** ，在侧边栏中检查和标注网页 

•  **Chrome 级工作流** ，使用你已登录的浏览器状态，处理需要身份验证的操作 

•  **桌面 GUI** ，操作那些只有图形界面的应用 

•  **MCP 服务器和连接器** ，把能力延伸到更广泛的工作流中 

![从编程到万能](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFngpqZiceyoq82iaCQqnGBuck4JTNB9lsPy00oty4SzqtghrBGqpoQ323nm9j0ibicOJ32xTHYvub4C1MaHwRRUeXbWE09KKON1Rlg/640?from=appmsg)从编程到万能

也就是说……Codex 已经不只是个写代码的工具了。

它可以帮你看 Slack、查 Gmail、操作 Google Docs，甚至在你的电脑桌面上点来点去。Jason 在文中的原话是：

> “ 从指令到执行到产物审查，即便工作已经超出了代码仓库的范围。

06

## 技能和云端

**Skills（技能）** 的概念和 Claude Code 的 Skills 有些类似：把验证过的工作流封装成可复用的模块，下次直接调用，不需要重新教一遍。

![技能和云端](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFn4O0qZ5aeRfbHStD7XichR2wpaUNzFQSaRBfzz6TDtaaoSdNBLSYkPoHcUdwXoDXJwmwYsyQ2MOibiaUpicmx5VG6Xe3vDlRcvIZI/640?from=appmsg)技能和云端

另一个值得关注的是 **Cloud Context（云端上下文）** 。Codex 的任务可以从电脑上启动，然后在手机上继续跟进。你可以离开工位，让 Codex 在后台跑更长的任务，随时从手机上审批下一步、或者重新调整方向。

也就是说，Codex 不只是一个绑定在本地终端上的工具，它的工作状态是跟着账号走的。

07

## 两种自动化

Codex 区分了两种自动化模式：

**定时自动化（Scheduled Automations）** ，按时间表运行，每次从头开始。比如每天早上生成一份日报，或者定时检查某个仓库的状态。

**线程自动化（Thread Automations）** ，在同一个线程中定时唤醒，带着之前的上下文继续工作。

Jason 举了个例子：

> “ 每 30 分钟检查一次 Slack 和 Gmail，找到需要我关注的未回复消息。帮我排出优先级。如果有人问了我一个问题，尽可能深入地研究答案，帮我起草回复，但不要发送。

这就是个**全天候在线的私人助理** 。

你离开电脑，它在后台帮你收集信息、整理回复、跟踪 PR 评论。等你回来，昂贵的上下文收集工作已经做完了，你只需要审阅和确认。

![两种自动化](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFlqrrXqwAN62VBfl01H7Zf81miaUia1TA4dNZuar8E6de8J5TW88DAgg0WAdY8aEY2qQY5bbq9I63550TYMqp0pDXKsDLSmUhWjY/640?from=appmsg)两种自动化

线程自动化还有一个用法，是用来做**反馈循环** 。比如让它持续关注 PR 评论、Google Docs 里的批注、或者 Slack 频道的回复，在你离开的期间保持工作推进。

这个设计的核心洞察是：**Agent 最有价值的能力，不在于它能替你做什么，而在于它能替你等什么。**

08

## 目标驱动

`/goal` 是 Codex 上个月推出的新功能，我之前写过一篇文章专门介绍，[Codex 推出 /goal 功能，不达目标，不罢休](<https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453483290&idx=1&sn=904eb46992f4d152d712fd963e274f9f&scene=21#wechat_redirect>)

简单来说，Goal 就是给 Agent 设定一个明确的终点线，让它自己跑到终点。

Jason 在文中区分了「弱目标」和「强目标」：

弱目标：「按照这个 Markdown 文件里的计划实现。」

强目标：把一个 Python 项目迁移到 Rust，用单元测试作为成功标准。

![目标驱动](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkA7M7xqInVDf2E6nz7icXsEplBOr6cxiadjRicxNiaUhfVPuosDTcFAX2wANRZR5Oaibd6OjoL6zasfXwo76wmoDmohWu9KvK7taAo/640?from=appmsg)目标驱动

好的目标需要配套**验证机制** ：

•  测试套件 

•  基准测试 

•  Bug 复现步骤 

•  端到端工作流 

Jason 的总结是：

> “ 野心当然重要，但没有验证机制，它终归只是个愿望。

09

## 侧边栏

侧边栏（Side Panel）承担了四个功能：检查产物、标注修改、操作网页、审查代码变更。

![Codex 侧边栏，CSV 文件](https://mmbiz.qpic.cn/mmbiz_jpg/ZKqVLiaIpzFn0xQ6XTtgh7KRq5ibSoczLZHcB09hibN3JETDYuicCXvTq1I6FM2wWHobInA61eJUJZ6ib8nvIALCgnIjmP2F0UqwbbHB39dRm3JE/640?from=appmsg)Codex 侧边栏，CSV 文件

它支持的格式包括 Markdown、电子表格、数据表、文档、幻灯片、代码、PDF 等等。

Jason 特别推荐了几种适合在侧边栏中使用的产物格式：

•  **index.html** ，轻量级静态产物，不需要服务器 

•  **Storybook** ，UI 组件审查 

•  **Remotion Studio** ，程序化动画 

•  **浏览器幻灯片** ，演示文稿 

•  **数据应用** ，分析工作流 

![Codex 侧边栏，PDF 和标注](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFlcxPVpbqx4ZuGKJATHdlaoic9tNnSSTm3Xp7NCY0C1lbCQnVzmHBKsngejStic8kJXrZKbNtiapSkIPibxV0vwPictEwrUr2XFpcicg/640?from=appmsg)Codex 侧边栏，PDF 和标注

一个单独的 `index.html` 文件就能创建出不需要服务器的交互式产物。配合线程自动化，还能定时刷新这些静态产物，让你每次回到线程时都能看到最新内容。

![Codex 侧边栏，幻灯片](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFlj4w18TnK29EGC31jtPUM1CcIuEr2dIua4SxXEvIVJjJia4xj5VeYsI7r8LicZ3ficI8mWtD7PpQOhHuyKvjqCFia6a9VhRLUkxCo/640?from=appmsg)Codex 侧边栏，幻灯片

而且你可以直接在侧边栏的渲染界面上做标注，标注会留在工作循环中，不会变成单独的「交接文档」。

10

## 共享记忆

这部分和 Claude Code 的 memory 系统有些类似。

Jason 的建议是：**重要的上下文不应该只存在对话记录里** ，要写到一个 Agent 下次能找到的地方。

他推荐的方案是用一个 Obsidian 知识库来存储持久信息：
```
    ●●●
    
    vault/  
    ├── TODO.md  
    ├── people/  
    ├── projects/  
    ├── agent/  
    └── notes/  
    
    
    └
```

![知识库](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFl4D9XpFiboXp2p5cAgrNeYItb1to9xQuUPkb7ibJdu2AqzUmVUjmn5wEWemOGROkuEV0s0yO4rR7djSGkribB6nia4nx5HVTVebPM/640?from=appmsg)知识库

然后在 `AGENTS.md` 里告诉 Codex 怎么使用这个知识库：

> “ 把 ~/vault 当作持久工作记忆。优先更新已有笔记，而不是到处创建新文件。按照 TODO、人物、项目、每日摘要和草稿分类。保留决策、阻塞项、负责人、日期和有用链接。如果没有有意义的变化，就不要动知识库。

知识库可以放在云存储、Git、Dropbox、Google Drive 等任何同步工具上。代码归代码仓库，**滚动的工作上下文归知识库** 。

除了外部知识库，Codex 还有一套**内置记忆系统** （Settings > Personalization > Memories），用于记录偏好、常用工作流和已知的坑。配合屏幕上下文捕获，Codex 可以从你最近的操作中自动建立记忆。

11

## 全部串起来

把上面这些功能组合在一起，Jason 描绘的是这样一个工作模型：

**实时干预** 打断正在走歪的方向，**任务排队** 在不打断的前提下追加工作，**线程自动化** 在你不在的时候保持进度，**目标** 设定明确的终点线。

![指挥中心](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFm61pec26OLuSCXiazCq21xATDiaqARic2NotZDHnzeGrWUsQHibY4bFiaxzkG8o6HJzZ6TaiaJNicfd5hzbhbx7rdy5GJLBFuW2nELgg/640?from=appmsg)指挥中心

从「给它一段 prompt 让它写代码」，到「管理一个持续运行、跨仓库、跨应用、自动化的工作系统」……

Codex 能做的和想要做的，显然已经超出了编程工具的范畴。

12

## 另一种 open

回看 Codex 刚发布的时候……，如果你也有用过就会知道，那会儿真的是极其难用。不论是模型在编程上的能力，还是 Harness（编排层）的设计和实现，相比 Claude Code 差距都不小。

但 OpenAI 正视到了这个差距，并一直在快速追赶。

过去的两个月，Codex 的更新节奏更是密到让人目不暇接，除了先是全线对标 Claude Code 到部分超过 Claude Code（尤其像多模态生成这样的 claude 完全忽视的领域），甚至还搞了个 **Codex 插件** 直接做进了 Claude Code 里，直杀入对手腹地……

甚至便是说 Codex 已经超过了 Claude Code，也并不算夸张。只是 Claude Code 在生态和用户心智上还是有先发优势的，社区更成熟，开发者的肌肉记忆更深。

我自己就……有时明明想用 cx（codex 的 alias），结果还是习惯性地输入了 cc（Claude Code 的 alias）……

而且在使用上，OpenAI 对国内用户相对更为友好。同时它也比较开放，对于社区（比如龙和马）一直敞开大门，还时常一言不合就干出重置用量这种事。

![另一种 open](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkxFWjc59W9w3fYycGp06YIBErNCicSn9d8HXL3OV0IBRiauCEulRuArtQ2gIJP5F8CKUxKicfvMeHiaRs1j2yxZc25UaA00gGXBZQ/640?from=appmsg)另一种 open

虽然模型不 open，但这个 open 也不错……

而现在，官方亲自下场，教你怎么把 Codex 的每一个 token 都压榨完全来了。

**那就充好钱、好好学，暴力压榨起来吧！**

◇ ◆ ◇

相关链接：

https://x.com/jxnlco/status/2057153744630890620
