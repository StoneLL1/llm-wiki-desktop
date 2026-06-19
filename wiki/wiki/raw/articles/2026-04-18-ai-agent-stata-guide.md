---
title: "让AI agent直接跑你的Stata附傻瓜安装指南"
url: "https://mp.weixin.qq.com/s/qRF6eFgAEItfn2_p_iVuVA"
source: "微信公众号"
fetched: 2026-05-27
sha256: e804d1d2032f2983
image_count: 9
---

凡是搞计量经济的，都关注这个号了  

**邮件：****econometrics666@126.com**

****所有计量经济圈方法论********丛的code程序**********, 宏微观****数据库和各种软**********件都放在社群里.欢迎到计量经济圈社群交流访问********.****

本来都不想去专门写，但还是有不少群友在询问，索性就写一篇简单的介绍性指南。

> 让AI直接跑你的Stata，前件是需要安装个Claude code，但很多群友说装不来，建议通过咱们计量社群那份傻瓜式Claude code安装指南上手，已经被验证过轻易安装成功（视频）。

你有没有这种痛苦？

写完一段回归代码，要在Claude或Codex与Stata之间来回切换。一会儿要把代码粘进去，把报错复制出来，一会儿又要把结果再粘回去。

每次问AI都像在做手工搬运；不过，Stata MCP可以终结这个循环。

MCP是Anthropic推出的开放协议，允许AI模型直接调用外部工具。

Stata MCP基于该协议，把Claude Code、Cursor等AI工具与你本机的Stata打通。

之后，AI写代码，Stata直接跑就行，输出实时的返回，全程不用离开对话界面。

### 目前主流实现（2026年最新）

本文以Claude Code + SepineTam/stata-mcp为主线演示，同时介绍hanlulong版扩展的安装方式。

### 方案一：Claude Code + stata-mcp（推荐计量党）

##### 第一步：安装stata-mcp

先确认本机已有Python（≥3.10）和`uv`。没有`uv`的话，
```
    pip install uv
```

装好后用`uvx`验证一下：
```
    uvx stata-mcp --help
```

也可以直接用pip安装：
```
    pip install stata-mcp
```

##### 第二步：告诉Claude Code在哪里找到Stata

macOS：
```
    claude mcp add stata-mcp -- uvx stata-mcp \  
      --stata-path "/Applications/Stata/StataMP.app/Contents/MacOS/stata-mp"
```

Windows：
```
    claude mcp add stata-mcp -- uvx stata-mcp ^  
      --stata-path "C:\Program Files\Stata18\StataSE-64.exe"
```

> `--stata-path`改成你本机Stata可执行文件的路径。SE、MP、IC版本均支持。

##### 第三步：验证
```
    claude mcp list
```

看到`stata-mcp`出现在列表里，状态为`running`，配置就完成了。
```
    ✓ stata-mcp   uvx stata-mcp --stata-path ...   running
```

### 方案二：VS Code / Cursor扩展（hanlulong版）

主要在VS Code或Cursor里写.do文件的话，这个版本更顺手，直接在编辑器里跑代码。

安装步骤，

1.打开VS Code，进扩展市场，搜索"Stata MCP"

2.安装`hanlulong.stata-mcp`

3.在设置里填入Stata路径（`stata-mcp.stataPath`）

4.在MCP配置文件（`~/.cursor/mcp.json`或`settings.json`）里加入以下内容，
```
    {  
      "mcpServers": {  
        "stata": {  
          "command": "uvx",  
          "args": [  
            "stata-mcp",  
            "--stata-path",  
            "/Applications/Stata/StataMP.app/Contents/MacOS/stata-mp"  
          ]  
        }  
      }  
    }
```

5.重启编辑器，用`Ctrl+Shift+Enter`运行选中代码。

功能亮点是，最多支持100个并行Stata会话，跑多个模型互不干扰；WebView实时图形显示；大数据集`list`输出时可开启精简模式，只返回摘要，省token。

