---
title: "再也不担心论文了！西湖大学开源：AI论文绘图可以编辑了"
url: "https://mp.weixin.qq.com/s/6CzFVWr9ariTCM_p9pWi_g"
source: "微信公众号"
fetched: 2026-04-18
sha256: ff62f71d34ea98d5
---

# ****

# 
 Datawhale干货 

******作者：西湖大学张岳实验室******

## **那些年我们为一张论文插图付出的代价******

你是否也经历过这样的场景：论文截稿在即，却在一张系统架构图前耗尽心力。AI生图工具虽然颜值在线，但逻辑经常"放飞自我"；而传统的绘图软件又需要专业设计技能，学习曲线陡峭。

更让人头痛的是——好不容易生成一张满意的图片，想要修改一个小图标或者调整几个文字，却发现得到的只是一张无从下手的"死图"。这种"生成不可编辑、编辑要重新生成"的尴尬局面，终于被彻底打破了。

## **从AutoFigure到AutoFigure-Edit：AI论文绘图可以编辑了**

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSibOYen5nJ6khU8twHDrwybazA6QV7UNC2CDXEkib2a0Q4mNDYqghic33HmQ63zpXT28wZPSouSvjNnKCQwOZ6tOniccaf1xlbZ0NU/640?wx_fmt=png&from=appmsg)

论文地址：https://arxiv.org/abs/2603.06674

西湖大学张岳实验室推出的AutoFigure，作为首个能够从长篇科学文本中自动生成出版级学术插图的智能体框架，已经入选ICLR 2026。现在，团队更进一步，发布了AutoFigure-Edit——一个让AI生成图真正"活"起来的全新系统，目前已在github揽获1.6k+ star。

这次升级可不是小打小闹：

- 
从像素到矢量的跨越：生成的不再是静态PNG图片，而是完全可编辑的SVG文件。这意味着你可以在浏览器内直接拖拽、改字、换色，所有元素都按需定制。

- 
参考图引导的风格迁移：上传一张你喜欢的风格图，AI就能自动学习其配色、字体、图标风格，应用到你的科学插图上。再也不用费劲调试Prompt了。

- 
内置交互式编辑器：生成完成后，立即进入可视化编辑画布。调整布局、修改标注、替换图标，所见即所得。

## **AutoFigure-Edit的五阶段流程：从文本到可编辑SVG******

AutoFigure-Edit的核心是一个创新的五阶段流水线，将"文本→可编辑SVG"的复杂任务分解为清晰可控的步骤：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS9JzbiaoRibsyrAtctj3INWF7hicFsXsxZXLtAcWhBAwBK7of7f01icwnpJMfJzUyOW3Wr3q4w5zrs9ePJticGBt2mp8EkN1hhlO8Lc/640?wx_fmt=png&from=appmsg)

AutoFigure-Edit的五阶段流程：风格条件生图 → 分割与结构索引 → 资产提取 → SVG模板生成与精炼 → 资产注入（上图由AutoFigure-Edit生成）

- 
**风格条件生图**：使用文本和参考图生成初始栅格图像

- 
**分割与结构索引**：利用SAM3技术识别视觉组件并构建结构骨架

- 
**资产提取**：提取透明背景的视觉资产

- 
**SVG模板生成与精炼**：生成结构化的SVG布局模板

- 
**资产注入**：将视觉资产注入模板，形成完全可编辑的SVG

## **核心突破：AutoFigure的"推理式渲染"范式******

AutoFigure的成功源于其独特的"推理式渲染"（Reasoned Rendering）范式，将"逻辑布局"和"美学渲染"彻底解耦：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSibtNt7S2ECz0WdY8hhdPIARxc1wn0nQDS1OWEgeC8h6vfq9AE6lMDUAaiaaUSoxQiapxiasib5OYse3hOYvwH3hCqnHVuXYx2nGTuU/640?wx_fmt=png&from=appmsg)

AutoFigure的两阶段解耦设计：Stage I生成结构布局，Stage II进行美学渲染和文字后处理，论文地址：https://arxiv.org/abs/2602.03828v1

### **第一阶段：概念锚定（Conceptual Grounding）******

AI读入你的长篇方法描述（平均超过10,000 tokens），自动提取核心实体和关系，构建初始的逻辑骨架。这一步确保的是"正确性"——把该有的元素都找出来，把该有的关系都理清楚。

