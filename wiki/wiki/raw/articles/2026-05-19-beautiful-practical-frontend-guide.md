---
title: "如何做出美观且实用的前端，速成篇"
url: "https://mp.weixin.qq.com/s/s4GfRWWWXKkOh_VJndv2yw"
source: "微信公众号"
author: "Mav高未央"
account: ""
fetched: 2026-05-19
sha256: f3e999d59468c6a8
---

很多AI博主能做出实用但平庸的前端页面，还有一些AI博主可以做出很炫酷，很有艺术审美的作品，但是这些“作品”往往是无法直接投放到市场的，大概是Gemini自由发挥做出来的“博物馆藏品”。

如何让缺乏美学直觉的人做出优雅同时实用的前端界面，做到根据不同的用途找出最适合的美学风格，遵循最符合用户使用习惯的信息架构，是我这几个月一直在思考的问题。

  

下面我来分享一些这方面的思考，这篇是速成篇，讲可以直接上手使用的我的工作流程。能力培养等长期的事情放在下一篇：

  

## 谁适合阅读

  1. 大概知道前端，skill，claude/kimi/vscode/gemini，html，prd，uiux是什么意思的人
  2. 

三个观念层面的认知矫正：

  1. 一个优秀的设计不只是要美，还要让风格服务于目的（比如复古的风格可能适合音乐分享app，但是不一定适合体育网站）
  2. 对于缺乏美学训练的开发者来说，降低一定的自由度，把美学的的部分决策交给ai是合适的
  3. 一个很美的设计如果不能重复复现而只是只能靠灵光一现，那对于开发者来说就是不实用的

##   

## 工具清单

antigravity(可用其他支持agent/skill的coding agent代替)  
stitch（一个可以直接生成前端设计的网站）  
ia-planner，visual-explorer，ux-designer，这些是我定制的agent（antigravity里面叫workflow，你也可以按照核心的功能让ai自己写一个），具体配置我都放在文档了。  
即梦（可用其他生图模型代替）

下面是我的前端工作流，这个工作流的思想高度体现了上面三个观念，但是具体的实现形式在不断更新，以下是是2026/5/4的版本，里面我会用到几个workflow，如果对antigravity不熟悉，可以用subagent平替：

  

## 完整流程

第一步：生成ia document（信息架构图）  
你需要信息架构图来规定页面布局等等（“用户能看到哪些页面，先点哪里后点哪里”...），这个部分很大程度上决定了网站是否好用。

在生成完prd以后，把prd交付给Antigravity的ia-planner(这是我在antigravity里面定制的一个workflow)，ia-planner的主要功能是通过阅读prd理解项目受众，立意，大致的功能设计，从而针对性的设计网站/app的基础架构，页面规划，信息流。

他也会不断地去问开发者问题，确保需求对齐。末尾会生成线框图让用户对布局有直观的印象（但是线框图在用户确认之后会被删除，因为线框图很容易限制后续设计agent的发挥）

最终会交付一个ia文档（information architecture），里面会有：概述，sitemap，用户路径流向，核心模块清单。

  

  

第二步：生成静态visual schema(视觉方案)  
视觉方案决定了网站的美术风格。把prd交给Antigravity的visual explorer(也是我在antigravity里面定制的一个workflow)，他的主要功能是引导用户从产品需求文档(PRD)和信息架构（IA）向视觉设计方案过渡，确定色彩、动态和整体设计调性，最后的交付物非常类似DESIGN.md

  

第三步：stitch完成设计  
接下来把ia document和visual schema分别依次交给stitch，分别让其根据信息架构和视觉方案完成设计，一般visual schema我会让他尝试多个方案，都试试，最后选一个喜欢的，可以把喜欢的设计选中点export，code to clipboard然后交给你claude/codex/kimi等等去生成

  

  

第四步，完成动态部分设计  
如果走完前三步你觉得页面还不够惊艳，没关系，因为一个美观的前端界面百分之70取决于底图和动效。

调用ux-designer（仍然是我的一个workflow）让他进行创意概念动态表达、设计音效、有冲击力的动画、美术素材生图提示词，+html in canvas（非必要，只适合追求非常高级动态效果的人） 改造建议。

  

html in canvas 的一个示例

  

最后交付完整的一个动态交互方案和一个给用户的生图提示词文档，等待用户拿提示词生成图片，放到对应的assests文件夹。

  

里面每一个workflow（agent）的子功能，比如生成线框图，色彩情绪版等等，都可以单独做一个skill，写入workflow让其调用（skill也不用你自己写，交给ai就好了）