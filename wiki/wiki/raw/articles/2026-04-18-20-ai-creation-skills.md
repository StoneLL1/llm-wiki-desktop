---
title: "从选题到发布全链路，花叔的20个AI创作Skills全拆解（附教程）"
url: "https://mp.weixin.qq.com/s/PjBn3tUoJxeaYcrQEnSTRQ"
source: "mp.weixin.qq.com"
fetched: 2026-05-27
sha256: 20286605551c0f33
image_count: 8
---

之前推荐过花叔的「女娲」人类蒸馏项目。那套东西的工程质量在整个GitHub的skill生态里都算顶尖——能写出那种项目的开发者，两只手数得过来。

今天我发现花叔还有压箱底干货skill，他悄悄的把20个内容创作Skills全开源了。

huashu-skills，选题、调研、写作、审校、配图、排版、发布——内容创作的每个环节都有对应skill。不是那种README写得很漂亮、装完发现是空壳的demo。是实打实的产品级完成度。

## 这20个Skill到底是什么

花叔把这20个技能按用途分了七类。挑重点说：

**端到端工作流（4个）：**

` huashu-slides` 是整套里分量最重的。给一句话描述，5个阶段自动推——内容结构化→设计风格选择→AI插画生成→幻灯片组装→细节打磨。18种设计风格预设，浮世绘、包豪斯、像素画都有。最终输出标准PPTX，扔给PowerPoint直接改。有个细节：它提供了两条技术路径，一条走可编辑HTML保证可控性，一条走全AI视觉追求效果，你按需选。

`huashu-data-pro` 干数据分析的活。Excel丢进去，出来的是带ECharts图表的交互式HTML报告。5套报告风格库——Financial Times、McKinsey、Economist、Goldman Sachs、Swiss Design。选哪个风格，报告就长什么样。

`huashu-design` 的思路跟别的配图工具不太一样。它不是直接出图，而是先按20种设计哲学、5大流派帮你建立设计语言——每次推荐3个不同流派的方向，并行生成视觉Demo，最后自动启动5维度专家评审打分。适合那种"我知道要好看但不知道要什么好看"的场景。

`huashu-douyin-script` 做短视频脚本。竞品视频下载→AI分析提炼爆款公式→脚本+分镜生成→审校。7维度视频深度分析，钩子、节奏、话术、转化设计全覆盖。

**写作与审校（4个）：**

` huashu-proofreading` 是这套里我个人用得最多的。三遍审校：第一遍事实核查加逻辑链检查，第二遍6大类AI腔识别改写，第三遍节奏打磨。不是简单换几个词——是从内容、风格、细节三个层面重塑。经它审过的文章，AI检测率能压到30%以下。

`huashu-material-search` 解决了一个很实际的问题：AI写出来的东西最大的短板不是文笔，是"没有人味"。这个Skill维护了一个1800+条的个人素材库，写作时直接检索你的真实经历和观点，自动改写成适合长文的叙述逻辑，标注最佳插入位置。前提是你得自己积累素材。

`huashu-topic-gen` 选题用的。给一个方向，出3-4个方案，每个带标题、大纲、优劣分析和工作量评估。还附一个判断：写哪个性价比最高。4种选题类型覆盖——深度评测、实战教程、洞察观点、案例拆解。

`huashu-article-to-x` 把长文浓缩成短内容。3000-5000字的公众号文章压到200-500字，适配X、微博、小红书。3种开头风格——金句型、数据型、价值主张型。不是砍字数，是按平台逻辑重写。

**选题与调研（3个）：**

` huashu-research` 和 `huashu-info-search` 一个做结构化调研，一个做信息搜索。两个都解决同一个痛点：搜了一半会话截断了，之前的成果全丢。这套的方案是搜一轮存一轮，成果实时落盘。`huashu-info-search` 还有个实用功能——自动过滤过时信息，按官方>科技媒体>社区的优先级排序。

**视频创作（3个）：**

` huashu-video-check` 用MrBeast的策略框架帮你检查标题和封面——5种强对比公式，数量/价格/结果/强弱/时间各一套。`huashu-video-outline` 快速出大纲方案，2-3个方案带优劣对比。`huashu-script-polish` 把书面脚本改成能"说"的版本，删书面腔，标停顿和重音。

**配图和文档工具（5个）：**

