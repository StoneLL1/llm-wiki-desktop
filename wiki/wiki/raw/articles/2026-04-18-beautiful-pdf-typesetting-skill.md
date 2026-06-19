---
title: "我做了一个精美 PDF 排版 Skill，以生成 100+ 页 Claude Code 系统报告为例"
url: "https://mp.weixin.qq.com/s/7ZhEyJNmwiHaKFrNKZaAHw"
source: "微信公众号"
fetched: 2026-04-18
sha256: d92f83e4b75e3e51
---

**
一天时间，从"源码泄露要做深度研究"到"136页精美PDF自动生成"——一个 Claude Code Skill 的诞生记。

本文目录：

- 一、起因：Claude Code 源码泄露，我想做一份像样的研究报告

- 二、"这能有多难？"——一个下午的崩溃之旅

- 第一站：Pandoc + LaTeX

- 第二站：WeasyPrint

- 第三站：md-to-pdf / Puppeteer 方案

- 第四站：看看 MiniMax 的方案？

- 三、既然没有，那就自己做一个

- 四、解决了哪些"别人没解决"的问题

- 4.1 CJK 双层方案——字符级精确混排

- 4.2 代码块保真——`esc_code()` 黑魔法

- 4.3 全链路文档结构——从封面到封底

- 4.4 智能表格列宽

- 4.5 主题系统——10 种设计风格

- 五、对比：12 个竞品，逐一过招

- 六、为什么不用 MiniMax 的 Skill？

- 七、一个设计哲学的选择

- 八、安装与使用

- 九、后记

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2JxBCGWnj5UJjlzzvl4YVAGl81D9BfBXsWKOp2VZQLSib6EqSwlWzibHvMH91acNcHtAckYhXrSGhwEbJAXIicqhlNDIMibrRGP07w/640?wx_fmt=jpeg&from=appmsg)

目前支持十种设计风格，可自由扩展

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2LqJkAVUXv5Qtume4seNp07xnJcUEdFeiby7j9Y2HoUyuH0CP87LDvotWETz8Z4ZRYQNxWokEibcBliayBdQPmIH7Nmbb7Ojic8A5A/640?wx_fmt=jpeg&from=appmsg)

手动安装：npx skills add lovstudio/md2pdf

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2JzhpQb7uKKFx0mHCsf3fDNg9WRbN8qRWk8yvnDAOmQRtss5KF4OPBE6EfmPSfSzge9DbicuiaScPyfIv4h63YTQgEmLnDtIIX90/640?wx_fmt=jpeg&from=appmsg)

md2pdf skill 的问卷交互

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2JoCpCDj5Uop92iaf0LLABCR4MfQjiaQ0eVCovnQTiahG0LvQ2bY402oIg16Kicm3CtXGHIuMdYcnb8KVxMdbdBgJGzAEfn0ls9IIg/640?wx_fmt=jpeg&from=appmsg)

claude code 深度报告 - 封面

## 一、起因：Claude Code 源码泄露，我想做一份像样的研究报告

3 月 31 日，Claude Code 的完整源码被意外泄露到了 npm。

消息在圈子里炸开。我的第一反应不是围观，而是——**这是一次千载难逢的深度研究机会**。Claude Code 的架构、工具系统、多 Agent 协调、Bridge 远程控制……这些平时只能靠猜的东西，现在全摊开在面前了。

我让 Claude 自己读自己的源码，做了一次彻底的"自我解剖"。最终产出了一份超过 10 万字、覆盖 30+ 章节的深度技术报告。

**然后问题来了：这份报告怎么分发？**

Markdown 文件直接丢群里？没人看。GitHub 上挂 .md？阅读体验差。我需要的是一份**出版级的 PDF**——有封面、有目录、有页眉页脚、有书签导航、代码块排版正确、中英文混排不乱码。

听起来不难对吧？我也是这么以为的。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2LBtiaWoVQHhWO3XNlznaOeqh9Am1PibgALuT4SAN8ibcVZ8rJfic3E4uqXZzpgBoQG8QIlqtRWCeC3l1dy73wWNsEl99huEbPPtFo/640?wx_fmt=jpeg&from=appmsg)

从源码泄露到出版级PDF报告的诞生

## 二、"这能有多难？"——一个下午的崩溃之旅

### 第一站：Pandoc + LaTeX

经典方案，理论上最强大。

现实：装完 TeX Live，4 个 GB 没了。然后开始配 `xeCJK`……中文引号被 LaTeX 劫持了（pandoc#7509[1]），中文加粗语法失效了（pandoc#4887[2]）。光是让一个中英混排的代码块不报错，就折腾了半小时。

**判决：太重了。为了生成一个 PDF，我不想维护一个 LaTeX 发行版。**

