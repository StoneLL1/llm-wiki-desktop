---
title: "啊？我刚开源的 Skills 已经 7K Star 了？！"
url: "https://mp.weixin.qq.com/s/eJEayoZz_0_XX1seSu2txg"
source: "微信公众号"
author: "ConardLi"
account: "code秘密花园"
pub_date: "2026-06-02"
fetched: "2026-06-02"
status: "success"
tool: "wx_extract.py"
image_count: 45
---

# 啊？我刚开源的 Skills 已经 7K Star 了？！

**作者**: ConardLi | **公众号**: code秘密花园

大家好，欢迎来到 **code秘密花园** ，我是花园老师（ConardLi）。

最近，我连续写了几篇关于 AI Agent 的教程。

一篇讲怎么用 Agent 做视频，一篇讲怎么让 Agent 做出更惊艳的网页，还有一篇讲怎么用好 GPT Image2。

每篇文章后面，我都顺手开源了一个 Skill。

本来只是想把自己这段时间沉淀下来的工作流整理出来，方便大家直接拿去用。

结果没想到传播得比我预想中快很多。

后来我把这些 Skill 整理到了一个合集仓库里：

https://github.com/ConardLi/garden-skills

![https://github.com/ConardLi/garden-skills](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLxdGctadfoAHPnGxKI0YicNTpicYyPmX5zQD4CrskXC3iaFEHmGRicTd2hqtFhBpoLkEoUeA7IDwSk76FQ3Xy8stdV5GCAB57iao1Bw/640?wx_fmt=jpeg&from=appmsg)https://github.com/ConardLi/garden-skills

写这篇文章时，GitHub 已经接近 7K Star。

另外也很高兴看到很多同学真正的把我的 Skill 用起来了：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLwBrKAYRybb4ZdMRU6sUfw3XZok8DgibPohXY4koMEic4M3lfKEHbZib2QicRPjDQR6xfCdLeagCcWFSIm1HCCaH26VwMrcibxXAHx0/640?wx_fmt=png&from=appmsg)

也有一些大 V 转发推荐，带来了不少真实反馈。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLyACnGuaYEC2PWGKAEgzGC4QhTZec80nIytPXUAQr9LUHYAw0GH33SggmxD84WgHWG9wu3ZicDs8mHLiaat4jmDW7ia8ibHBHp9Jb8/640?wx_fmt=jpeg&from=appmsg)

这些反馈让我更确定了一件事：

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLzmEuwkkT1WDD4JdWHeOrUXkE9GIHYIIHejB58KicwV9RJlDIrZmMQOlQFuGg33Tl1KVnRBSBHVRJTGvrdRudzkQdKllT2IWZts/640?wx_fmt=jpeg&from=appmsg)

Skill 这东西，真正有价值的地方，不在提示词写得多漂亮。

它的价值在于把一套可重复稳定工作的方法交给 Agent。

## Skill 到底解决了什么问题？

很多人刚开始用 Agent 做复杂任务，都会遇到这样的问题：

有时候效果很惊艳，有时候就开始飘了。

你让它做网页，它可能一会儿像 SaaS 官网，一会儿像课程海报。

你让它做视频，前 30 秒节奏很好，后面突然开始堆字、乱切画面。

你让它生成图片，"赛博朋克"、"高级感" 它都听得懂，可到了真实项目里，出图一张一个样。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLwowuIKHVLia6sHiaNDnz770X4KIBjDOkoIjGE2ArqknyD8urFibstjjia7f7VXjIFH4vj7BmXBE6r7vq46HTBVQzRycCOkN82I3bg/640?wx_fmt=jpeg&from=appmsg)

原因很简单 — Agent 默认接到的是一个"任务"，可复杂产物需要的是一条"生产线"。

一个好 Skill 要提供什么？

明确的工作流程（什么时候该问、什么时候该做、什么时候该停下来让你看）

明确的质量标准（什么算好、什么算 AI 味太重）