### **第二阶段：评审-精炼闭环（Critique-and-Refine）******

模拟人类设计师与甲方的反复拉锯过程。AI Designer负责根据反馈修改布局，AI Critic则专职"挑毛病"——"这里箭头重叠了""布局重心不稳""文字层级不清晰"。经过多轮迭代，直到得到满意的绘图质量。

### **第三阶段：美学渲染与"擦除-修正"******

这是AutoFigure的杀手锏。将优化后的布局渲染为精美图片后，系统会：

- 
用OCR识别图片中的模糊文字

- 
把这些文字"抠掉"

- 
用清晰的矢量文字重新覆盖

彻底解决了AIGC生图中文字变形、模糊的历史难题。

## **AutoFigure-Edit：让矢量编辑成为现实******

在AutoFigure的基础上，AutoFigure-Edit引入了多项技术创新：

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS9sWtJJ6TA44qgWlPSaiayEpXMMCXGvsq8j4yVXcvYGxYUBiarzjoBUzpjaaoy6b2A1iaZrKqn17cGqEh5V5R864djzlzFq4n3B3U/640?wx_fmt=png&from=appmsg)

(1) Raw Generation → (2) SAM3 Segmentation → (3) SVG Layout Template → (4) Final Assembled Vector

### **SAM3驱动的自动分割******

利用Meta最新的SAM3分割技术，系统能够智能识别图中的每个视觉组件（图标、模块、连接线等），并自动生成对应的透明背景资产（RGBA）。

### **SVG模板自动生成与资产注入******

系统会根据分割结果自动生成SVG布局模板，然后将提取的视觉资产一一注入。最终得到的是一个结构清晰、完全可编辑的矢量文件——每一个模块、每一条连线、每一个文字标注都是独立的对象。

### **参考图风格控制******

不再依赖模糊的Prompt描述。上传一张风格参考图，系统会自动学习其视觉特征（配色方案、字体风格、图标类型、间距密度等），并应用到生成的科学插图上。这使得实验室风格的统一、期刊风格的适配变得前所未有的简单。

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSibefLbmXicSAJvhweibqFXm6kRv7s4MyzfPagZPAicQI5fQqgwm0ibTO3tlY42ogicibbsZ3iaGkS6SHYiaD71VwcphmnfUn5UBo9JwMkA/640?wx_fmt=png&from=appmsg)

开源链接：https://github.com/ResearAI/AutoFigure-Edit

## **实验结果：用数据说话******

### **自动化评估：碾压级表现******

团队在FigureBench基准测试上对AutoFigure-Edit进行了全面评估，结果显示其在所有核心维度上都显著领先于现有方法。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSicxp3iasrAYzYkJU2iaHq7ZWEzZicKEUc8sd4q7yHHdHmOVxdIv0ez8FepAvVfeS5qQDEu7b0TA5Z3SukYwA4w1ZfX3OV3aANumwQ/640?wx_fmt=png&from=appmsg)

关键发现：

- 
使用参考图后，**Win-Rate从76.0%提升到83.0%** ，表明参考图引导让生成结果更受用户青睐

- 
内容保真度全面提升：准确性8.83、完整性8.26、适应性8.37，远超其他方法**
**

- 
**无参考图模式下，综合评分达到8.29**，在视觉设计方面表现优异（美学8.32、表达力8.66）

对比基线方法（包括GPT-Image、SVG-Code、Diagram Agent等），AutoFigure-Edit在视觉设计、沟通效果、内容保真度三大维度的平均得分和盲测胜率都展现出压倒性优势。

### **用户研究：217位真实用户的验证******

更具说服力的是基于真实使用场景的用户研究。217位参与者通过在线网站生成了262个插图，并进行了多维度评价：

PNG生成质量：

- 
科学语义正确性：4.04/5.0（48%的用户给满分）

- 
信息完整性：4.11/5.0（51%的用户给满分）

- 
视觉呈现质量：3.95/5.0

- 
风格一致性：4.09/5.0（50%的用户给满分）

**实际可用性：**

- 
**126/262位用户（48%）** 认为生成结果可直接用于论文发表，无需修改

- 
这表明系统已具备**真实科研工作流的可用性**

**SVG转换质量：**

- 
**转换正确性平均得分3.60/5.0**

- 
**36%的用户给满分，说明SVG结构保持了高度准确性**