### 实战演示：让Claude Code跑完整DID分析

装好后，直接在Claude Code或Codex对话框里描述需求，

> 我有一份面板数据`firm_panel.dta`，变量包括`year`、`firmid`、`treat`（处理组虚拟变量）、`post`（政策后虚拟变量）、`lnrev`（对数营收）、`size`（规模）。帮我做一个标准的DID估计，控制企业和年份固定效应，结果用`estout`输出。

Claude调用Stata MCP，先查看数据结构，
```
    use "firm_panel.dta", clear  
    describe  
    summarize lnrev treat post size, detail
```

输出实时返回，
```
    Contains data from firm_panel.dta  
     obs:        45,230  
    vars:             8  
    ...  
    Variable |        Obs        Mean    Std. Dev.       Min        Max  
    ---------+------------------------------------------------------------  
       lnrev |     45,230    12.4821     1.8932     6.2140    18.9034  
       treat |     45,230     .3821      .4860          0          1  
        post |     45,230     .5012      .5000          0          1  
        size |     45,230    487.23    1203.41         12     89432
```

紧接着跑DID估计，
```
    ssc install reghdfe, replace  
    ssc install ftools, replace  
      
    reghdfe lnrev c.treat#c.post size, ///  
        absorb(firmid year) ///  
        cluster(firmid)  
      
    estimates store did_main
```

回归结果直接返回，
```
    HDFE Linear regression                            Number of obs   =     45,230  
    Absorbing 2 HDFE groups                          F(  2,   3241)  =     124.83  
                                                      Prob > F        =     0.0000  
                                                      R-squared       =     0.8821  
                                                      Adj R-squared   =     0.8614  
                                                      Root MSE        =     0.6912  
      
                              (Std. err. adjusted for 3,242 clusters in firmid)  
    ------------------------------------------------------------------------------  
                 |               Robust  
           lnrev | Coefficient  std. err.      t    P>|t|     [95% conf. interval]  
    -------------+----------------------------------------------------------------  
     treat#post  |  
            1 1  |   .1823*   .0341      5.34   0.000      .1154       .2492  
            size |   .0003*   .0001      4.12   0.000      .0002       .0005  
           _cons |   10.9214    .2341     46.69   0.000     10.4624     11.3804  
    ------------------------------------------------------------------------------
```

Claude给出解读，

> 处理效应（`treat×post`）系数为0.183，在1%水平显著。政策实施后，处理组企业的对数营收比对照组高出约18.3%。已控制企业和年份双向固定效应，标准误在企业层面聚类。

然后自动做平行趋势检验，
```
    gen rel_year = year - policy_year  
    tab rel_year  
      
    reghdfe lnrev ib(-1).rel_year##treat size, ///  
        absorb(firmid year) cluster(firmid)  
      
    coefplot, ///  
        keep(*.rel_year#1.treat) ///  
        vertical ///  
        yline(0) ///  
        xline(5.5, lpattern(dash)) ///  
        title("Event Study: Parallel Trends Test") ///  
        xtitle("Years Relative to Treatment") ///  
        ytitle("Coefficient (log revenue)")  
    graph export "event_study.png", replace
```

图形生成后自动预览（hanlulong版通过WebView显示，tmonk版直接内嵌在对话里）。

你只说了一句需求，从数据探索、回归估计、结果解读，到平行趋势检验和图形输出，全程没有一次手动切换。

### SepineTam版：安全守卫 + 日志

在服务器或共享环境跑Stata，最怕AI误操作删文件。SepineTam版会自动拦截危险命令，
```
    # 被拦截（AI无法执行）  
    ! rm -rf data/          # shell命令  
    erase important.dta     # 删除文件  
    shell python script.py  # 外部程序  
      
    # 正常执行  
    reg y x1 x2, robust  
    use data.dta, clear  
    xtset firmid year
```

所有执行记录自动写入日志，方便复现和审计。

