---
title: "Claude Design发布后，留给AI设计厂商的时间不多了"
url: "https://mp.weixin.qq.com/s/Q7mVF27HoKiCQhO0M92aXA"
source: "微信公众号"
author: "鲁工"
fetched: 2026-04-19
status: "success"
sha256: 712dd89b11a04515
---

# Claude Design发布后，留给AI设计厂商的时间不多了

**作者**: 鲁工（AI编程实验室） | 2026年4月19日

大家好，我是鲁工。

AI设计领域，最近迎来了一次不小的震动。前两天，Anthropic发了个新产品叫Claude Design，直接把Adobe、Figma、Wix等设计赛道的头部玩具股价集体干跳水。

怎么说呢。这事挺有意思的，因为刚好卡在我最近连着写的几篇文章的线头上。

前天我也写一篇Lovart的四点重大功能更新，讲的是AI设计垂直赛道头部玩家Lovart的新功能实测。周一我写了一篇Figma vs Pencil，聊的是怎么把设计工具接到Claude Code里。再往前一篇是DESIGN.md，讲怎么用一个设计规范文件约束AI生成的前端UI不那么丑。

这三篇写完我自己心里其实挺清楚一件事。社区里现在都在绕着一个问题打转：Vibe Coding这套流程里，前端UI那一截始终是个断层。代码生成越来越靠谱了，但做出来的东西好不好看，基本靠玄学。

社区里的解法大致三条。约束AI（DESIGN.md），把设计工具接进来（Figma MCP、Pencil），或者先用AI做完设计再人工搬（Lovart）。三条路各有各的好处，也各有各的短板。

但我真没想到的是，Anthropic这次直接自己下场了。而且切入的角度，和上面这几条完全不一样。

## Claude Design简介

先快速讲一下产品本身。

Claude Design是Anthropic Labs下的新产品，可以搭配最新发布的Claude Opus 4.7模型进行构建，这是Anthropic目前视觉能力最强的模型。产品定位是对话式设计协作，你用文字描述需求，Claude给你画出第一版，然后通过聊天、行内注释、直接拖拽，或者Claude为你生成的滑块（官方叫adjustment knobs）一步步调。

输入端很开放。纯文字prompt、上传图片、DOCX/PPTX/XLSX这类办公文档，或者直接把Claude指向你的代码仓库。还有一个叫web capture的工具，可以从现有网站上抓取视觉元素，让做出来的原型和你的产品保持一致。

输出端更值得说一下。设计稿可以作为组织内的URL分享、打包成文件夹下载，也能导出为PDF、PPTX、独立HTML文件。最有意思的是和Canva做了深度集成，一键可以把草稿推到Canva里继续编辑和发布。

支持的使用场景官方列了六类：交互原型、产品线框图、设计探索、pitch deck、营销物料，还有一个叫Frontier Design的东西，允许你调用语音、视频、shader、3D和内置AI去做偏实验性质的创作。

订阅覆盖Pro、Max、Team、Enterprise，目前是Research Preview阶段，不过付费用户都能体验。

## Claude Design和之前几条路线有什么不一样

Claude Design发布之前，我一直以为Anthropic不会自己做设计工具。

毕竟Figma上个月刚发布了Code to Canvas，和Anthropic联名推的，思路是让Claude Code生成的代码能直接倒流回Figma画布变成可编辑图层。这个合作看着就很像"你管设计、我管代码"的泾渭分明。

Anthropic的首席产品官2026年初才退出Figma董事会，节奏上刚好对得上。现在回头看，这步棋的意思就很明显了。

真正让Claude Design和之前几条路线区别开来的，是切入角度。

DESIGN.md走的是文本约束路线，便宜好用，但它本质上还是给AI一个设计规范，具体怎么画基本还是AI自由发挥。Figma MCP和Pencil走的是把设计工具接入代码生成链路，适合已经有设计稿的团队。Lovart解决的是品牌设计这类相对独立的创作场景。

这几条路线有个共性：它们都默认设计这件事应该在专门的设计工具里完成，然后再想办法把结果导给Claude Code。

Claude Design刚好反过来了。设计起点直接搬进对话窗口。Claude Opus 4.7读你的代码库和设计文件，onboarding阶段自动推出一套属于你团队的设计系统，后续不管做原型、做PPT还是做落地页，都在这一个对话里完成。最后一步才是把设计打包交给Claude Code去实现。

用一句话形容，就是把设计到开发这条长链路，从多工具串联压缩成单对话完成。

讲到这儿，这个产品里我最在意的功能叫Handoff to Claude Code，意思就是把设计好的东西交接给Claude Code来实现。这一步才是原生态的Vibe Design。