明确的迭代接口（不满意时该反馈什么、Agent 知道该改哪一层）。

这几个点组合起来，Skill 才能变成 "一套稳定性的工作系统"。

这也是 garden-skills 想做的事。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLxj6Nz7OvlYB8XzPypobfaUvHRP68GZ8pe1XqywKicsiaX4ps4eDlxq2QkicGhSeuicRy0aicAYlXCYK5jLQPJKdjM9xa060vgoJl5M/640?wx_fmt=jpeg&from=appmsg)

下面，我会正式介绍一下我这几个 Skills，以及最近的一些重点更新。

* * *

## 1\. 视频制作 Skill：把文章做成网页视频

第一个是 `web-video-presentation`。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLzsWHbbjEiafM2uMAibGDNDKDzE9c4JLKWGs2ALlTvnyNnicYATdibQowJMBQMPibiatIFZNibBxmBPpAAsBibIvO9FqBnMaiaabuHw5Tk8/640?wx_fmt=jpeg&from=appmsg)

它可以帮你把文章、脚本、课程、产品 Demo、技术分享等任何文字内容，转化成基于网页制作的演示视频。

注意，这里说的 "视频" 不是直接生成 mp4。

它生成的是一个用 "网页" 模拟的视频效果。

你可以点击或用键盘推进章节，每一步对应一段旁白、一组画面、一段节奏。

录屏时，就像在播放一个精心设计过的视频。

为什么要用网页做视频？

如果你做过 AI 视频，大概率知道长视频最麻烦的地方：随机抽卡、消耗爆炸。

画风一变、节奏一乱，前面铺好的东西全散了。

网页的优势在于，它能把 "视频" 拆成工程。

章节、步骤、旁白、画面、主题、进度控制，全都可以被代码控制。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLwnH6K6GbLiaD9JmzcRvLf1A2R2yWxCHl6BBHCEiaU8wvgiaKCSpHazjh6klFXpibeiaq11Aj4GWchGlVvkOIc7icyx0vm6G7f4dxAwc/640?wx_fmt=jpeg&from=appmsg)

Agent 生成后，你还能继续让它局部修改：

  * "第三章节奏太平了，做得更像发布会 Keynote。"
  *   * 

适合做这些东西：

  *   *   *   *   *   *   * 

### 最近更新 - 主题

新增内置了多套主题模板，覆盖演讲、技术架构、数据报告、科普、终端风、杂志风等不同的方向。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLx8OF4hrFQttBwCoKamUMo028zRINBerAR5Trsy9mu6d0H7tysicw5Uiau848VnCx8Crrg1wTzHxvkEYkLY35RYWCABJJvJdHlb4/640?wx_fmt=png&from=appmsg)

另外，我做了一个在线效果预览网站，帮助大家快速选择适合自己内容的主题：

![https://mmh1.top/#/ai-application/web-video-presentation](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLx4McAPDt4StDV1SFbvWBWdUPHUTaY1pWG0Gia0bHdQzeMjheiaMteNchxEBL3Dic38L2pH8qh4Y4v08tUqMguPR84JQZFydzV9jc/640?wx_fmt=jpeg&from=appmsg)https://mmh1.top/#/ai-application/web-video-presentation

> 网址：https://mmh1.top/#/ai-application/web-video-presentation

下面是部分主题的效果预览（完整的大家可以到网站上自己去看）：

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLzXiaefmXZeCCfeib6QHzjt9nnuqbUV8sY5Rg7k9Z3AKXsVfv8flUML8KG4hUNT0D2YzpnptbakPRfb7LDZ5NBBumzhnINw434As/640?wx_fmt=jpeg&from=appmsg)

`bold-signal` 适合产品发布、营销片头、投资人 pitch 和品牌主张。它的大色块和大标题很适合做强观点开场，第一眼冲击力会更强。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLxUpa7YjWhzCoIun9g0F7wMPoqRwiatmibZO9N36kwVCn0coSKezFZr9SKuPjM7d78wJkVAicRX1LjnhmAIVrKaqMNjviczbdlGgTI/640?wx_fmt=jpeg&from=appmsg)

