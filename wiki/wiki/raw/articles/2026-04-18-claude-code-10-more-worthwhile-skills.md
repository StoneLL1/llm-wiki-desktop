---
title: "Claude Code 又十个最值得装的 Skills！这篇全补上"
url: "https://mp.weixin.qq.com/s/003XUxNONHU70MwkE3LuAg"
source: "微信公众号"
fetched: 2026-04-18
sha256: bb3087de601ad30d
---

上篇写了大概十个[让龙虾/OpenClaw越用越聪明！我都装了哪些技能Skill?](https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247514031&idx=1&sn=965c94da4fae011fa176c45d84febc80&scene=21#wechat_redirect)留言区有人问：还有吗？

有。

Skills 生态这半年扩张速度很快，很多方向上一篇根本没来得及覆盖。今天补齐另外十个，标准还是一样：真实可装、高频能用、明显提升结果。

**frontend-design：别让 AI 味毁了你的产品**

如果你做过一段时间的 Web 开发，你一定遇到过这个问题，让 Claude Code 写界面，出来的东西永远是 Inter 字体、紫色渐变、卡片布局、中规中矩。

技术上没问题，但是一眼能认出来是 AI 做的。

frontend-design 这个 skill 解决的就是这个问题。

它强迫 Claude Code 在写一行代码之前，先做一个承诺：你要选哪个视觉方向？极简主义、极繁主义、复古未来、editorial 风格？定了之后，所有的字体、配色、动效、空间布局都必须服从这个方向执行，不能在中途滑回安全区。

它不是让界面更好看，而是"让界面有立场"。

对任何在意用户侧体验的开发者来说，这个 skill 值得长期挂着。

安装：

`npx skills add anthropics/skills@frontend-design -g -y
`
```

**superpowers：让 Claude Code 真正跑项目**

很多人对 Claude Code 的误解，在于把它当作一个高级补全工具。

写几个函数，改几段逻辑，验证通过，完事。

但如果你想让它跑一个完整的需求，从想法到代码到测试到合并，那就不是补全工具能搞定的事了。

superpowers 做的就是这件事。它是社区里最完整的多 agent 开发工作流 skill 集合，把软件开发的完整生命周期拆成一串可衔接的步骤：需求发散、规格确认、计划拆分、子 agent 执行、代码 review、合并。每个环节都有专属 skill 负责，自动触发，自动接力。

它的核心优势在于，子 agent 驱动的方式防止了长任务里的上下文漂移；TDD 强制要求先写测试，再写实现；review 关卡让合并之前有人把关。

安装：

`npx skills add obra/superpowers@using-superpowers -g -y
`
```

**firecrawl 看懂网页数据**

Claude Code 默认状态下，它访问不了大多数真实网站。

原因是多方面的，网络限制、反爬机制、JavaScript 渲染的动态内容。

Firecrawl 它专门为 LLM 工作流设计，绕过反爬检测，渲染 JavaScript，然后把页面内容输出成干净的 Markdown，而不是你需要再次解析的 HTML 噪音。

接上这个 skill 之后，Claude Code 可以直接抓取任意网页内容、截图、提取结构化数据、批量爬取文档站点。

对于做技术选型调研、竞品分析、抓取 API 文档、自动化信息处理的人来说，这个能力不是锦上添花，是硬需求。

安装：

`npx skills add firecrawl/cli -g -y
`
```

**web-interface-guidelines，前端页面审审**

写前端页面代码最容易犯的错，不是逻辑错误，而是那些感觉没问题但实际有坑的 UI 细节。

ARIA 属性缺失、focus 状态看不见、input 没有 label、触摸目标太小、heading 层级乱了、键盘无法导航。

web-interface-guidelines 是 Vercel 维护的一个 quality gate skill。

它每次运行都从源仓库拉取最新的 Web Interface Guidelines，然后把你的界面代码对着 100+ 条规则逐条检查。

安装：

`npx skills add vercel-labs/web-interface-guidelines@web-interface-guidelines -g -y
`
```

**mcp-builder，让 Claude Code 帮你造工具**

用 Claude Code 的人，迟早会走到这一步：某个 API 没有现成 MCP server，或者现有的 server 不够好用，想自己造一个。

mcp-builder 这个 skill 是一套帮你构建高质量 MCP server 的完整指南，封装了架构决策、工具命名规范、错误处理模式、调试方式。

安装：

`npx skills add anthropics/skills@mcp-builder -g -y
`
```

**remotion-best-practices：视频就是代码**

很多开发者做产品发布，最怵的一件事是录 demo 视频。

不是因为不会用 ScreenFlow，而是因为它打断了开发节奏。你要切出去、录制、剪辑、导出，整个流程和写代码完全不在一个频道上。所以大多数人选择不做，或者做一个将就的版本。

Remotion 把这件事变成了写 React 组件。动画是随帧变化的状态，时间轴是代码逻辑，导出是一条命令。

它是 Remotion 官方维护的 skill，把正确的动画曲线、音频处理、字幕生成、3D 集成方式都封装进来了。

没有这个 skill，Claude Code 写 Remotion 代码会反复在插值、音频裁剪这些地方出错。加上之后，它的输出质量完全不同。

如果你有程序化视频的需求，比如 changelog 演示、API demo、数据可视化，这个 skill 值得装。

安装：

`npx skills add remotion-dev/skills@remotion-best-practices -g -y
`
```

**pr-review / git-pr**

Claude Code 会写代码，但不代表它默认知道怎么在 PR 流程里正确参与。

最常见的问题是在 review 的时候逐条发评论，把 PR 作者的通知轰炸成垃圾桶；或者绕过 pending review 模式，直接 submit；或者不给用户确认机会，就自己把东西发出去。

这类 skill 专门解决这些问题。它强制 Claude Code 先检查 gh CLI 是否就绪，起草所有评论，展示给你看，等你确认之后再统一提交，使用 batch review 而不是逐条轰炸。

参考入口：

https://skills.sh/aidankinzett/claude-git-pr-skill/claude-git-pr

安装：

`npx skills add aidankinzett/claude-git-pr-skill@claude-git-pr -g -y
`
```

**gws，Google 生态的统一入口**

Google Workspace 有五十多个 API。Gmail、Drive、Calendar、Docs、Sheets、Slides，每一个都有自己的 OAuth 流程、client library、REST 端点。

对于大量工作流程依赖 Google 生态的人，比如读邮件归档、同步日历、整理 Drive 文件、操作 Sheets 数据，这个 skill 把所有全部自动化。

安装：

`npx skills add googleworkspace/cli@gws-docs -g -y
`
```

**simplify / code-review**

这是一个经常被人低估的内置 skill，但它是我用得最多的之一。

Claude Code 写出来的代码，通常是"能用的第一稿"。能跑、逻辑对，但不代表是经过整理之后的状态。未使用的 import、冗余变量、可以提取的共用逻辑、过于复杂的条件判断，这些东西散落在里面。

/simplify 会在你的改动之后自动跑三个并行的 review agent，每个从不同角度审查文件，然后直接修掉问题，不只是报告。

它不需要你去主动学什么新东西，也不需要配置。你只需要养成一个习惯：每次大的修改之后，或者 AI 一口气生成了一批代码之后，跑一次 /simplify。

它是把"AI 生成"和"人工整理"之间那段距离缩短的东西。

使用方式：

`/simplify
`
```

或带参数：

`/simplify 只关注测试覆盖缺口
`
```

**project-context 龙虾记忆**

这个最后说，但可能是长期价值最高的一类。

Claude Code 没有跨会话记忆。每次你开一个新会话，它不知道你的项目叫什么、用的什么框架、哪些模块是核心、哪些坑之前踩过。

[抄一下OpenClaw！写了个跨端的Claude Code记忆工具Skill](https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247513344&idx=1&sn=6e26257c9ff0abf776d01364b1b045ee&scene=21#wechat_redirect)

OpenClaw和CC在使用Skill上没有什么特别的区别，如果你刚开始配，两篇合起来看，先装基础能力那一批，再按你实际工作场景补齐。

Claude Code 的上限，从来不是模型本身决定的。

你给它配了什么，它就能走多远。

[实现基于微信OpenClaw插件的主动消息推送](https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247514539&idx=1&sn=0c551c01ca14d0d7eb214ec6e89495a1&payreadticket=HMkaf6tumAOjSc0DgbOtbN1XUuT9_UUvKP2_vwEXxpyQoL14zuDgzGJ6Gp1fBEeBhkypKFk&scene=21#wechat_redirect)