低评分（1-2分）在语义维度上非常罕见（通常低于12%），证明系统能够可靠地保持科学含义和结构完整性。

## **风格迁移与编辑能力：从生成到创作的完整体验******

AutoFigure-Edit的核心创新在于其强大的风格迁移和编辑能力。系统任意用户自定义风格的参考图引导生成，只需一键上传参考图片，让同一研究内容可以适配不同的视觉风格和出版需求。

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS9GxDajnauERF7q2Qa2V6rO73IbIyll83TSHDsAdS9oZiaS1rEtGOhVNeaICfHjria5kDWTUTg1kQzPH7HzlxmLQ1wGhZbjicMAUk/640?wx_fmt=png&from=appmsg)

上图为参考图，下图为AutoFigure-Edit的生成结果

## **生成案例******

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSiceKpvxG0DzjicXoj4svdCdSoXpxuQn3Yicajdh5ZmhJ7RukZiavrAYT7x9icUcnyjjndZibo8Of8RcW7l3VEBPnwPAaZzfM4Njicawg/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSicZUok1GvRFCwUj30C0LI29j5k2J9LDc8YPdgQ9EjH1lUj5icXYKu7QXof5UAWu4iby7zH1bR2VibmjQUicC5SsGysqs2uibGnz8pPU/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS8ukrJic7uNT6x71wTCyhJgLRjeFJia0vibutM19twyEia48VQ889lbSjjrZj1XOu1JziaUvXicu1qWTL4md7ePN4hhqToriczW8ibViaBg/640?wx_fmt=png&from=appmsg)

CycleResearcher****

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS8RiaOBHf48pG2kgBM2a60AtFTjUt9gibWNicbriaRwrpvqGILbpicxPEuhCmQMxkfb4nVo4pQkiaGEbapDiaV9HOHJ6mQS82cqKN05Gw/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSibEOlBSKN29cgoQGlx6EqS5sISgjxXn209uauAwaLLicnHS6bj4juaAu9AttQon719lB55bIkTicY9Tzhjwrlhj04NOFSsXsUwQc/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS8hnGY4zXccoFWibTlOo2266ktib3hANGjtcrJVqibJL1u15MdH0TQ2YPwMKC1SxfpvIk8564Tjfhpsiaszv3PaMjhLPVS5kkGHtt0/640?wx_fmt=png&from=appmsg)

**DeepReviewer******

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSib3583zu6hhtxs8gicvGf9hWZS2aKtfPfsK8rt20gKnB51TgZ1BbicIrlx8CSEn6DWCdVN5MpcuDwxx6ZFnPHHvicBnEfc6cUP4pQ/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSiczaJUicPDiaYyyOiaKqp8BMiciavermCfICBicHQMmib4Am2d1I2aVLDl4DmfQU53ibfNUdTrp8vPibLliaqpAeYSNxBCcMjFCA5EhEY0EI/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSicSntoSRJJNmVgYpEezVzWah3TAmFIqeO51OkGiaegbEbP9JZNEJVWiaw1YXIuG8TBuiaic2YCTGIBL9tqCib0HYctE0LZ4cxZTbKick/640?wx_fmt=png&from=appmsg)

**DeepScientist**

**图片说明**：以上三组风格迁移案例展示同一论文内容在三种不同参考风格下的生成结果（左侧为PNG图，右侧为SVG图）

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSib0oD3PujIILXQDF40ib8YD3VuV8Z4V8faMrGL4vn8Kb5HiakHga6IpfNxmM52MGDG9FJ4pLDVpZ5hNK6oq6CibXDzjfrO9xZH71E/640?wx_fmt=png&from=appmsg)

AutoFigure产生的方法图

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS8MXd6oKbR7ttzXOLj4JYXDhxYhXVetia6PIFlOluMZ6YwicXa031DWfElicea6Hyz08XiafEiaXWA258QhohHsVpChv3MDibZM2t2T8/640?wx_fmt=png&from=appmsg)

AutoFigure产生的方法图

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSic6AE156PTQf2PhIF7XOtHp28tWRvB2wQMevA8Pne9feZpnvariazN7QRfKpeJES2CEcZkricy5ywgiblYL2ibcNAg19D17ZiarXOUk/640?wx_fmt=png&from=appmsg)

AutoFigure-Edit产生的方法图