`terminal-green` 适合 CLI 工具教程、命令行实操、安全话题和黑客风内容。它的终端感很强，适合那种边讲边演示、技术味比较重的视频。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLwWPVqU4GLb9iafVCvCDk5ElQFkFyWzMnLKp0uHfnRkxtzqdqL3y9JweYr8zib1hTgk2fpr6qclxM7KVyIlCViaLs7LCVqvnbczias/640?wx_fmt=jpeg&from=appmsg)

`newsroom` 适合热点解读、时事评论、深度报道和 AI 产品分析。它像一篇报纸特稿，适合有观点、有资料、有叙事的内容。

![](https://mmbiz.qpic.cn/mmbiz_png/xN4PTYkZJLx3VHCsDZj3HhliaMBgKQje3ypADpgjzFq4nGPWKrhgZ7aj7cXm9KRGCmIx9eRgaMkicXVhEYcje7Q0x7kJf0spbjVvO7Dt5Ahpk/640?wx_fmt=png&from=appmsg)

`electric-studio` 适合 B2B 产品演讲、投资人路演、企业财报和行业研究。它是白底电光蓝风格，看起来清晰、商务，但不会太死板。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLzFC6zu282RzzkoRpv1LnLZ5GMBjf8KB9rbhqNNQV1CUHUCAV6SmQVNDblr6orBIP3U35E25j8cqf9lkibDdrbibJ0SelV9omaGQ/640?wx_fmt=png&from=appmsg)

`bauhaus-bold` 适合观点宣言、设计演讲、产品发布和品牌主张。它的包豪斯和布鲁塔利风格很直接，适合那种态度鲜明的视频。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLz1keWZN7jibr1fBq2wuOA97MdEuabkxZ0W0N4z9SzhiaQ1ibmoGia9nsMv4O2hPbWLSoIYibBZsOlviaVvhaQj6kOWDJXvcgymMh0D8/640?wx_fmt=jpeg&from=appmsg)

`creative-voltage` 比较适合创意分享、设计周、工作室作品集和视觉文化类内容。它不是稳重的企业风，而是更像设计工作室、艺术节、创作者发布会。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLx7mxHfuBhZQtiaHuxIKfnj5wIOIUibEgKcE3GvE3ZMtQc8xrBXVfSjuV89oahlcGuGYZ7KU5SJRdgO9qLBkV8nz9IJKJkicSgWuo/640?wx_fmt=jpeg&from=appmsg)

`neon-cyber` 适合 AI、大模型、web3、网络安全和未来科技内容。它的霓虹感比较强，适合节奏快、冲击力强的视频。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLySP7cic5dw7k0KckFeZWelXyPwkib5BneSrF7Dh5801ibmvCxQ11PNCbjSVDvdmViaZUqic9AkERl4nH7wfJFEDORh0icxDIuDRt0OI/640?wx_fmt=png&from=appmsg)

`vintage-editorial` 适合个人观点、文化随笔、美学话题和设计评论。它比较有"专栏作者"的气质，适合有个人表达的视频。

![](https://mmbiz.qpic.cn/mmbiz_png/xN4PTYkZJLze7oV8RPIuDdGJkI0hPDl1IyYufa1BydgPW67oG1hxibAcicXKGfjd8twsoj9t8DEufaTtst5hR9Jhj8rQGvxNicdLkAuz5zI5YA/640?wx_fmt=png&from=appmsg)

`split-canvas` 适合双主题对比、概念对照、辩论和故事讲述。左右双色画布很适合做"过去 vs 现在""A 方案 vs B 方案"这种内容。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLy7VkbIfouaSprMLMUZofqFCTibQwqOkPDG0FqicIF7ibklKmcCMvY0pfEEKuox7aaLKOd8VkQvc3feiaWQ06MqYQbV5S41flk9CEQ/640?wx_fmt=jpeg&from=appmsg)

