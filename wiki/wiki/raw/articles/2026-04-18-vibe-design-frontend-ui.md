---
title: "Vibe Design来了！通过DESIGN.md搞定前端UI设计"
url: "https://mp.weixin.qq.com/s/SBSY_cbwn3xCiIon_XOZbQ"
source: "mp.weixin.qq.com"
fetched: 2026-04-18
sha256: d7e108bd2f28bcd1
---

大家好，我是鲁工。

对于前端项目，使用Vibe Coding做出来的UI落地页面，通常不忍直视。  


功能能用，但看着像是2015年的Bootstrap模板拼出来的。配色要么太素要么太艳，间距全靠AI自由发挥，组件风格不统一，整个页面如同小红书上的AI美女一样，让人有一种一眼AI的感觉。

那种半蓝半紫的土味配色，想必每个Viber都有亲身感受，没有比丑更恰当的形容词了。

比如下面这种：

这其实是Vibe Coding一个老问题了。审美这块可以说一直是各大coding模型的一个短板。之前主要通过官方的/frontend-design skill或者直接提示词约束来做审美和设计提升，比如我之前常用的：

> Generate a $100K AI Prompt Generator SaaS website frontend, fully responsive, do not use Tailwind CSS, give me a code preview.

上面那个土味UI经过这个提示词优化之后，就会摇身一变：

看起来高端了不少，也差不多能摆脱土味设计。但这种方法稳定性和可控性难以把握，虽然大多数情况下能出来不错的landing page，但有时候生成的页面并不是我们心目中想要的那种设计。

那么问题出在哪呢？其实不难想。我们让AI写代码的时候，CLAUDE.md告诉它代码怎么组织、什么规范要遵守。但有没有告诉它，UI应该长什么样？配色用什么色系？按钮圆角多大？标题用什么字体？间距规则是什么？

大多数人没有。因为没有一个方便的方式来表达这些设计意图。

Figma文件AI读起来困难，设计稿截图信息量有限，JSON格式的design token太碎片化。直接在prompt里写一大段"我想要深色背景、蓝色强调色、卡片式布局、8px间距体系"，写完AI也不一定能完全做到指令遵循。

直到我发现了DESIGN.md这么个东西。

看名字就知道这是一个Markdown文件，用纯文本把一个网站的完整视觉设计系统写清楚。我昨天在：

