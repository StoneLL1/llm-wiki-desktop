---
title: "Kami：让 AI 生成的文档，终于有了值得一看的排版"
url: "https://mp.weixin.qq.com/s/Rb3r0keQv8q6yFeDYn669Q?scene=1"
source: "微信公众号"
author: "开源小聪明"
account: ""
fetched: 2026-05-27
sha256: 10c01897d1957b46
image_count: 7
---

# Kami：让 AI 生成的文档，终于有了值得一看的排版

> 好内容，值得好纸面。

* * *

## 你有没有遇到过这种情况

让 AI 写一份分析报告，生成出来的东西长这样：默认黑色字体、纯白背景、段落之间毫无节奏感，像一份随手打印出来的 Word 文档。再让它生成一次，风格又完全不一样了。

Kami 的作者 tw93 做美股投资，长期用 AI 生成研究报告，对这个问题深有体会：

> "我受不了丑的文档，尤其是每份报告的风格都不一样。所以我反复调字体、配色、间距，直到做出自己愿意读的东西。"

这就是 **Kami（紙，かみ）** 的起点——日语里"纸"的意思，也是你所有交付物最终落地的那个载体。

* * *

## Kami 是什么

**Kami 是一套给 AI 写的文档设计系统** ，以 Claude Code Skill 的形式分发。安装之后，你只需要用自然语言告诉 Claude 你要什么文档，它会自动套用这套视觉语言生成 PDF，不需要任何斜杠命令，不需要额外提示。

它支持六种文档类型，中英双版本：

* * *

## 先看效果

四份官方示例，直接感受一下：

**Tesla 公司介绍 · One-Pager**