## **应用场景：不止是画图******

AutoFigure-Edit的意义远不止于"省时间"：

### **1. 赋能AI科学家******

这是AI实现全流程自主研究的关键一步。从文本理解、实验设计到结果分析，现在有了AutoFigure-Edit，AI也能自主生成可视化结果，真正打通科研的"最后一公里"。

### **2. 降低科研创作门槛******

对于缺乏设计经验的研究者，AutoFigure-Edit让高质量科学插图触手可及。无论你是做算法流程图、系统架构图，还是复杂的教科书示意图，都能一键生成。

### **3. 统一视觉风格******

通过参考图风格控制，整个实验室的论文插图风格可以轻松统一。期刊要求的特定风格（如Nature、ICLR风格）也能快速适配。

## **开源与可用性******

西湖大学张岳实验室始终坚持开源理念：

- 
代码完全开源：GitHub仓库包含完整代码库

- 
数据集公开：FigureBench数据集已在HuggingFace发布

- 
在线网站：提供一键使用的Web界面**
**

- 
**交互式编辑器**：内置可视化编辑画布，支持实时调整

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS8CRH6DMf7WZ2JufeTjEKJILibyXpSh0vCeYhww9NQib9NLuvFpkib1DOuKsewo1SsBK4fe7Lth7BtrN5ibzf4UoiaeaoOiaGXLmxc7w/640?wx_fmt=png&from=appmsg)

开源本地部署画布页面

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSicqXPXPs7omk1iaXbiajzcUgcfekFUKBf30st5wbfVZA3U2rRywUtEr4SZ44Fz8zJiaUbgyGW2nCZyTJpBuRn08clkccYkD8jbJ3o/640?wx_fmt=png&from=appmsg)

网站画布页面

## **如何体验**

AutoFigure-Edit的论文和代码已全部公开：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS97k9eT0mqwJ49ffy8HIH0fzqV8pJ0B6x7pQiaqWibSia6UELqFYphdu309f5fhjT8qiaOl4ykY0gl8pFCwxZtJymjsxRRacu2pON8/640?wx_fmt=png&from=appmsg)

- 
**AutoFigure原始论文**：https://arxiv.org/abs/2602.03828v1

- 
AutoFigure-Edit论文（新）：https://arxiv.org/abs/2603.06674

- 
HuggingFace Daily Paper：https://huggingface.co/papers/2603.06674

- 
AutoFigure **GitHub仓库**：https://github.com/ResearAI/AutoFigure

- 
AutoFigure-Edit **GitHub仓库****（****新****）**：https://github.com/ResearAI/AutoFigure-Edit

- 
在线体验网站：https://deepscientist.cc 

## **团队简介**

本项目由西湖大学张岳实验室全面开源。西湖大学自然语言处理实验室成立于2018年9月，由张岳教授领导。

![image](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSib8OayAhQwmvSg5ZSVibavxzTc2zvM3BrKIQkVfEr89Sqr6O3Nbn6NEg599XL8ibdAVQHTibog2d8jPz00JYejLKDC8N3ibEhlb8SY/640?wx_fmt=png&from=appmsg)

张岳教授毕业于牛津大学，获博士学位，现任西湖大学工程学院副院长，曾担任EMNLP 2022等多个顶级NLP会议的程序委员会主席。欢迎感兴趣的同学加入！有意向申请长期实习、博士生、研究助理者可联系张岳教授邮箱：

zhangyue@westlake.edu.cn

## **写在最后******

学术插图不应是科研路上的拦路虎。AutoFigure和AutoFigure-Edit的出现，正在重新定义科学可视化的边界——让AI不仅"读懂"你的研究，更能"画出"你的洞见。

下次DDL前，不妨试试让AutoFigure-Edit帮你搞定那些繁琐的插图工作。毕竟，你的时间更应该花在思考科学问题上，而不是在PPT里画框对线。

![image](https://mmbiz.qpic.cn/mmbiz_png/vI9nYe94fsGxu3P5YibTO899okS0X9WaLmQCtia4U8Eu1xWCz9t8Qtq9PH6T1bTcxibiaCIkGzAxpeRkRFYqibVmwSw/640?wx_fmt=other&wxfrom=5&wx_lazy=1&wx_co=1&tp=webp#imgIndex=2)
**
**
**一起“**点****赞”****三连**↓**