`dark-botanical` 适合品牌故事、时尚、美妆、旅行、艺术评论和高端产品发布。它有一点时尚杂志和博物馆图录的感觉，更适合偏品牌片的内容。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLzNjicPBFicTwI9ibAyjaGFfjvYbyibcB6XnxksjDSEwUYlHaiaJgK3PSAbep5I6gnH25b7TOrTDw6TKvfEILAiaroq0lsO1QL33ib91M/640?wx_fmt=jpeg&from=appmsg)

`forest-ink` 适合自然、可持续、户外品牌、农业、纪录片和人文观察。它像旧版《国家地理》的气质，沉稳，有文献感。

### 最近更新 - 支持自定义 TTS

第一个版本只支持了 MiniMax CLI 进行音频合成，考虑到大多数人的需求，新版把 TTS 做成了可插拔的方式。

内置 MiniMax 和 OpenAI TTS 示例，也给 ElevenLabs、edge-tts、Azure、Google Cloud 等其他 TTS 留了接入方式。

最简单的，你直接把你的 TTS 接口发给 Agent，它就能自己进行适配了。

### 用好它的几个建议

如果你想出比较稳定的效果，有三件事值得注意。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLxnb8HbQNsyv0muUjibib7l0BByBl7XGsib4e39u001L30BlImN9pykxugcaic0UnEWQ784ib4FHwtxxvm7rAYq1n90iapcgotibPicgNQ/640?wx_fmt=jpeg&from=appmsg)

**模型很关键。**

目前试下来效果最好的是 Opus 4.7。

视频制作 Skill 里有很多审美判断、章节规划、代码实现和返工决策。

模型能力不行，最后的效果可能千差万别。

* * *

**第一轮 Review 一定要认真看。**

很多人一上来就让 Agent 直接做完整视频，跑完发现不满意。

这类长任务最怕前面方向错了，后面做得再精致也没用。

脚本、主题、章节大纲、视觉方向，在前面定得越清楚，后面返工越少。

如果第一轮 outline 不满意，就直接改。

该删章节删章节，该换风格换风格，该调节奏就调节奏。

不要觉得"先让它做完再说"。

* * *

**别期待一次到位，但也别觉得不满意就是失败。**

更好的方式是：先让 Agent 做完整版本，整体跑通后，挑不满意的章节继续调。

"第二章太平"、"第四章信息太密"、"缺少动画效果"，这些都可以单独反馈。

Agent 很擅长这种局部迭代。

你用得越多，越知道自己想要什么，也越容易把它调成适合自己内容风格的版本。

* * *

## 2\. 网页设计 Skill：让 Agent 像专业设计师

第二个是 `web-design-engineer`。

它和视频制作 Skill 有一定交集，但目标不一样。

AI 生成网页最常见的问题，就是一眼就能看出 "这是 AI 做的"。

大渐变、玻璃卡片、发光边框、过度圆角、信息排布松散...

看起来元素不少，实际都是千篇一律的老套路。

`web-design-engineer` 就是为了解决这个问题。

它会把 Agent 从 "套默认审美" 拉回到真正的网页设计流程里：

先判断产品类型和受众，再确定视觉方向、信息层级、排版节奏、组件密度和交互细节。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLyiaDOoONxRO727s92RwLogtD4d8XsPAY09PyeKQBsG9JYv9GbOWLnwyAbpUdqsBOesSuNXvD0AbhSiayhwUuEuDDZRC970U5QG0/640?wx_fmt=jpeg&from=appmsg)

目标是让 AI 做出来的网页更像有经验的设计师和前端一起打磨过，而不是一张常见的 AI 模板图。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLxFnibzDHm6ibb8P2x5XJ5Q6f3xrzF6hdhBicdBnO3h1zLAhKo1v8x7zrmvVL8bzWJJ0gA0KLITsI9O2x8vqrLIYOTUVZEGbA52ibE/640?wx_fmt=jpeg&from=appmsg)