### 第二站：WeasyPrint

Python 生态，HTML/CSS 转 PDF，听起来现代优雅。

现实：先编译 cairo 和 pango 的 C 依赖（在 macOS 上又是一番折腾）。生成出来一看——代码块缩进错乱（weasyprint#9292[3]），中文字体一上，渲染速度暴跌（weasyprint#2120[4]）。最要命的是：没有目录页，没有 PDF 书签，没有封面。这些全得自己用 HTML/CSS 从零搭。

**判决：坑太多，CJK 是重灾区。**

### 第三站：md-to-pdf / Puppeteer 方案

Node.js 启一个 headless Chromium，把 Markdown 渲染成网页再"打印"成 PDF。

现实：效果其实还不错——毕竟是完整的浏览器引擎在渲染。但 Chromium 本体 300MB，每次生成要启动一个浏览器进程，内存占用 300MB 起步。而且：**在 Claude Code 的 sandbox 里根本跑不起来。**

我做这个 Skill 的初衷就是让 Claude 自己能用，在对话中直接把 Markdown 变成 PDF。一个需要 Chromium 的方案从根本上就不适合。

**判决：太重，sandbox 不友好。**

### 第四站：看看 MiniMax 的方案？

正好 3 月 25 日 MiniMax 开源了 Office Skills 套件，GitHub 8.5k 星，PDF 模块做得相当专业——15 种封面模板、设计令牌系统、LaTeX 数学公式、自进化质量循环。

但仔细一看：

- **Markdown 只是"二等公民"**。它的核心 CREATE 路线吃的是 JSON content 数组（19 种 block 类型），Markdown 只在 REFORMAT 路线里作为输入——先解析再重排，排版精细度远不如原生 JSON 路线。而我需要的是 Markdown **原生驱动**的全功能排版。

- **没有 CJK 专项支持**。README 原话："CJK support depends on system font availability—not guaranteed across platforms"。中文报告直接用，大概率出□。

- **没有 TOC、没有 PDF 书签、没有水印**。对一份 30 章节的长报告来说，没有目录和侧边栏书签是致命的。

- **封面渲染依赖 Playwright + Chromium**。又是 300MB 的浏览器依赖。

MiniMax 的定位是"通用 PDF 文档生成平台"，做得宽，但在"中文技术报告"这个垂直场景下，它确实不好用。

**判决：好东西，但不是我要的东西。**

![image](https://mmbiz.qpic.cn/mmbiz_jpg/uw5djxcva2IRLeggdzUjwTYibW30QsKpK48ma7oEiaz1TEs6Dxm2cquZgY8ibqDheGyibIaYl8YSmqKAicUBKibg2RAYQDC5rmuAqzO3Wfb3Eyh0I/640?wx_fmt=jpeg&from=appmsg)

四条路全是坑——从Pandoc到MiniMax的踩坑之旅

## 三、既然没有，那就自己做一个

一个下午踩完所有坑之后，需求反而变得极其清晰：
**
我需要一个 **纯 Python、零重依赖、接受标准 Markdown、对中文友好、能在 Claude Code sandbox 里直接跑** 的排版引擎。

技术选型几乎只剩一个：**reportlab**。

`pip install reportlab`，5MB，纯 Python，无 C 编译依赖，无浏览器引擎，无 TeX 发行版。直接构建 PDF 字节流，不经过任何中间格式。

**于是 lovstudio/md2pdf 诞生了。**

整个 Skill 的核心是一个 817 行的 Python 脚本 `md2pdf.py`，从 Markdown 源文件直接生成出版级 PDF。一天之内完成了从零到 136 页实战验证的全过程。

## 四、解决了哪些"别人没解决"的问题

### 4.1 CJK 双层方案——字符级精确混排

这是整个 Skill 最硬核的部分，也是和所有竞品拉开差距最大的地方。

中英文混排看似简单，但在 reportlab 里是个深坑：reportlab 的 `Paragraph` 不支持 fallback 字体——你注册一个 CJK 字体，英文就丑了；注册一个 Latin 字体，中文就□了。

我们的解决方案是 **两层拦截**：

**第一层 `_font_wrap()`**：在 Paragraph 层面逐字符扫描文本，检测每个字符是否落在 CJK Unicode 范围内（U+4E00–U+9FFF、U+3400–U+4DBF 等），然后动态插入 `<font face="CJK">` 标签。一段文字里中文用思源宋体、英文用 Carlito，切换精确到单个字符。

**第二层 `_draw_mixed()`**：在 Canvas 层面（封面标题、页眉页脚等直接画在画布上的文本），实现 CJK/Latin 逐段切换绘制，每段用各自的字体和精确的 x 偏移。

这是**唯一同时解决 Paragraph 和 Canvas 两个渲染层 CJK 问题的方案**。我扫遍了 6 个 Skill 平台、13,000+ 个 skills，没有找到第二家。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/uw5djxcva2IsjibZ3cjCtoggoSgHCBUkZBn6asp8bL5g54ialqHWHU45xxeqJJRia0jQuAa3j6kXFWmk3Jxy3CwONh0s6AeDfGN2bHrlPicc4Jw/640?wx_fmt=jpeg&from=appmsg)

