---
title: "Anthropic 开源金融 Skills！华尔街分析师的活装进了插件包"
url: "https://mp.weixin.qq.com/s/v05AAiwJvUA9xtz0BKkCpg"
source: "微信公众号"
author: "字节笔记本"
account: "字节笔记本"
fetched: 2026-05-27
sha256: 55b4e4deee313717
image_count: 5
---

Anthropic开源了金融 Skills。

你可以把这个金融Skills理解成一套面向金融行业的 Claude 插件包。

这套金融Skills把金融分析师、投行顾问、基金运营、财富管理和开户审核这些真实岗位里的重复工作，拆成了一组可以运行的 Agent 工作流。

覆盖的场景很全面。

客户会议准备、行业研究、财报分析、DCF/LBO/三表模型、PE 尽调、LP 报表审计、总账对账、月末结账、KYC 文档审核，不一而足。

整套Skills围绕真实金融工作流拆出来的岗位模块。

比如，客户与咨询有 Pitch Agent 和 Meeting Prep Agent。

前者负责可比公司、先例交易，后者负责在客户会议前整理资料。

Market Researcher 可以围绕一个行业输出市场概览、竞争格局和潜在标的清单。

Earnings Reviewer 用来消化财报电话会和公告，更新模型并起草研报。

![fd7bba76-fa4c-44db-b46d-6341d2850c85.png](https://mmbiz.qpic.cn/sz_mmbiz_png/iacaCWlP1x1wJTPq7qYZcw4lUlH698CtTG5amDUNVhWMEibyb90XzOTdo85qtCwiapokzIwB1PZJJxVzWHaamZptxhJ2S0L7iaSngk3NCW6IAiaM/640?wx_fmt=png)

Model Builder 负责 DCF、LBO 和三表模型，而且是在 Excel 文件里写公式、改单元格。

这套金融 Skills设计真正有意义的地方在于：把金融机构里的具体岗位动作拆成一个个可调用、可审计、可替换的工作单元。

![0c5a0b6a-15a7-4830-93e7-becf3d94a31c.png](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1zTj3QWvlZR7Jf6WAXRAnpcOLIOTVrQ1wUrYIHmevPEclWTp0YSgcASkT1bgshY4mwlwq7dLnTVDT1IJv6IR9M1qCvZLtjuL60/640?wx_fmt=png)

标准化财务数据、基金研究、评级数据、即时新闻、财报电话会转写、一级市场数据、都被放进了 Claude 的工作流里。

当然，这个Skills并不是下载p就可以使用的，里面的金融Skills接口是 Anthropic写的，数据的钱还得你自己掏，而且价格还不菲。

另外它并不适用于国内的金融场景，数据源大部分是海海外的金融数据商，A 股、港股都需要自己来改。

这套对于我们开发者而言，是一个非常值得拆解的企业 Agent 样板。

**因为它展示了一个垂直行业 Agent 应该怎么组织。**

什么时候写 system prompt，什么时候拆 skill，什么时候引入 subagent，什么时候接 MCP 数据源，什么时候把风险边界写死，用什么方式防止模型越权。

从中我们也可以完整窥见Anthropic是怎么把一个金融工作流细分拆成 Agent、Skill、命令和数据连接器。

使用方法如下，Claude Code 里可以直接装：
```
    claude plugin marketplace add anthropics/claude-for-financial-services
    claude plugin install financial-analysis@claude-for-financial-services
    claude plugin install pitch-agent@claude-for-financial-services
    
```

脚本会自动解析文件引用、上传 skill、注册子代理，然后完成部署。

以前大家写 Skills，最多就是做一个知识库，PPT啊，图片生成之类的。

现在，Anthropic 直接拿金融行业开刀，把 Agent、Skill、数据连接器、部署方式、权限边界和人工审批放到同一个仓库里，可以说是给我们完整地打了一个样，完成展示了一个企业级 Agent 应该怎么被组织，如何把一套围绕业务流程拆出来。

这个是金融金融，那么医疗、政务、教育、跨境电商、企业财务和研发管理呢？

是不是也能迁移一下，也可以按照他的思路来重走一遍？

把这些领域按专家流程拆开，把重复动作固化，把外部数据接进来，把边界写清楚。

这或许才是 Claude Skills 真正开始变得有价值的开始。
