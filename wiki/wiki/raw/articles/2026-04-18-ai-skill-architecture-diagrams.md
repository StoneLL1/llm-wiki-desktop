---
title: "告别手搓架构图AI Skill 30秒搞定复杂系统图"
url: "https://mp.weixin.qq.com/s/7jXUmaGs6sh68_3Iubgt2w"
source: "微信公众号"
fetched: 2026-04-18
sha256: f3b82f8bd359f474
---

![](https://mmbiz.qpic.cn/mmbiz_jpg/4ONtZwRRHGnaMhr6wwAvEibR0MJlA7yZZdLbaP6DDHkqXF0PC6JcktndXNndf1gicgcl4hicgDO3JL14kxuZ0v9XA/640?wx_fmt=jpeg&wxfrom=5&wx_lazy=1&wx_co=1)
关注 “**AI 工具派”**

探索最新 AI 工具，发现 AI 带来的无限可能性！


Hi，我是Chris，一个专注于探索各类 AI工具的博主，与大家一起发掘 AI 的潜力。我正在开发[WiseMindA](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651429730&idx=1&sn=14456a0880d01521dbbd6f0e3f82c79f&scene=21#wechat_redirect)，期待它能成为提升学习好搭子。


Hi，我是 Chris。

最近看到 X 很多人在分享制作漂亮架构图的 Skill，毕竟写文档、做方案、讲架构的时候，**最头疼的不是内容，而是画图**。
Chris 一直使用 Excalidraw，画图确实很舒服，很自由，但一旦图复杂起来，排版混乱、结构不清晰、反复调整就会很麻烦了。

我最近找到一个很不错的 Skill，叫做 Excalidraw Diagram Skill，可以帮你**一键生成清晰好看的图**。

补充下：


Excalidraw 是一个开源的虚拟手绘风格白板，目前有 121k Star，支持协作和端到端加密，可用于创建手绘风格的图表、线框图等。

官方网址：https://excalidraw.com/


![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjfs645KO3ttzbySo0kebyHJobeeN792dicyiat8mgW9RDnWZt5e5dQfbRPibF4th3BAHlsCZJoTicHqHND705L23NyR4hV5W1Eviafs/640?wx_fmt=png&from=appmsg)

## 一、项目介绍 🌈


🌟 工具名称：Excalidraw Diagram Skill
🔗 工具地址：https://github.com/coleam00/excalidraw-diagram-skill


Excalidraw Diagram Skill 是一款非常不错的 Skill，可以**将自然语言描述转换为美观实用的 Excalidraw 图表**。它专注于**将复杂的概念和流程可视化**，让沟通更清晰、更高效。


![](https://mmbiz.qpic.cn/sz_mmbiz_png/L3QFuGxENjd40FIq4wkt2uhp5NqtTILACq6vt46QZ9hS8gtdbC1lb4BSia2iaYhM8MObuibGicJbDib0R6moFMTXUDeEibT2kXaFNEpn6CSQ0qIjA/640?wx_fmt=png&from=appmsg)
因为本身是个标准的 Skill 项目，所以**可以无缝接入 Claude Code、OpenCode、CodeX 等主流 AI Agent 工具**，甚至 QClaw、WordBuddy 这些工具也支持。


## 二、快速上手 🚀

安装方式跟其他 Skill 一样，最简单的方式就是通过对话方式让 Claude Code 等工具直接安装：


帮我安装 https://github.com/coleam00/excalidraw-diagram-skill 这个 Skill


Chris 本来使用 Claude Code 安装但是一直报错，卡在 `uv run playwright install chromium`一个多小时，于是就换成 QClaw 去安装使用：


![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjcXokJAibDp0HDSfdyXTpbHhDbiboWIobQ4a8K6gK26KIUGEgACibMIiaKibmjOZrpkPK6KXB0Nicwcjf8a00AZ3PGa7Mt6ep6JoOuB8/640?wx_fmt=png&from=appmsg)
安装 Skill 的时候，会自动运行所需命令，安装 Playwright 依赖（用于把 `.excalidraw` 渲染为 PNG）。


由于 Skill 会使用 Playwright 生成图片，而 Chris 发现 QClaw 竟然会自动设置成电脑在使用的 Chrome，这样就不用去安装 Playwright 了，不错。


当然还可以使用脚本进行安装：


```
# 1️⃣ 克隆仓库
git clone https://github.com/coleam00/excalidraw-diagram-skill.git

# 2️⃣ 复制到你的项目技能目录
cp -r excalidraw-diagram-skill .claude/skills/excalidraw-diagram

```

✅ 兼容任何支持 Skills 的编码代理（如 Claude Code、OpenCode），放入 .claude/skills/ 目录即可自动识别。


Excalidraw Diagram Skill 使用起来非常简单：


- **输入描述**：在 AI 代理中输入自然语言描述，例如 "**创建一个流程图，展示 AI 搜索增强（RAG）流程**"。
- **生成图表**：AI 代理会自动分析描述内容，并生成相应的 Excalidraw 图表。
- **查看结果**：查看生成的图表，并根据需要进行调整。

![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjfYUqKBd2ict5Vic6osibmCVgUiaFxY7ChgznyBaE74VlibcqKE3pTnwn26nTmKrlibmDjAcsR7p5ibhAn1hqkL5Weh78cyaqk4J9iboyA/640?wx_fmt=png&from=appmsg)
整个过程非常简单，几分钟就能完成从描述到图表的转换。


![](https://mmbiz.qpic.cn/sz_mmbiz_png/L3QFuGxENjfZ7xC5QnHucCPSbaUQiaz5J925scvRWccRD1XzN9qibHGDf4OwVdfwWPTciblorIgpBfOQ91lU85LRvicNCib61W3n3TKExKPuLIkQ/640?wx_fmt=png&from=appmsg)
当然，你还可以提供更详细的指令，比如：


```
帮我画一个 RAG（检索增强生成）架构图，包含：
- 用户提问
- 向量化处理（Embedding）
- 向量数据库（存储知识）
- 相似度检索
- Prompt 拼接
- 大模型生成回答
- 返回结果

要求：
- 展示完整数据流
- 每个步骤用简单语言说明

```
生成完成后，系统会同时交付 `.png` 和 `.excalidraw` 两个文件：


- `.png`：高清图片，适合直接插入 Markdown 文档、飞书或 PPT 中。
- `.excalidraw`：源文件，支持一键导入 Excalidraw 白板进行二次编辑。毕竟 AI 生成后，偶尔仍有局部不满足需求的地方。此时直接导入源文件手动微调（比如复杂脑图的节点位置），比自己从零拖拽快得多。

![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjdDHuKP1wnrwNNiapXfYzsYrKlc9lyFVEBCwonTIQ1IMFaF5tc2D64dmmiafm9m2292oPlE0PgEDacH8Evyics4VACBsh1KhC24oM/640?wx_fmt=png&from=appmsg)

## 三、核心功能 🔍


### 1. AI 图表生成

Excalidraw Diagram Skill 的核心功能是 AI 图表生成。它能够根据自然语言描述自动生成结构清晰、风格一致的图表，包括：


- 流程图
- 组织结构图
- 技术架构图
- 系统设计图
- 其他复杂图表

### 2. 可视化验证

Excalidraw Diagram Skill 支持可视化验证，能够自动检测并修复布局问题，例如：


- 重叠文本
- 错位箭头
- 不平衡间距

### 3. 可定制品牌风格

Excalidraw Diagram Skill 支持可定制品牌风格，你可以通过修改 `color-palette.md` 文件来调整图表的颜色和样式，使其与你的品牌风格保持一致。


### 4. 兼容性

Excalidraw Diagram Skill 兼容任何支持技能的 AI 代理，例如 Claude Code、OpenCode、CodeX 等。


## 四、效果展示 🍭

最后，Chris 分享几张使用这个 Skill 制作的 WiseMindAI 产品相关的效果图：


![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjeLKlNiaOlsMibVq02icEMSicPVfftWN58cssRPicdDheNPYuw6ho8ibBbndDxs2Pd2eia9j4b19DX4QZKRa2zPKXaMccvXlEwYmPhe48/640?wx_fmt=png&from=appmsg)
上图提示词：


帮我画一个 WiseMindAI 核心功能架构图，包含：


- 用户入口（Web / App）
- 核心功能层：
文件中心、知识库、AI 工作台、笔记中心、AI 搜索


- 学习与输出层：
知识卡片、AI 考试、学习统计、信息图


- 数据与同步层：
浏览器剪藏、数据备份


- AI 能力层：
大模型、向量数据库、多模型调度


- 扩展能力：
Skill、API、外部工具接入

要求：


- 结构从上到下
- 模块分组展示
- 简洁清晰


![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjchOKw6NqpwciaNicEl6HOHI1NOBqvZGjfJmMvkicLBPOAWBT6y5a3J7VbreTdZL3an7T55icrgHaGp6Qm7TTFKocd8QYcffuaYLMw/640?wx_fmt=png&from=appmsg)
上图提示词：


帮我生成一个 WiseMindAI 用户使用场景流程图，用“一个人学习新知识”为主线，展示完整过程：


- 用户遇到问题或学习需求
- 收集资料（上传文件 / 收藏网页）
- AI 自动解析内容
- 构建个人知识库
- 通过 AI 对话理解内容
- 自动生成笔记
- 转换为知识卡片进行记忆
- 通过 AI 考试检测掌握情况
- 最终生成总结或信息图进行输出


![](https://mmbiz.qpic.cn/sz_mmbiz_png/L3QFuGxENjeZiawVWhsGYKtYC0FicYRO8juTU2omx9a8sXJGMXhNQPWhWtC6SehJlxicxOnwr564Bxh5BwA5BCxhUjlAfGG1BdIzmeboqQ35J0/640?wx_fmt=png&from=appmsg)
上图提示词：


帮我画一个知识库使用流程图，包含以下步骤：


- 创建知识库
- 添加资料（文件 / 网页）
- 系统自动整理内容
- AI 提问与总结
- 输出结果（笔记 / 信息图）
要求：


- 使用简单流程图
- 每一步用一句话说明
- 风格简洁清晰


## 五、总结 📝

就像开头提到的，Excalidraw 手绘风的自由感很棒，但复杂图表的排版确实耗人。Excalidraw Diagram Skill 的出现，刚好补齐了这块短板。**它把“构思图表”和“落地排版”彻底解耦**，配合内置的视觉校验，交付效果好。

如果你经常要写技术文档、做项目演示或梳理复杂系统，Excalidraw Diagram Skill 绝对值得尝试。

欢迎评论区交流～


****

**近期推荐**


-
[WiseMindAI：本地优先的 AI 学习工作台](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651429730&idx=1&sn=14456a0880d01521dbbd6f0e3f82c79f&scene=21#wechat_redirect)


-
[ChatTOC：一键管理你的 AI 聊天记录，支持 20+ AI 平台](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651429710&idx=1&sn=4e11c5b5975c6e98b5abecfc22c255f9&scene=21#wechat_redirect)


-
[用 AI 把学习效率拉满：通过“学 → 考 → 评” 提升你的学习效率](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651428705&idx=1&sn=4989c6da39edf7d829d8a4ff9a00fd77&scene=21#wechat_redirect)


-
[我是怎么用 AI，把一个人干成一个团队的？分享我的 AI 员工们](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651428628&idx=1&sn=282238de0808e29111df695935803438&scene=21#wechat_redirect)


-
[为什么我放弃稳定工作，专心做一款真能提升学习效率的 AI 工具](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651428585&idx=1&sn=af05a85a56aef11aead26c6bf3637b2e&scene=21#wechat_redirect)


-
[如何把一个顶级博主的全部内容，变成你自己的 AI 知识库](https://mp.weixin.qq.com/s?__biz=MzA5MjU0NzQ3Ng==&mid=2651429340&idx=1&sn=1fa1b3e774d9273df4fc4e01debb5e1e&scene=21#wechat_redirect)


![](https://mmbiz.qpic.cn/mmbiz_png/L3QFuGxENjet0oHLEq2He8bm6yqag9yprs72sD6hibJHfpXqaCSfup7vsUc750xqD4cBQfJlRofibjEEUGmerVbmXV1icVia5IBW1sJibTK30IyY/640?wx_fmt=png&from=appmsg)