用它来做官网、落地页、Dashboard、活动页、作品集、交互原型，都能明显减少 AI 味，做出更惊艳、更有设计感的网页效果。

### 最近更新 - 新增主题模板

最新版本增加了 25 套不同的设计风格。

每套模板都会包含具体的设计规则：颜色、字体、版式、标志性动作、适合场景、需要避开的套路等等。

很多时候你跟 Agent 说 "高级一点"、"有设计感一点"，它可能并不知道你要什么。

现在，你给它一个大概的方向，它就能自动推断出比较匹配的设计风格。

同样的，我也提供了一个在线预览的网站，部署在 Easy AI 上：

![https://mmh1.top/#/ai-application/web-design-engineer](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLzdFa7LHGzia4SsgJa7NyL11LCzvZ3ibamKNWPJ1f0GCOu7I48mSuPqcFibxQdcPqu7Q6Dh3c84DXtN7kP8yicrqajSDMET8OlkAv0/640?wx_fmt=jpeg&from=appmsg)https://mmh1.top/#/ai-application/web-design-engineer

在线效果预览：

web-design-engineer 在线预览

下面是部分主题的效果预览（完整的大家可以到网站上自己去看）：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLy6aRDLbMqibL54bBSGzfoRCfbiaBydkibdqRwI9VkibluDWKvhapjiaNfPRstepicjobdQVv15D7XiaibNWaqFZ9b9Qm5xHt2Z2swsTrE/640?wx_fmt=png&from=appmsg)

`linear` 适合 B2B SaaS、开发者工具、项目管理工具和 AI 工具官网。它不会把页面做得太花，但会保留足够的产品质感。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLwIEI5Z9gOToBrfgAqyl5ceDPflKlWJk5TfN5SOz4Vg4NMCOn648IJxC0ao17rB3wiaVZK4R6xk6hVhwGcHEGluufy9hITbxAj8/640?wx_fmt=jpeg&from=appmsg)

`raycast` 适合效率工具、命令面板、开发者工作流和工具型产品。它的暗色和快捷感很强，适合偏极客的产品。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLyuZNK7wtOlDj3gNvc9M2Sb4iakbd2ibJkzcUHonbI02cudFeumMGRkXSdwFN8P33w53wRQEfNqCSLTsAEHibDNKbZdgRaZMu7YK4/640?wx_fmt=png&from=appmsg)

`aesop` 适合美妆、护肤、精品零售和生活方式品牌。它的关键不是"高级灰"，而是留白、文字比例、产品图和空间感。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLyr09zavibobxrSOErhygSRvtics75tl2WeIZWGkf4jiad477rIOudpoxFtA3zCGkbrZW1A9VIV3KtOiaIGRogw4SLZYGmv85xomxY/640?wx_fmt=png&from=appmsg)

`tufte-dataink` 适合数据叙事、研究报告、论文图表和信息可视化。它会尽量减少装饰，把注意力放在数据本身。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLzBOm8cTZVlnvVnt6zJPNAcBmiaR5cJ18ibUY3phYBfria4wJp4Kv32xSS8pakxNLp0Pw5B3Noiac7L9QM8GLiaUPv4dSn0vgHOsotk/640?wx_fmt=jpeg&from=appmsg)

`field-io` 适合艺术科技、互动装置、创意工作室和动态视觉官网。它更实验一些，适合做有视觉探索感的页面。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLzemzTZCeOyU4EiaHaRsMH3kL8pwLpjbmpW8lCnTvJV9se4icDgrF3OdvfDWCOP0ZU2jBib3mjVvDiccq92Klic3uAmuOG16nVWhySI/640?wx_fmt=jpeg&from=appmsg)

`active-theory` 适合电影感发布页、品牌 Campaign、游戏 / 娱乐产品和沉浸式首屏。它的冲击力强，适合需要第一屏抓住人的项目。