### tmonk/mcp-stata：最轻量，图形嵌入对话

LSE经济学家开发的版本，核心卖点是图形直接显示在AI对话框里，不用打开任何外部窗口。安装一行搞定：
```
    claude mcp add mcp-stata -- uvx mcp-stata \  
      --stata "/usr/local/bin/stata"
```

说一句"画一张lnrev对year的折线图，按treat分组"，图就出现在对话里，可以直接导出。

### 四个实用技巧

1.写一个`CLAUDE.md`文件

在项目根目录放一个`CLAUDE.md`，写明数据路径、变量说明、分析目标。AI打开就能直接上手，不用每次从头介绍。

2.开启输出精简模式（hanlulong版）

大数据集的`list`输出可能很长，精简模式只返回摘要，能省不少token。

3.让AI做识别诊断

回归跑完后可以直接问："这个模型有什么潜在的识别问题？"AI会结合Stata输出给出具体建议，比自己翻文献快得多。

4.多会话并行（hanlulong版）

同时跑稳健性检验的多个规格，开多个会话互不干扰，最后用`estimates table`汇总。

### 常见问题

Q：Stata需要提前开着吗？

A：SepineTam和tmonk版会在后台自动启动Stata批处理模式，无需手动开。hanlulong版扩展也自动管理会话。

Q：支持哪些版本的Stata？

A：Stata 14及以上均支持，MP、SE、IC版本均可。

Q：数据会不会上传给AI？

A：数据完全在本地运行，发给AI的只有Stata的文字输出（回归结果、统计摘要），原始数据不上传。

Q：Claude Code怎么装？

A：稍微麻烦一点，咱们计量社群有一份傻瓜式上手的Claude code安装指南，已经被验证过轻易上手。

### 怎么选？

|   
---|---  
|   
| SepineTam/stata-mcp + Claude Code  
|   
|   
  
配置半小时，之后每次跑回归少折腾一半。值不值，跑一次就知道。

> *群友可以到计量社群下载和安装stata mcp。

> “1.[天塌了! 不到1小时,斯坦福教授用AI独立,自动完成1篇实证论文, 并且过程和结论都相当精准.](<https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133505&idx=1&sn=2f1263896a16e7adb2854813e4905390&scene=21#wechat_redirect>) 2.[太强悍! 6小时全自动完成一篇QJE级顶尖论文, AI的论文生成速度已碾压人类的验证速度.](<https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133704&idx=1&sn=06c9f26f1ffe101556226b08bc7dfafd&scene=21#wechat_redirect>) 3.[喜欢用DID的, 遇到麻烦了, 一智能体1个月完成了340篇DID论文, 具备经济学顶刊的水准.](<https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133747&idx=1&sn=3829b2f49a1c193115b20362590c44af&scene=21#wechat_redirect>) 4.[DID大牛Sant’Anna发布了一份超强工作流指南: 我的Claude Code配置.](<https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133624&idx=1&sn=f61dbf89dd03cc8970b43c1bb0ea0e48&scene=21#wechat_redirect>) 5.[经济学研究的34个神器! 当AI能自动生成顶刊论文, 经济学者靠什么立足? 该如何不被时代抛下?](<https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133764&idx=1&sn=9f7f00d958d9dd1d71bfd239517fc546&scene=21#wechat_redirect>)

下面这些短链接文章属于合集，可以收藏起来阅读，不然以后都找不到了。  

****8年，计量经济圈近2500篇不重类计量文章，****

**可直接在公众号菜单栏搜索任何计量相关问题****,  
**

**Econometrics Circle**

计量经济圈组织了一个计量社群，有如下特征：热情互助最多、前沿趋势最多、社科资料最多、社科数据最多、科研牛人最多、海外名校最多。因此，建议积极进取和有强烈研习激情的中青年学者到社群交流探讨，始终坚信优秀是通过感染优秀而互相成就彼此的。