![Tesla 公司介绍一页纸](https://mmbiz.qpic.cn/sz_mmbiz_png/icboQCHicdAMF4wvBTvyzxkM6LSWnGLiaUA5caPzwsZRyQlkJLvk5E7Dq5JKicXfBnrhic5SMibmv9SBZsLevPAMQKnmnHicugf3KzBWWb2aSllibGs/640?wx_fmt=png&from=appmsg) Tesla 公司介绍一页纸

**Agent 演讲 Slides（英文）**

![Agent keynote slides](https://mmbiz.qpic.cn/mmbiz_png/icboQCHicdAMH7GOvq4NzVZ3WaQl9vyj3DSZCMXgBNyJQSdrBfTgDxz81tvxqTFlQxgcjg5aelHS5LSiaKE8l4OTJBTdMPQESYxQHfUAsDbjGQ/640?wx_fmt=png&from=appmsg) Agent keynote slides

**Elon Musk 简历 · Resume**

![Musk resume](https://mmbiz.qpic.cn/mmbiz_png/icboQCHicdAMF5DicJlWHJtIrVqpX61ldFGXM03V1qrzKAzCHILEQGgDw0G6xvBqNRlbCOV4zHnRicA3AG1ruhfJYrdE3tGZUU2WTog2UgJwiaYI/640?wx_fmt=png&from=appmsg) Musk resume

**Kaku 项目作品集 · Portfolio**

![Kaku portfolio](https://mmbiz.qpic.cn/sz_mmbiz_png/icboQCHicdAMGH5PVAmtMbq95tjrwxlp38DtvAXZW70SZr1YpKwHmfnwIo4eTwicXuIEytrwwf3iaINZoZVu05zuNAu4YiasfTZKRdt29ylFF4ib4/640?wx_fmt=png&from=appmsg) Kaku portfolio

你会注意到，所有文档都有一种统一的"质感"：暖米色背景、深蓝色点缀、衬线字体、留白充足。这不是偶然，而是 8 条写死的铁律在背后兜底。

* * *

## 8 条铁律，让每份文档都一个样

Kami 的核心是一套跨文档的约束集合，每一条都有明确理由：

**1\. 页面背景用`#f5f4ed` 暖米色，不用纯白**  
纯白页面在屏幕上太刺眼，打印出来也容易显得廉价。米色是整个设计系统的情感基础。

**2\. 强调色只有油墨蓝`#1B365D` 一种**  
全文档占比不超过 5%。超过就是堆砌，不是克制。

**3\. 所有灰色必须暖调，禁止冷蓝灰**  
冷灰色（比如 `#6b7280`）是 SaaS 产品的典型配色，放进文档里会显得廉价和模板化。

**4\. 英文用 serif 通吃标题和正文；中文 serif 标题，sans 正文**  
字体选型决定了文档的"气质"。serif 承担权威，sans 承担功能。

**5\. Serif 字重固定 500，不用 bold**  
单一字重是这套系统的签名——需要更强存在感，用字号或左侧竖线，不加粗。

**6\. 行距三档，禁止 1.6+**  
紧凑标题 1.1–1.3 / 密排正文 1.4–1.45 / 阅读正文 1.5–1.55。网页习惯用的 1.6 放进印刷品里会显得松散。

**7\. Tag 背景必须实色 hex，禁止` rgba()`**  
这条是个技术细节：WeasyPrint 渲染 PDF 时，`rgba()` 会导致 padding 区和字形区透明度叠加，出现双层矩形 bug。

**8\. 阴影只用 ring shadow 或 whisper shadow**  
` 0 0 0 1pt` 的描边阴影，或 `rgba(0,0,0,0.05)` 的极轻浮起。硬 drop shadow 是 SaaS 界面的习惯，放进文档里很突兀。

* * *

## 配色系统：暖调克制，零冷色

整套色板只有一个彩色——油墨蓝，其余全是暖中性灰。
```
    画布色   #F5F4ED  Parchment  — 整个设计的情感基础  
    卡片色   #FAF9F5  Ivory      — 浮起容器  
    强调色   #1B365D  Ink Blue   — CTA、竖线、数字高亮  
    正文色   #141413  Near Black — 主体文字  
    副文色   #4D4C48  Charcoal   — 次级段落  
    注释色   #87867F  Stone      — 图注、脚注
```

* * *

## 字体系统

**中文** ：仓耳今楷 02（serif，标题）+ 思源黑体（sans，正文）  
**英文** ：Newsreader（serif，标题和正文）+ Inter（sans，UI 元素）  
**代码** ：JetBrains Mono

仓耳今楷是商业字体，个人使用免费，商业用途需在 tsanger.cn 购买授权。Newsreader 和 Inter 均为 OFL 开源字体。

* * *

## 如何安装

一行命令，全局生效：
```
    # Claude Code  
    npx skills add tw93/kami -a claude-code -g -y  
      
    # Codex  
    npx skills add tw93/kami -a codex -g -y
```

Claude Desktop 用户：从 Releases 下载 ZIP，在 Customize → Skills → "+" 中上传即可。

安装后，直接用自然语言描述你要什么：

  *   *   *   * • `make a slide deck for my talk`
  * 

Skill 自动触发，无需任何额外指令。

* * *

## 作者和背景

tw93 是国内知名开源作者，旗下项目包括 Pake（用 Rust 把网页打包成桌面应用，23k+ stars）、MiaoYan（macOS Markdown 编辑器）、Kaku（为 AI 编程打造的终端）。

Kami 是他"工作方法论三部曲"的最后一环：
```
    書く  Kaku  — 你写代码的方式（AI 编程终端）  
    技    Waza  — 你反复操练的习惯（工程实践 Skill 集）  
    紙    Kami  — 你的交付物落地的纸面（文档设计系统）
```

三者共同回答同一个问题：一个认真对待自己输出物的人，工作方式应该是什么样的。

* * *

## 编辑点评

大多数人用 AI 生成文档时，从未想过"视觉质量"这件事——反正够用就行。Kami 的出发点恰好相反：**好内容值得好纸面，并且这个标准可以被自动执行。**

这是一个值得关注的思路：不是让用户去学排版，而是把设计经验编码成 AI 可以遵守的约束。Skill 机制让这套规则变成了可复用的上下文，每次生成都自动套用，风格一致性有了保证。

项目刚刚在 2026 年 4 月 20 日发布 V1.0.0，体量不大，但完成度极高——设计规范、字体选型、反面示例、中英双版本全部到位，开箱即用。

* * *

**项目地址** ：https://github.com/tw93/Kami  
**设计规范官网** ：https://kami.tw93.fun

> 如果你也受不了 AI 生成的丑文档，这个项目值得一试。