CJK双层架构：Paragraph层与Canvas层的字符级精确混排

### 4.2 代码块保真——`esc_code()` 黑魔法

reportlab 的 Paragraph 有个臭名昭著的行为：**它会吞掉连续空白和换行符**。对正文段落来说无所谓，对代码块来说是灾难——缩进全没了，换行也没了。

`esc_code()` 做了两件事：

- `\n` → `<br/>`（保留每一个换行）

- 行首空格 → `&nbsp;`（保留每一级缩进）

此外，**30 行自动截断**——超过 30 行的代码块自动截断并标注 `... (truncated)`。技术报告里经常有几百行的配置文件或日志输出，不截断会直接撑爆后面好几页。

### 4.3 全链路文档结构——从封面到封底

这是和其他 MD→PDF 工具差距最大的维度。一张表说清楚：

![image](https://mmbiz.qpic.cn/mmbiz_png/uw5djxcva2I5yyC7FW6r09Nv3rSjzUkgxaicKgt40yibICXMVVuJxgXCz1ZNR8YbBXpAkuFxCdjjDByibPD3rSO8cB9bHG48ia86bQMpRsaOVDU/640?wx_fmt=png&from=appmsg)

市面上没有任何一个 MD→PDF 工具同时提供这些。Pandoc 通过 LaTeX 模板可以实现部分功能——但你得会写 LaTeX 模板。我们用 **20+ 个 CLI 参数**把这些全部暴露为零代码配置：

![image](https://mmbiz.qpic.cn/mmbiz_png/uw5djxcva2JhV4GfBThOribIUU5jBOOYH7icwBOcBC6mAAsHhLyD4kkL5JuAOJymQfR9Y6S1jT9eItfq1pkUR3RDH8ichdDZvtBl5qgxicVcUpA/640?wx_fmt=png&from=appmsg)

一行命令，136 页，7.9 MB，出版级 PDF。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2IqaTtEyHQ0XyoPlKyiaUiaHAmBjOYC0icNJibqyGhbQ7dCJSwt8hpRYAhIzFn3KC8hkRG93l7lc5tFUiaiaIYrTR7aPqImTUUdSm1Wg/640?wx_fmt=jpeg&from=appmsg)

全链路文档结构与10种主题风格

### 4.4 智能表格列宽

Markdown 表格到 PDF 的列宽分配是个经典难题。等宽分配浪费空间，浏览器引擎的自动布局在 reportlab 里不存在。

我们的算法：

- 计算每列最长内容的字符数

- 按比例分配可用宽度

- 每列保障 18mm 最低宽度

- 如果总宽度超限，从最宽的列开始扣减

效果：窄列不会被挤成一条线，宽列不会独占整页。

### 4.5 主题系统——10 种设计风格

不是简单换配色。每个主题都有独立的**版式语言**：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/uw5djxcva2IYKXr9oOaUYjxUAjicgibtZw6Iia6LH433HdmXPohfk4JyoQQ3W2LibdMUUicHKelfS5eyyibIkWYibW7LysfN3nYRbRWEExM3ukXgws/640?wx_fmt=png&from=appmsg)

从学术论文到期刊风格，从中式正式到极简水墨，一个 `--theme` 参数切换。还支持 `--theme-file mytheme.json` 完全自定义。

## 五、对比：12 个竞品，逐一过招

我在 Anthropic Skills、SkillsMP.com、LobeHub、OpenClaw（13,729 skills）、Microsoft Skills（132 skills）、FastMCP.me 六大平台做了全面扫描。找到了 12 个 MD→PDF 相关的竞品。

核心结论放一张表：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/uw5djxcva2KaKTj74b5ztGOZFh3x4ba63ASJ8ib3ibz4GbfQia4h3H0wNJmic1Y1lIriaf1ETjXOicGl0bYsPUUXsK72ib2XVcYA4nZOibwuk8qaREw/640?wx_fmt=png&from=appmsg)

**12 个竞品里，没有一个同时具备：Markdown 原生全功能排版 + CJK 专项支持 + 全链路文档结构 + 零重依赖。**

我们是这个交叉点上唯一的方案。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/uw5djxcva2IXY95XFvxdBibTofnfmlqgNsQBtNof7GUG1LqRQ0WiawybTtWqd5dDxYza4uTR0s7DHTLDPdUGqCRlqktkkVC5HWkZKTe4M9EE4/640?wx_fmt=jpeg&from=appmsg)