以前做一个从原型到可跑代码的流程，大致是这样走的。设计师在Figma里画稿，用Dev Mode导出CSS参数，前端根据这些参数写代码，过程中不断回去和设计师对像素。就算有了Figma MCP，AI能读结构化的设计数据，但一个稍复杂点的Figma页面，MCP Server返回的JSON载荷能到5MB以上，上下文窗口直接就被噪声塞满了。

Claude Design这次做的是打包工作。你在Claude Design里把设计敲定，在导出中选择Handoff to Claude Code，它会把整套东西（布局语义、组件层级、设计意图）打成一个Bundle，然后你把这个Bundle丢给Claude Code，一句话就能让它开始写代码。

官方也给了Brilliant的案例。Brilliant是做在线教育的，他们的产品页面包含大量复杂交互和动画。按照他们资深产品设计师的说法，那些页面在其他工具里需要20多次prompt才能还原出来，在Claude Design里只用了2次。

## 对AI设计赛道的震动

消息出来后，Adobe、Figma、Wix的股价在开盘前就跌了。The Information提前泄露Anthropic本周要发Opus 4.7加AI设计工具，这几家软件股最近一周都有点难。

但这里头有个插曲。

Figma自己最近日子不太好过。2025年7月IPO，首日暴涨250%，但后来一路跌到24美元上下，距离最高点142美元跌了超过80%。主要背景就是市场担心AI编码工具（尤其是Claude Code）会让设计工作过时。

Figma的CEO Dylan Field的应对方式挺务实，选择主动拥抱AI。Code to Canvas就是他们和Anthropic合作的第一步。2026年2月18日Figma发Q4财报，营收3.03亿美元同比增40%，净留存率136%，盘后直接拉升15%。管理层讲的故事是AI不会取代Figma，AI正在通过Figma设计。

现在Claude Design发布，这个叙事往哪边走就很难说了。

从产品逻辑看，Claude Design和Figma目前也不完全是零和。Claude Design没有真正的画布，没有精确到像素的矢量编辑，没有Figma那种多人光标，也没有Figma十几年积累的插件生态。它擅长的是设计前置阶段，就是PM和创始人写Notion文档给设计师翻译、然后设计师画完再交给工程师翻译回代码这个流程。Claude Design把这一段压缩掉了。

但真正的专业设计师、精细化的设计协作场景，Figma的护城河短期内可能还在。

这也是我想多说一句的地方。

Claude Design的切入点其实不是专业设计师。Anthropic官方原话说得很清楚，是给那些没有设计背景的创始人和PM用的。

Claude Design相当于把这个门槛直接干掉。你描述需求，它给你画，画完觉得哪里不对就直接说"按钮再大一点"、"整体风格更活泼一点"，然后点Handoff甩给Claude Code实现。一个人能把过去需要PM加设计师加前端三个人的活儿干完。

这个意义比替代Figma大得多。

专业设计师目前受到的威胁其实还不算大。你要的那种精细控制、多人协作、设计规范管理，它暂时都给不了。但它会蚕食你工作里那一块"帮非设计背景同事画个糙稿"的时间。

如果你是PM、是做SaaS的创始人、是营销团队的成员，Claude Design简直就是专门为你们打造的。尤其是pitch deck和landing page这类场景，几分钟出稿加上一键导到Canva继续编辑的流程，确实有种生产力突变的感觉。

如果你是做Vibe Coding的开发者（比如我自己），那最有用的还是那个Handoff to Claude Code。只要它能让Claude Code生成的前端UI从"一言难尽"升级到"能看"，就已经值回票价了。

## Claude Design简单实测

我大概初步地用了一下Claude Design，让它根据帮我们公众号设计一套品牌物料。

Claude Design一顿操作之后，设计效果我还是比较满意的：

设计完成后，我直接handleoff给Claude Code：

总结来看，Anthropic这次的打法相当于直接把起点和终点都接管了。起点是对话输入，终点是Handoff给Claude Code实现。中间那些工具串联、格式转换、像素对齐，基本都省掉了。

我不敢说这就是所谓的最终解，毕竟产品刚出来，很多承诺还要看后面怎么兑现。但至少，Vibe Coding里那个"前端UI做不好看"的老大难问题，终于有人拿模型能力正面硬解了。

顺便还有两点感慨，一是现在做垂直赛道，能不能搞到钱，就看谁手速快。头部玩家的一次模型更新或者一次产品发布，随之而来的可能就是很多垂直赛道的终结。很多垂直领域，护城河不是没有，只是保护期会越来越短。

对于个人用户而言，很多新技术和新产品、新工具，你只要学的慢，你就不用学了。

感谢您阅读我的文章。

我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道。