![](https://mmbiz.qpic.cn/mmbiz_png/xN4PTYkZJLzlclPD6Z5iciapu0WPu03wpm6MR0iag1vqprclWV2ricQM0dgI1pwH3hMSWicg3BTusgX0gibMUV7H2dy2AcfWzjPX7b5BtoJGmYODU/640?wx_fmt=png&from=appmsg)

`bloomberg-businessweek-turley` 适合杂志封面、观点专题和强视觉编辑页。它更大胆、更夸张，适合需要强冲突感的内容。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/xN4PTYkZJLyAn9fQP93DDsFCUg3a7xPzOH09tbSvJ7deVSxuicnIy7pQ7SVQnEZ0phhVGqsicBUS02NW8Poia9H4KGLrgicBlbSkm1jZf0S0nAk/640?wx_fmt=png&from=appmsg)

`balenciaga-post-2017` 适合时装、潮流、反奢侈品和冷感品牌页。它会刻意生硬、压迫、反常规，不适合温和型产品。

![](https://mmbiz.qpic.cn/mmbiz_png/xN4PTYkZJLwvFOU2sThzkF7yibcN5MGd4Rs4fSKQTaQl06kDGJOPOhKkgK6l7BXzYdNQllTMCXHMQfKibGuvnyJCic5pblBbtowxibNtxpQNRIY/640?wx_fmt=png&from=appmsg)

`mailchimp-freddie` 适合社区、创业工具、小团队产品和 B2C SaaS。它更温暖，也更容易拉近和用户的距离。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLzvSftmVBxiay2vHicFKHjiaQNQxSG5icq7MwHP1nibdvL7yYaXBV4sNjSve9TriaUbC8XgbfcLOGHibibZc7cK26wsR3Opr3HUEQ7Q0icQ/640?wx_fmt=jpeg&from=appmsg)

`headspace-meditation` 适合健康、心理、教育和儿童产品。它圆润、轻松，适合低压力的产品体验。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLyygkUaqQwUI7laL0ABtRlY7SV2hM58YsGjoV7CDEz835Zk9K9pY5gatxdWQrm4C3klRbhV6sV1y36ibRIOnRKL0AkzJMENRYJg/640?wx_fmt=jpeg&from=appmsg)

`y2k-retrofuturism` 适合 Y2K 活动页、音乐、潮流、复古科技和年轻化 Campaign。它识别度很高，适合想要明显年代感的页面。

> 完整的主题效果大家可以去这个网站上看：https://mmh1.top/#/ai-application/web-design-engineer

## 3\. 图片生成 Skill：精准复刻各种主流生图玩法

第三个是 `gpt-image-2`。

面向 GPT Image 2 和 OpenAI 兼容图像 API。

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLxhzyA44r7fQicNBlPjTZMpzmNib7xBTQAAuoa68BHbQKvgXJvvIB2pNRyPicAWOR9wTbiagbmibJFE1PXicKkV5815ic7ptPtOTZgq5o/640?wx_fmt=jpeg&from=appmsg)

可以帮你做海报、UI Mockup、产品图、信息图、论文图、技术架构图、漫画、头像、分镜、品牌板，以及图片编辑工作流。

很多人对图片生成的理解还停留在一句 Prompt — "生成一张科技感海报""做一个高级的产品图"。

这种方式能出图，但结果就不太稳定了。话。

真正做项目时，你很快会遇到更多问题：

尺寸是多少？主体放在哪里？文字区域要不要留白？风格参考是什么？要不要分层？能不能安全裁切？要不要适配公众号封面、PPT、官网首屏？

`gpt-image-2` 解决的就是这些问题。

它把图像任务拆成了不同类别，提供结构化模板。

目前包含 18 大类、79 个结构化 Prompt 模板，覆盖生成和编辑两类工作流。