竞品全景：12个方案中唯一的四维交叉点

## 六、为什么不用 MiniMax 的 Skill？

同期 MiniMax 开源的 Office Skills（8.5k stars）也有 PDF 模块，做得很好。两个项目的设计侧重不同：

MiniMax 侧重**通用文档平台**——HTML/CSS 渲染的封面模板（15+ 种，视觉表现力很强）、LaTeX 数学公式、chart/flowchart 图表内建、表单填充和文档重排三条路线。适合需要丰富视觉效果和多种文档操作的场景。

我们侧重**中文技术报告的全链路排版**——CJK 字符级混排、Markdown 原生驱动、封面→扉页→目录→书签→水印→封底的完整链路、10 种正文版式风格、纯 Python 零重依赖。适合在 Claude Code 对话中一行命令生成长篇中文报告的场景。

此外我们还有一些自己的特色：

- **AI 生图集成**：支持调用 `lovstudio:image-gen`（基于 nano-banana-pro 等模型）根据文档内容自动生成扉页插图，不只是预设模板

- **品牌标识全链路植入**：通过 `--banner`、`--header-title`、`--footer-left` 等参数，把用户自己的品牌元素贯穿封面、页眉、页脚、封底

MiniMax 的 HTML 封面渲染和 LaTeX 数学公式支持是很好的设计，我们后续也会吸收。至于他们提到的"自进化机制"（Execute→Evaluate→Fix 循环），这其实是一种通用的使用范式——在手工川社区我们也非常推荐这种"生成→检查→修正"的迭代工作流，任何 Skill 都可以这样用。

**总之，大家都在探索 AI Agent 生成专业文档的最佳实践，方向不同，互相借鉴。**

## 七、一个设计哲学的选择

一泽（@eze_is）在他的 Web Access Skill 文章里提出了一个公式：
**
**激发模型能力上限的 Skill = Agent 策略哲学 + 最小完备工具集 + 必要的事实说明**

md2pdf 的设计暗合了这个思路：

- **策略哲学**：Markdown 是最自然的中间格式，AI Agent 天然就在输出 Markdown。让 Markdown 成为一等输入——直接 .md → .pdf，全功能，一步到位。

- **最小完备工具集**：一个 Python 脚本，一个 `pip install reportlab`，20+ 个 CLI 参数覆盖所有排版需求。没有多余的依赖，没有多余的配置文件。

- **必要的事实说明**：SKILL.md 里详细记录了每个参数的用途、Hard-Won Lessons（那些踩过的坑）、主题配色参考。Claude 读完就知道怎么用。

**一个好的 Skill 应该让 Agent 用起来像呼吸一样自然。** 不需要装浏览器，不需要学新格式，不需要配字体——告诉它"把这个 .md 转成 PDF"，它就能做到。

## 八、安装与使用

`npx skills add lovstudio/md2pdf
`
```

一行命令安装。装完之后在 Claude Code 里说：
**
"把这个 report.md 转成 PDF，用 Nord 主题，加上水印'内部资料'"

它就会自动调用 md2pdf.py，生成一份带封面、目录、书签、水印、页眉页脚的专业 PDF。

**GitHub**：lovstudio/md2pdf[5]

## 九、后记

这个 Skill 从需求产生到第一个可用版本，只用了一天。但这一天浓缩了：

- 4 种方案的踩坑（Pandoc、WeasyPrint、Puppeteer、MiniMax）

- 6 大平台 13,000+ skills 的竞品调研

- 12 个竞品的逐项对比

- 817 行代码的反复迭代

- 136 页 / 7.9 MB 的实战验证

有人可能会说：为什么不直接用 Pandoc？为什么不等 MiniMax 补上 CJK？

因为**我现在就需要一份漂亮的中文 PDF**，而市面上没有一个方案能在 Claude Code 的 sandbox 里、不装任何额外依赖、一行命令就搞定这件事。

所以我做了一个。

现在你也可以用了。

- *作者：Mark / LovStudio*

- *GitHub：https://github.com/lovstudio/md2pdf*

- *安装：`npx skills add lovstudio/md2pdf`*

- claude code深度报告pdf文件地址（7天有效）：https://lovstudio.ai/paste/f9cniihz

[1] pandoc#7509, https://github.com/jgm/pandoc/issues/7509

[2] pandoc#4887, https://github.com/jgm/pandoc/issues/4887

[3] weasyprint#9292, https://github.com/Kozea/WeasyPrint/issues/9292

[4] weasyprint#2120, https://github.com/Kozea/WeasyPrint/issues/2120

[5] lovstudio/md2pdf, https://github.com/lovstudio/md2pdf