[Markdown，正在成为Agent时代...](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485402&idx=1&sn=0a2a473b5fe6f410a55c77a34d8f9114&scene=21#wechat_redirect)

谈到markdown正在成为Agent时代第一文件格式，DESIGN.md就是一个典型代表。

颜色用什么、排版什么规则、按钮长什么样、卡片怎么画、间距体系是什么、深色模式怎么处理，全部写在一个md文件里。你把它丢到项目根目录，AI编程工具启动时就会读它，然后生成的UI就会按照这套设计规范来。

这个概念最早是Google的Stitch带出来的。今年3月Google Labs发布了Stitch，一个AI原生的UI设计平台，内置了Gemini 3.1系列模型。Stitch的核心思路叫Vibe Design（是的，Vibe家族新概念）。核心思路就是你不需要手动画线框图、拉组件、调参数，而是告诉AI你想要什么感觉的界面，AI帮你生成高保真UI。

Stitch有个很实用的功能：它能把你的设计系统导出为DESIGN.md文件。这意味着你在Stitch里调好的设计语言，可以直接带到Claude Code、Cursor这些编程工具里用。

但真正让DESIGN.md火起来的，是GitHub上一个叫awesome-design-md的开源仓库。地址：

https://github.com/VoltAgent/awesome-design-md/

awesome-design-md这个仓库是VoltAgent团队做的，今年3月底发布，到现在已经有3.7w多颗Star，4700多个Fork。

它做了一件事：把58个知名网站的设计系统，全部提取成DESIGN.md文件。

覆盖范围非常广。AI类的有Claude、Mistral、ElevenLabs、xAI。开发工具类的有Linear、Cursor、Vercel、Supabase、Sentry。设计工具有Figma、Framer、Notion、Webflow。企业消费品牌有Apple、Spotify、Airbnb、Uber。甚至还有汽车品牌，BMW、Ferrari、Tesla、Lamborghini。

每个DESIGN.md文件包含9个标准模块：视觉主题与氛围、色彩体系与角色定义、排版规则、组件样式（按钮、卡片、输入框、导航）、布局原则（间距、网格、留白）、层次与阴影、设计的"该做"和"不该做"、响应式行为，以及一个Agent提示指南。

最后这个Agent提示指南是关键，它用自然语言告诉AI：在生成UI时，应该优先关注什么，避免什么，如何在不同场景下做设计决策。等于给AI写了一份设计方面的系统提示词。

用法也极简：把某个网站的DESIGN.md复制到你的项目根目录，告诉AI"按照这个设计规范生成页面"，就完了。不需要装插件，不需要配置design token，不需要Figma权限，一个md文件搞定。

这也是它Star涨得这么快的原因。开发者太需要一个低门槛的方式来解决前端审美问题了。

说了这么多，我简单实测了一下。

测试方案很简单：用Claude Code生成一个医疗项目管理看板页面，先不加DESIGN.md，看默认效果。然后加上Linear风格的DESIGN.md，用同样的功能需求再生成一次，对比差异。

选Linear是因为它在开发者圈子里公认设计好看，深色主题、清爽排版、克制配色，很适合做项目管理类的数据看板页面。

第一次不加DESIGN.md，直接让Claude Code生成一个项目管理看板。出来的结果，怎么说呢，功能完整，白底黑字，卡片有基本的阴影，但整体风格非常"默认"。间距偏大，颜色偏素，图表配色是Tailwind默认色，总体就是平淡普通。能用，但你不会想把它给别人看。

然后我把Linear的DESIGN.md丢到项目根目录，告诉Claude Code"根据DESIGN.md的设计规范重新生成这个页面"。

在保留Linear整体设计的前提下，我把看板背景色由深灰蓝色调改回了白色，更符合项目管理看板页面习惯一点。数据卡片有了微妙的边框和阴影层次，hover时有轻微的亮度提升。字体层级清晰，标题、数据、标签用了不同的大小和粗细。间距也规整了不少，卡片之间的间距统一，内边距一致。并且有更多的细节设计体现。

  


可能在这个例子上差别不是那么明显。后续有更好的例子我会专门再分享出来。

  


  


整体的一个感受是：DESIGN.md对AI生成UI的约束力，比我预想的要强。它不只是告诉AI用什么颜色，而是给了一整套设计决策框架，让AI在生成每一个组件时都有规则可循。

设计系统这个东西，传统上要么存在Figma的设计库里，要么用JSON格式的design token，要么写在内部wiki上。这些格式对人来说都还行，但对AI来说很不友好。Figma文件AI不好读取，JSON太碎片化缺少上下文，wiki格式不统一。

Markdown是目前LLM最能理解的文档格式。它有结构但不死板，有层级但不复杂，对人可读对机器可解析。你在DESIGN.md里写"主按钮用#6366F1，圆角8px，hover时亮度提升5%"，AI能精确理解并执行。你写"整体氛围偏沉稳专业，避免过于鲜艳的配色"，AI也能理解这种模糊的设计意图。

这恰好是md文件的优势所在：它可以同时承载精确的参数（色值、间距、字号）和模糊的指引（氛围、风格、原则），而AI两种都能处理。

Google推Stitch的时候用了Vibe Design这个概念，这就很精准。

过去一年Vibe Coding解决了"不会写代码也能做产品"的问题，但没解决"做出来的产品好不好看"的问题。Vibe Design就是来补这块的。你不需要系统学过设计，不需要会用Figma，甚至不需要能讲清楚什么是"视觉层次"。你只需要有审美直觉，知道自己想要什么感觉，然后用一个DESIGN.md来锚定这个感觉。

Stitch目前免费用（Google Labs阶段），每月350次Flash生成额度、50次Pro生成额度，可以导出到Figma或直接输出前端代码。v0、Lovable这些工具也在不同程度上做Vibe Design的事。但我个人觉得，DESIGN.md这个思路可能是最实用的，因为它跟具体工具无关。你用Claude Code也行，用Cursor也行，用Gemini CLI也行，只要工具能读md文件就行。

awesome-design-md仓库3.7w多Star的热度也说明了一件事：开发者社区对"低门槛提升前端审美"这件事有非常强烈的需求。大家缺的从来不是好看的参考，而是一种让AI能理解并执行设计意图的标准化方式。DESIGN.md恰好填上了这个空缺。

如果你也被Vibe Coding的前端审美困扰过，个人推荐试试DESIGN.md这条路。

  


**参考资料**

  1. Design UI using AI with Stitch from Google Labs, Google Blog: https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-ai-ui-design/
  2. awesome-design-md, VoltAgent: https://github.com/VoltAgent/awesome-design-md
  3. Google Just Introduced "Vibe Design", Muzli Blog:https://muz.li/blog/google-just-introduced-vibe-design-heres-what-it-means-for-ui-designers/

  


如果觉得有用，点个赞或者在看，也方便更多朋友看到。

我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道。感兴趣的朋友也可以加我微信（louwill26_）交个朋友。

>/ 作者：鲁工
