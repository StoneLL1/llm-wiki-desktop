---
title: "让AI帮你生成Logo和图标的Skill"
url: "https://mp.weixin.qq.com/s/ZgyTWjkHPElIIR1Ios5APQ"
source: "微信公众号"
fetched: 2026-04-18
sha256: 874d0f36738de891
---

前几天想给 CodePilot 设计个新 Logo。

就跟 Gemini 聊了聊，让它生成一些 SVG 格式的 Logo。

结果出乎意料——生成的几个变体都很干净、规整，几何感很强。

我把这些 Logo 发到推上，热度还挺高。


![Logo 展示](https://mmbiz.qpic.cn/sz_mmbiz_png/ofWbZTuv4DXeltic4wR1jYK5bXQ5snypE6Z4ERrseJqAuqpFFDX30W8doVunZ1r3LdBIlAm5DU9icbtQSZiaNaznqK2u7CPMzpPDYfrx5VhXDk/640?wx_fmt=png&from=appmsg)
后来我又试着把这些 Logo 做成那种高级的展示图，配上专业的背景，效果很惊艳。


![高级展示图](https://mmbiz.qpic.cn/sz_mmbiz_png/ofWbZTuv4DVia3qgbXMFf4rsXNGkfIkUpdMzMIKjHN1PgqKWAiasMdBwlVHJuljUhBRgQMIZ5lpdWfIJJL3YS3ianxGickTn5787icCcknlgYytE/640?wx_fmt=png&from=appmsg)
发出去以后，发现很多朋友都有类似的需求：

自己做了个小工具或开源项目，需要个图标或 Logo。

但让 AI 画图总是画不好——要么细节不对，要么文字必错，要么就是太随机。

找设计师吧，又觉得"不值得"，毕竟只是个小项目。

最后只能用个丑陋的字母缩写，或者随便找个 icon 凑合。

其实大家的需求很简单：不需要多独特，只要干净、规整、稍微好看点就行。

所以我就想，能不能把这个流程做成一个 Skill，让每个人都能快速生成"够用的好 Logo"？


## Skill 核心能力：三步生成 Logo 和高级展示图

推荐在 Gemini CLI 或者其他用 Gemini 驱动的 Agent 里面用，Gemini 的 SVG 生成能力还是很强的。

当然，你在 Claude Code 里也可以。

这个 Logo Generator Skill 的核心逻辑就三步。


### 第一步：信息收集

Skill 会问你几个简单的问题：

•
产品名称是什么？

•
属于什么行业或类别？（比如 AI、金融科技、设计工具）

•
核心概念是什么？（比如连接、流动、安全、简洁）

•
有什么设计偏好？（比如极简/复杂、冷色/暖色、专业/友好）


![信息收集界面](https://mmbiz.qpic.cn/mmbiz_png/ofWbZTuv4DXKbMjh8faMtLehVcur1XquVAGn6CzfvgAiaxseKaxHaRIA63xsXIc3C0BmKibMjiaK8CZcG7SyjT82uweJ3Hia2IOEEtGAIJQgSVg/640?wx_fmt=png&from=appmsg)
当然你也可以直接把你的项目介绍发给 AI。

好的设计来自理解，而不是随机生成。


### 第二步：生成 6+ 设计变体

基于你提供的信息，Skill 会自动匹配设计模式库，生成至少 6 个不同风格的 SVG Logo。

比如这里我把 Pi 这个开源项目的介绍发给他，他就给了六个选项：

1.
核心 Pi：希腊字母 π 的现代抽象化设计，由三条核心笔画构成。

2.
二进制指令：利用圆角矩形点阵表现扩展系统的模块化特性。

3.
流动智能：粗细不一的平行线代表代码和数据在终端中的持续流动。

4.
系统原点：几何六边形核心代表引擎高效、结构化的基础。

5.
语法壳：使用粗体括号将"支架"表现为一种保护壳。下划线光标代表工具的 CLI 属性。

6.
胶囊支架：对项目中支架概念的进阶诠释。


![六个设计变体](https://mmbiz.qpic.cn/mmbiz_png/ofWbZTuv4DVyBEOyIVwZAE6ykmc0CGGo36waZvia6uDPEibgADjEpuXm3eGSuZ4XnXjCKX2RcaLpPX4rITVMaY8I9g2bkSkZXKMnK7VzBCBec/640?wx_fmt=png&from=appmsg)
每个变体都会生成一个交互式网页，你可以在浏览器里对比查看，选择最喜欢的。

由于 AI 的特性，生成的作品一定会有好的也有差的。

如果你觉得这 6 个里边有哪个不喜欢，或者觉得比较丑，你可以跟它说"换一个"。

它就会找其他可以套用的设计模式帮你替换。

你要是有具体的指导意见也可以提供给它，它也会帮你修改。


### 第三步：高级展示图

选好 Logo 之后，Skill 会帮你生成专业的展示图。这一步提供两种方案：


### 方案 1：Nano Banana 图片生成（12 种专业背景）

用 Nano Banana（Gemini 的图片生成能力）生成高质量的静态展示图，提供 12 种专业背景风格：

暗色系（6 种）：

•
The Void（绝对虚空）：纯黑 + 银色微噪点，硬核科技感

•
Frosted Horizon（磨砂穹顶）：钛灰色 + 有机纹理，高端产品感

•
Fluid Abyss（流体深渊）：深紫/深蓝 + 流体融合，AI 原生感

•
Studio Spotlight（物理影棚）：碳灰色 + 编辑级打光，杂志质感

•
Analog Liquid（物理流体）：纯色底（橙/蓝/绿）+ 金属光泽，创意品牌感

•
LED Matrix（数字硬件）：发光点阵 + 数字复古，赛博朋克感

亮色系（6 种）：

•
Editorial Paper（纸本编辑）：米白色 + 纸张纹理，人文品牌感

•
Iridescent Frost（幻彩透砂）：银灰色 + 全息暗示，科技硬件感

•
Morning Aura（晨雾光域）：暖象牙色 + 柔和色彩，亲和 AI 感

•
Clinical Studio（无菌影棚）：纯白 + 几何阴影，算法驱动感

•
UI Container（容器化界面）：磨砂玻璃容器，SaaS 平台感

•
Swiss Flat（瑞士扁平）：绝对扁平 + 纯色块，永恒权威感


![12种专业背景展示](https://mmbiz.qpic.cn/sz_mmbiz_png/ofWbZTuv4DXoRw9eg4AZGnlmLU22j0vXTdpuTDvTK2aY0FwoCPOg8pU20Yumhwhyiayc4VSN4O5tiayucHtgs8VylgHY9HcrfSW1AicDb2WjvQ/640?wx_fmt=png&from=appmsg)
每种风格都有特定的视觉特征和适用场景。

比如做 AI 产品，可以选 Fluid Abyss 或 Morning Aura；

做硬件产品，可以选 Iridescent Frost 或 LED Matrix。


注意这里需要填写你的 AI Studio 的 API 才行，但是也支持三方的 Nano Banana API。


### 方案 2：WebGL 动态背景（6 种交互式背景）

用 WebGL Shader 生成的动态背景，可以随意缩放、支持鼠标交互，非常适合放在官网首页或产品页：

6 种动态风格：

•
LED Matrix（LED 矩阵）：90×90 高密度 LED 网格 + 流动波浪动画，自适应主题色

•
Fluid Warping（流体扭曲）：域扭曲 + 分形布朗运动（FBM），3 色渐变混合 + 鼠标交互

•
Fabric Wave（织物波浪）：丝绸般起伏 + 交叉波纹，深灰底色 + 微光闪烁

•
Off-Center Ripple（角落涟漪）：双涟漪从对角发散 + 指数衰减，中灰底色

•
Holographic Dispersion（全息色散）：虹彩流体 + RGB 色差，深钛灰底 + 棱镜般色彩分离

•
Spiral Vortex（螺旋漩涡）：旋转螺旋 + 角动量，浅灰底色 + 色带


![WebGL 动态背景](https://mmbiz.qpic.cn/mmbiz_png/ofWbZTuv4DVVTHQRia6DRsiaj53jEMT0s65NU7LRiaFI8H0krQK7fAGky525ic3XFtQW5uP8YXGOcVC4Lt7YVZXicbLKjOsmuHTuO630sIwgwgvk/640?wx_fmt=png&from=appmsg)
WebGL 背景的优势：

•
动态交互：鼠标移动时背景会实时响应，涟漪、扭曲、流动效果

•
无限缩放：基于代码生成，放大缩小都不失真

•
性能优化：60 FPS 流畅运行，自动适配设备像素比

•
直接可用：生成的是 HTML 代码，可以直接嵌入网页

你可以把这些放在官网首页，PPT 或动态背景都可以用。

同一个 Logo，在不同背景下的感觉完全不同。

静态图片适合社交媒体、文档、海报；

动态背景适合网页、演示、交互场景。

最终交付物：完成这三步之后，你会得到一个完整的设计资产包：

•
SVG 文件：可编辑的矢量格式

•
PNG 导出：多种尺寸（1024x1024、2048x2048 等）

•
展示图：4 种专业背景风格

•
交互式网页：可以随时查看和对比所有变体


## 为什么不直接让 AI 画 Logo？

大家看到了我是先让 Gemini 生成 SVG ，再生成展示图"。

而不是直接让 Nano Banana 一步到位生成 Logo 图片。

简单聊一下为什么这么做。

图片模型生成 Logo 的局限性：

1.
控制精度差：你想要一个圆角半径 8px 的圆角矩形？AI 画图很难精准控制这些参数。

2.
无法编辑：生成的是位图，想调整颜色、改个形状、调整间距？只能重新生成，碰运气。

3.
不是矢量：放大就糊，做不了响应式设计，也没法用在不同尺寸的场景。

SVG 有非常多的优势。

SVG 是代码，可以直接复制到 Figma 这些专业设计软件里，进行精细化调整。

可以做成设计体系，可以做动效，可以变成 loading 动画。

可以用在不同场景（网站、App、文档）。

矢量无损，放大缩小都不失真，适配各种分辨率。


![SVG 矢量优势](https://mmbiz.qpic.cn/mmbiz_png/ofWbZTuv4DWSokw83tkibk1PrnX3DvsRI4GytOaTvCRX3c7at2mdM2tp6JkY5dReic480WvDQSnj8DmCwkxFC7iaDT6iczSeBGTZBdJR8JaFAKE/640?wx_fmt=png&from=appmsg)
比如这里，我用 Gemini 生成的 CodePilot Logo SVG，导入 Figma 后：

加了渐变色（从单色变成渐变）、加了内阴影和外发光、调整了点阵数量和大小


![Figma 精修效果](https://mmbiz.qpic.cn/mmbiz_png/ofWbZTuv4DUA6mXuhAqf6ibosSDzmVNmb9Hvd4O7VljG2j4ia28Ic77I64jYE2Jpvcg9C0Gtmtj0gsLUmBcG6lZxia9sj46u75VvfR2s6BYb2Q/640?wx_fmt=png&from=appmsg)
最终的 Logo 比原始 SVG 精致很多，但基础几何结构是 AI 生成的。

这就是"AI 生成基础，人工精修细节"的工作流。

所以这个 Skill 的设计思路是：用 AI 生成可编辑的 SVG 基础，再用 AI 生成高级的展示图。

两步结合，既保证了可控性，又有专业的视觉效果。


## 使用场景拓展：不只是 Logo

这个 Skill 的使用场景其实挺广的：

快速生成 Vibecoding 项目图标，不需要独特性，但要专业、干净。

创业团队早期品牌：预算有限，但需要视觉资产。

可以先用 Skill 生成，后期再找设计师优化。

设计师的辅助工具：

快速生成多个方案给客户选择，或者作为灵感来源。

12 种背景风格不只能用来展示 Logo，还可以：

•
用在网页设计的背景

•
截图做 PPT 背景

•
展示其他产品截图（比如 App 界面、网站首页）


## 开源 + 安装方式

这个 Skill 是完全开源的。

GitHub 地址：https://github.com/op7418/logo-generator-skill

安装方式：

告诉你的 AI 助手： "帮我安装 logo-generator skill，地址是：https://github.com/op7418/logo-generator-skill.git"

## 结尾

这个 Skill 的价值，是降低设计门槛，让每个开发者都能快速获得"够用的好 Logo"。

它不是要替代专业设计师。

设计师做的是"独特性"和"品牌故事"，而 Skill 做的是"快速可用"。

就像 Canva 没有替代设计师，而是让更多人能做出"够用的海报"一样。

工具应该是开放的，让更多人能用上 AI 的设计能力。

欢迎试用、反馈和贡献，觉得有帮助可以帮我点个赞，或者转发给需要的朋友。


✦