一个好的图片 Prompt，通常需要同时描述：画面目标、主体与关系、构图、材质、光线、字体与文字限制、输出尺寸、后续编辑空间。

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLz4dKPBV90vXBdTbGhGGXHVuKr7S21ooBsIAwhtJyKfb6mD50nGUvnMeRR8eMmrT2iasxZvvhsTsibldf4syzXRTemV6VWib1gbdA/640?wx_fmt=jpeg&from=appmsg)

很多失败图片，不是因为模型听不懂风格词，而是任务本身缺少结构。

比如你要做一张系统架构图，只说 "现代、清晰、科技感" 可能远远不够。

你要说明有哪些模块、模块之间怎么连接、主次层级是什么、哪些文字必须准确、哪些区域需要留白。

Skill 的作用就是尽量少让模型猜。它让 Agent 先把任务拆清楚，再进入生成阶段。

### 三种运行模式

Skill 支持三生图模式：

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLzyRODuu7XwUdqB3ZMJBjIHyweicUDWU7PQMOqUmSuLa4zy2Wiaahs32qZibzk8WpRFEDjAjpxgV50xr3kt5gDWchyico9NVeIickpc/640?wx_fmt=jpeg&from=appmsg)

**本地模式** ，直接调接口出图并落盘（需要你自己提供生图 API Key）。

**宿主工具模式** ，把整理好的 Prompt 交给当前 Agent 自带的图像工具（如在 CodeX 环境中）。

**顾问模式** ，在没有图像工具时，退化成 Prompt 顾问，帮你把 Prompt 写到可执行水平。

这个设计很重要。

因为不同用户的 Agent 环境差异很大 — 有人在本地跑，有人用 Codex，有人用 Claude Code，有人只需要 Prompt。

Skill 先判断环境，再决定怎么工作，能减少很多隐性失败。

### 最近更新 - 在线体验 Image2

生图 Skill 本身最近没啥更新。

但是应很多同学的要求，我给我的 Image2 提示词网站加了个在线体验模块:

![https://gpt-image2.mmh1.top/#/playground](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLyGjnFmHcwSPYia4ibXmPFCld5FGM9ellGLHlCr8DNeuibUatT4ZI0ia0ibqc8RkajIGsY9qhq4ygxcadzpZ4EGZWIHdAqvJfUlJqY8/640?wx_fmt=jpeg&from=appmsg)https://gpt-image2.mmh1.top/#/playground

在提示词详情页，你也可以直接点击一键体验：

![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/xN4PTYkZJLwaoV2PEJImKd1ZxvEa8BdLqxIibr2xCjIQC2UTVCQGNXmnicOZqFibJsibcqjTwjmXPjIR49jBicUsSaycXCdxOPu3uO1KeGib6olAI/640?wx_fmt=jpeg&from=appmsg)

就会自动跳转到这个模块，然后把提示词填充好，你可以自由更改你的提示词：

![](https://mmbiz.qpic.cn/mmbiz_jpg/xN4PTYkZJLzGLmjZfWoSibTMWkBspuwvFBwIVM4UJ9FI98OoDZv2WEw6PickNbGFhghRYgP55WTlXsjvAtdPV74DJ9UQVR3jrMRW3siaPJ4BEs/640?wx_fmt=jpeg&from=appmsg)

## 最后

Skills 开源仓库地址：https://github.com/ConardLi/garden-skills

7K Star 有点超出预期。

但比数字更让我开心的是，很多人真的拿它去做自己的东西了。

如果你最近也在用 Agent 做内容创作、前端页面或者图片生成，可以直接拿去试试。

Skill 最终好不好，不能只看 Demo 漂不漂亮，还要经得起真实任务折腾。

三个在线体验的网页地址：

  * 图片生成：https://gpt-image2.mmh1.top/
  * 网页设计：https://mmh1.top/#/ai-application/web-design-engineer
  * 视频生成：https://mmh1.top/#/ai-application/web-video-presentation

如果这些 Skills 有帮助到你，来个免费的三连吧～