` huashu-wechat-image` 和 `huashu-xhs-image` 分别给公众号和小红书配图。`huashu-md-to-pdf` 把Markdown转成苹果设计风格的专业PDF，书籍级排版，自动生成封面目录页眉页脚。`huashu-speech-coach` 基于MIT教授Patrick Winston的演讲方法论，帮你准备线下分享。`huashu-prompt-save` 自动分类保存你用过的prompt，5大分类带索引。

## 怎么在WorkBuddy里安装

花叔这套skill原始是给Claude Code写的，但在WorkBuddy这类国产AI客户端里同样能用，只需要换个安装方式。

**第一步，下载源码。**

打开GitHub项目页面，点Code→Download ZIP：
```
    https://github.com/alchaincyf/huashu-skills  
    
```

`
[/code]

解压后你会看到20个文件夹，每个文件夹就是一个独立的skill。

**第二步，手动安装。**

WorkBuddy的skill存放在固定目录下。打开文件资源管理器，导航到：
```
    C:\Users\你的用户名\.workbuddy\skills\  
    
```

`
[/code]

把想要安装的skill文件夹整个复制进去就行。比如要装`huashu-proofreading`，就把解压出来的`huashu-proofreading`整个文件夹拖到`.workbuddy\skills\`目录里。

装完重启客户端，在对话里输入skill名就能触发。

**不需要npm install，不需要pip install，不用配API key。** 纯文档型的skill装完直接能用——这套20个skill里，有15个是纯文档型的，零依赖零配置。

## 有脚本依赖的5个Skill，额外说一句

20个skill里有5个带了Python或JS脚本，安装后需要确保对应环境就绪：

| |   
---|---|---  
`huashu-slides`| `create_slides.py`| `pip install python-pptx`  
`huashu-md-to-pdf`| `convert.py`| `pip install markdown weasyprint`  
`huashu-data-pro`| `read_excel.py`| `pip install openpyxl`  
`huashu-douyin-script`| `download_douyin.py`| `pip install yt-dlp`  
`huashu-image-upload`| |   
  
脚本只是辅助功能。核心的提示词逻辑、工作流设计全在SKILL.md里，哪怕脚本跑不起来，skill依然能帮你完成80%的工作——生成结构、输出内容、提供方案。

## 有一个坑，必须提前说

这20个skill里有3个原始设计依赖Gemini API调用出图：`huashu-wechat-image`、`huashu-xhs-image`、`huashu-douyin-script`。

对于绝大多数用户来说，Gemini的API调用是用不上的——要么没有API key，要么有但不想为生图花钱。

解决办法很简单：装完skill后，在对话里给AI下一条指令——

> 去除Gemini的API调用，也不调用内置生图工具，改为仅输出文生图提示词。

这样这三个skill照样能用，只是从"直接出图"变成了"出描述词，你拿去喂给任何生图工具"。实际上体验差不了太多，而且更灵活——提示词扔给MidJourney、DALL-E、甚至免费的生图网站都能用。

## 如果你想批量适配，有个捷径

如果你打算把20个skill全装上，逐个检查太麻烦。这里有个批量适配的思路：

下载解压后，不要直接复制。先做一次批量修改——在三个文件里各加一行声明：

  1. huashu-wechat-image/SKILL.md 顶部加：【本机定制】已去除Gemini API依赖，AI生成路径改为仅输出文生图描述词。
  2. huashu-xhs-image/SKILL.md 顶部加：同样的声明。
  3. huashu-douyin-script/SKILL.md 顶部加：【本机定制】已去除Gemini API依赖，视频拆解由AI直接完成。

这三行声明是写给AI看的。下次你触发skill时，AI读到这行就会自动跳过Gemini调用，走文生图提示词输出。

修改完再整体复制到`.workbuddy\skills\`目录。一次搞定，不用每个都手动对AI说一遍。

## 说两句真心话

AI客户端的skill生态现在有个明显趋势：从"单个功能工具"向"完整工作流"演进。宝玉的skills偏向微信生态发布，归藏的gstack偏向工程协作，花叔这套则是纯粹的内容创作全链路。

三者不冲突。你做公众号的，可能同时装三家的。

但花叔这套的工程完成度确实让人服气。每个skill的SKILL.md写得像产品文档，错误处理、降级策略、边界case全覆盖。这种东西不是一天写出来的。是长期实战、反复打磨的结果。

20个skill的README总字数加起来超过4万字。光是这份文档本身，就值一个star。

花叔的内容创作Skills合集 - AI审校、选题生成、视频大纲、素材搜索等实用技能

  

https://github.com/alchaincyf/huashu-skills
