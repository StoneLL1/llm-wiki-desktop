---
title: "AI agents写Stata总翻车? 装上这两个skills，从CSDID到RDD一键生成, 计量效率直接拉满."
url: "https://mp.weixin.qq.com/s/e5lIs4cp2XAcq_RRpvU-xQ"
source: "微信公众号"
fetched: 2026-04-18
sha256: e5680d53168f344f
---

![image](https://mmbiz.qpic.cn/mmbiz_jpg/gPuXCNBzgTdf9lEA4KIdT8AGI4RJM9oiaS1rV4waibBHFAnHYibA1RCAtIf7uicXjhLEHxCyFlzat1Umn7TcBrBHWw/640?wx_fmt=other&watermark=1&wxfrom=5&wx_lazy=1&tp=webp#imgIndex=0)

凡是搞计量经济的，都关注这个号了
**邮箱：****econometrics666@126.com********所有计量经济圈方法论************丛的code程序************, 宏微观****数据库和各种软************件都放在社群里.欢迎到计量经济圈社群交流访问************.******
![image](https://mmbiz.qpic.cn/sz_mmbiz_png/5hq6GFHibJv7BE3C1hmc3BkJYzmGbfVUuWXfFKYkKtauMTURupMoxPs9oLuF8AgfHdZ72HAZ0ytEy1hib4YIrL4AjC8DUia5zsicUTRLNoiaX3rs/640?wx_fmt=png&from=appmsg)

 

你有没有遇到过下面这些情况，
**
1.让AI帮你写CS`DID`的代码，结果给出的选项语法是错的；

2.让它做面板数据，它把`xtset`写成了`tsset`；

3.让它画事件研究图，`event_plot`的参数全靠瞎猜……

这里的问题，就在于AI没有专业的Stata知识库（毕竟是闭源软件）。

今天介绍的两个Claude Code Skills或者Codex Skills就是为了解决上述问题，stata和stata-c-plugins，让AI从略懂Stata直接升级到精通Stata。

### 什么是agent Skill？

Claude Code的Skill机制，类似于给AI装载一本领域专家手册。触发某个Skill后，Claude会自动读取相应的参考文档，用准确的语法、正确的选项、真实的代码模式来回答你。

这两个Skill来自开源项目dylantmoore/stata-skill，安装后立即可用。也可以前往计量社群或AI agent社科研究社群下载压缩文件，进行直接安装。

### Skill 1：stata

##### 它能解决什么问题？

覆盖Stata完整工作流，共38个核心参考文档，再加20个社区包指南，分为以下几大模块，
模块覆盖内容数据操作导入导出、数据清洗、`merge`/`reshape`/`collapse`、字符串与日期处理计量统计线性回归、面板数据、时间序列、受限因变量、MLE、GMM、调查数据、缺失值处理因果推断DID、RD、匹配、处理效应、样本选择高级方法生存分析、SEM/因子分析、非参数方法、空间计量、机器学习（Lasso）图形`twoway`、组合图、出版级图形导出编程宏、循环、`program define`、Mata矩阵编程输出报告`esttab`/`putexcel`/`putdocx`、LaTeX集成
20个社区包涵盖，
**
`reghdfe` ·`estout`·`outreg2`·`csDID`·`DID_multiplegt`·`DID_imputation`·`eventstudyinteract`·`rdrobust`·`psmatch2`·`synth`·`ivreg2`·`xtabond2`·`coefplot`·`binsreg`·`bacondecomp`·`gtools`·`winsor2`·`tabout`·`asdoc`· `graph-schemes`

##### 避免常见的陷阱

Skill内置了13个Stata专属陷阱的防护，这些是新手和老手都会踩的坑。

陷阱1：缺失值排序到正无穷

`* 错误：income有缺失值时，缺失值 > 50000为真，被错误纳入！
gen high_income = (income > 50000)

* 正确
gen high_income = (income > 50000) if !missing(income)`
```

陷阱2：`=`与`==`混用

`* 错误（语法报错）
gen employed = 1 if status = 1

* 正确
gen employed = 1 if status == 1`
```

陷阱3：局部宏引号写错

`local controls "age education income"
regress wage `controls'    // ✓ 正确：反引号 + 单引号
regress wage `controls     // ✗ 缺少右引号
regress wage 'controls'    // ✗ 引号方向错误`
```

陷阱4：`by`不自动排序

`* 错误：数据未排序时报错
by id: gen first = (_n == 1)

* 正确：bysort自动处理排序
bysort id: gen first = (_n == 1)`
```

##### 列举几个真实使用场景

#### 场景1：Stacked DID方法

你只要对Claude code说：“用stacked DID做研究”，然后它就通过stata skill技能得到了下面的内容。

### 为什么用Stacked DID？

标准TWFE在交错处理（staggered adoption）下有两个问题，
**
1.禁止比较，即用已处理单元作为新处理单元的控制组；

2.负权重，即在异质性效应下，TWFE 估计量可能是各组效应的加权平均，部分权重为负。

Stacked DID的解决思路是，为每个处理队列（cohort）单独构建一个干净的2×2 DID数据集，再叠加起来估计。每个子数据集只用从未处理或尚未处理的单元作为对照，彻底避免"禁止比较"问题。

### 完整代码模板

##### 第一步：数据准备

`use "your_data.dta", clear

* 必须变量：
*   unit_id      — 个体ID
*   year         — 年份
*   first_treat  — 首次处理年份（从未处理单元设为 0 或缺失）
*   outcome      — 结果变量

* 确认处理队列
tab first_treat, missing`
```

##### 第二步：构建Stacked数据集

`* ── 参数设置（根据数据调整）──────────────────────────
local window_pre  5    // 处理前保留几期
local window_post 5    // 处理后保留几期

* ── 获取所有处理队列 ──────────────────────────────────
levelsof first_treat if first_treat > 0, local(cohorts)

tempfile stacked
local first = 1

foreach cohort of local cohorts {
    use "your_data.dta", clear

    * 只保留该队列的处理单元 + 干净控制组（从未处理）
    keep if first_treat == `cohort' | first_treat == 0

    * 只保留该队列事件窗口内的时间
    keep if year >= `cohort' - `window_pre' & ///
            year <= `cohort' + `window_post'

    * 生成队列标签和唯一ID（防止不同队列的同一个体混淆）
    gen cohort       = `cohort'
    gen cohort_unit  = unit_id * 10000 + `cohort'

    * 相对时间
    gen rel_time = year - `cohort'

    * 处理变量（该队列单元在处理后为1）
    gen treated_post = (first_treat == `cohort' & year >= `cohort')

    if `first' {
        save `stacked', replace
        local first = 0
    }
    else {
        append using `stacked'
        save `stacked', replace
    }
}

use `stacked', clear`
```

##### 第三步：检查数据结构

`* 确认每个队列的处理和控制单元数量
bysort cohort: tab first_treat

* 确认相对时间分布
tab rel_time cohort, missing

* 检查是否有重复（同一 cohort_unit-year 不应有重复）
duplicates report cohort_unit year`
```

##### 第四步：主回归估计

`* ── 整体 ATT ─────────────────────────────────────────
reghdfe outcome treated_post, ///
    absorb(cohort_unit cohort#year) ///
    vce(cluster unit_id)

* 固定效应结构说明：
*   cohort_unit  — 队列内的个体FE（每个队列的同一个体是独立观测）
*   cohort#year  — 每个队列自己的时间趋势（关键！防止队列间时间趋势混淆）`
```

##### 第五步：事件研究图

`* ── 端点 binning（防止系数过多）────────────────────
gen rel_time_b = rel_time
replace rel_time_b = -`window_pre'  if rel_time < -`window_pre'
replace rel_time_b =  `window_post' if rel_time >  `window_post'

* ── 生成虚拟变量（以 rel_time = -1 为参照期）────────
forvalues k = `window_pre'(-1)2 {
    gen lead`k' = (rel_time_b == -`k') * (first_treat > 0)
}
gen lag0 = (rel_time_b == 0) * (first_treat > 0)
forvalues k = 1/`window_post' {
    gen lag`k' = (rel_time_b == `k') * (first_treat > 0)
}

* ── 事件研究回归 ──────────────────────────────────────
reghdfe outcome lead* lag*, ///
    absorb(cohort_unit cohort#year) ///
    vce(cluster unit_id)

* 平行趋势检验（前期系数联合检验）
testparm lead*

* ── 画图（coefplot）──────────────────────────────────
coefplot, ///
    keep(lead* lag*) ///
    vertical ///
    rename(lead5="-5" lead4="-4" lead3="-3" lead2="-2" ///
           lag0="0" lag1="1" lag2="2" lag3="3" lag4="4" lag5="5") ///
    yline(0, lcolor(gray) lpattern(dash)) ///
    xline(2.5, lcolor(red) lpattern(dash)) ///
    xtitle("处理前后相对时间") ytitle("处理效应估计值") ///
    title("Stacked DID 事件研究") ///
    ciopts(recast(rcap)) scheme(s2color)
graph export "stacked_event_study.pdf", replace`
```

##### 第六步：稳健性检验

`* ── 稳健性1：对比标准 TWFE ──────────────────────────
use "your_data.dta", clear
gen treated_post = (first_treat > 0 & year >= first_treat)
reghdfe outcome treated_post, ///
    absorb(unit_id year) vce(cluster unit_id)
local twfe = _b[treated_post]
display "TWFE 估计: " `twfe'

* ── 稳健性2：Bacon 分解（诊断 TWFE 是否有负权重）────
bacondecomp outcome, ddetail

* ── 稳健性3：对比 Callaway-Sant'Anna ────────────────
use "your_data.dta", clear
csDID outcome, ivar(unit_id) time(year) gvar(first_treat) ///
    method(dripw) notyet
estat simple
estat event, window(-5 5)
csDID_plot

* ── 稳健性4：对比 DID_imputation（BJS 估计量）────────
DID_imputation outcome unit_id year first_treat, ///
    horizons(0/5) pretrend(5) autosample cluster(unit_id)
event_plot, default_look`
```

##### 关键细节对比
细节Stacked DID普通 TWFE控制组每队列独立的干净控制组全样本（含已处理单元）固定效应`cohort×unit` + `cohort×year``unit` + `year`负权重问题不存在可能存在数据量膨胀（同一单元出现多次）原始数据量SE 聚类在原始 `unit_id` 层面聚类同**
Stacked DID数据集中同一个单元会出现多次（每个队列出现一次），因此标准误必须在原始`unit_id`层面聚类，不要在`cohort_unit`层面，否则会低估SE。

##### 还贴心地给出了常见问题

Q：窗口期（window_pre / window_post）怎么选？

根据研究问题和数据覆盖范围决定。处理前至少保留3–5期用于检验平行趋势；处理后根据关心的动态效应长度设定。窗口太长会导致不同队列的窗口重叠，控制组质量下降。

Q：控制变量怎么加？

直接在 `reghdfe` 中加入时变协变量，

`reghdfe outcome treated_post x1 x2 x3, ///
    absorb(cohort_unit cohort#year) ///
    vce(cluster unit_id)`
```

Q：如果没有从未处理单元怎么办？

可以用"尚未处理"单元（not-yet-treated）作控制组。构建数据集时改为，

`* 保留该队列处理单元 + 未来才会处理的单元（在本队列窗口内仍未处理）
keep if first_treat == `cohort' | first_treat > `cohort' + `window_post'`
```

但需注意，尚未处理单元本身可能存在预期效应，需额外检验。

Q：Stacked DID和Callaway-Sant'Anna哪个更好？

两者思路相近，都避免了禁止比较。CS方法有更完善的半参数双稳健估计；Stacked DID更直观透明，容易向读者解释，也更灵活（可以加复杂控制变量、非线性模型）。实践中建议两者都报告，结果一致则互相印证。

##### 场景2：reghdfe还是xtreg，到底用哪个？

有一个企业-年份面板，想控制企业固定效应和年份固定效应，Claude加载reghdfe文档后会这样回答，

`xtreg, fe``reghdfe`多维固定效应只支持一维支持任意多维多向聚类SE不支持支持速度（大样本）慢快5–10倍IV估计不支持支持（`ivreghdfe`）
`* 企业 + 年份双向固定效应，在企业层面聚类SE
reghdfe log_sales log_labor log_capital, ///
    absorb(firm_id year) ///
    vce(cluster firm_id)

* 想保存固定效应估计值？
reghdfe log_sales log_labor, ///
    absorb(fe_firm=firm_id fe_year=year)
* 自动生成fe_firm和fe_year两个新变量

* 超大样本？三维固定效应（AKM劳动力市场匹配）
reghdfe log_wage experience, ///
    absorb(worker_id firm_id year) ///
    vce(cluster worker_id)`
```

###### 场景3：RDD设计全流程，从图到估计到检验

用高考分数线做断点，想做完整的RDD分析，Claude加载rdrobust文档后，输出完整9步工作流，

`* 第1步：构造中心化运行变量（以分数线为零点）
gen score_c = test_score - 600
gen treated = (score_c >= 0)

* 第2步：可视化检验，看断点处有没有跳跃
rdplot outcome score_c, ci(95) shade nbins(20) p(4) ///
    graph_options( ///
        title("RD可视化：分数线两侧结果变量") ///
        xtitle("相对分数线的分差") ytitle("结果变量") ///
        xline(0, lcolor(red) lpattern(dash)) ///
        graphregion(color(white)) legend(off) ///
    )

* 第3步：操纵检验（分数能被操纵吗？）
rddensity score_c, c(0)
* p > 0.05表示密度连续，无操纵证据

* 第4步：协变量平衡检验
foreach var in age female parental_income {
    quietly rdrobust `var' score_c, c(0)
    display "`var'" _col(20) %8.4f e(tau_bc) ///
            _col(35) "p = " %6.4f e(pv_rb)
}
* 各协变量p均应 > 0.05

* 第5步：最优带宽选择
rdbwselect outcome score_c, all

* 第6步：主估计（偏差修正 + 稳健置信区间）
rdrobust outcome score_c, c(0) p(1) bwselect(mserd)

* 第7步：带宽敏感性检验
foreach mult in 0.5 0.75 1 1.25 1.5 {
    quietly rdrobust outcome score_c, h(`= e(h_l) * `mult'')
    display "带宽 × `mult'" _col(20) %8.4f e(tau_bc) ///
            _col(35) "p = " %6.4f e(pv_rb)
}

* 第8步：安慰剂断点检验
foreach c in -20 -10 10 20 {
    quietly rdrobust outcome score_c if score_c < 0, c(`c')
    display "伪断点`c'" _col(20) %8.4f e(tau_bc) ///
            _col(35) "p = " %6.4f e(pv_rb)
}

* 第9步：模糊RD（如果实际入学率 ≠ 1）
rdrobust actual_enrollment score_c               // 第一阶段
rdrobust outcome score_c, fuzzy(actual_enrollment)  // LATE`
```

#### 场景4：一键生成出版级LaTeX表格

想输出一张像AER那样的LaTeX表格，底部显示观测数、Within R²和固定效应情况，

`eststo clear

eststo m1: reghdfe log_wage education, ///
    absorb(year) vce(cluster state_id)

eststo m2: reghdfe log_wage education experience, ///
    absorb(state_id year) vce(cluster state_id)

eststo m3: reghdfe log_wage education experience ///
    i.industry#c.year, ///
    absorb(state_id year) vce(cluster state_id)

esttab m1 m2 m3 using "table1.tex", replace ///
    se star(* 0.10  0.05 * 0.01) ///
    label booktabs compress b(%9.3f) se(%9.3f) ///
    title("工资方程估计结果") ///
    mtitles("(1)" "(2)" "(3)") ///
    keep(education experience) ///
    stats(N r2_within, ///
          labels("观测值" "Within \$R^2\$") ///
          fmt(%9.0fc %9.3f)) ///
    indicate("年份 FE = year" ///
             "省份 FE = state_id" ///
             "行业×趋势 = *industry*")`
```

LaTeX编译后的输出效果，

(1)(2)(3)受教育年限0.082*0.071*0.068*
(0.006)(0.005)(0.005)工作经验
0.031*0.029*观测值45,23145,23145,231Within R²0.1420.1980.221年份FE✓✓✓省份FE
✓✓行业×趋势

✓
###### 场景5：调试报错的do文件

把这段代码粘给Claude，

`forvalues i = 1/5 {
    gen lead`i' = (event_time == -`i')
}
regress y lead* lag*, cluster state`
```

报错信息是`variable state ambiguous`。

Claude的诊断是，这是Stata的变量名缩写匹配到了多个变量（比如`state_id`、`state_name`），`cluster`后面必须写完整变量名。另外，`lead*`通配符按字母顺序展开，会把`lead10`排在`lead2`前面，建议改用reghdfe并显式构造变量列表，

`* 修复（推荐）：用reghdfe吸收固定效应
local leads ""
forvalues i = 5(-1)2 {
    local leads "`leads' lead`i'"
}
local lags ""
forvalues i = 0/5 {
    local lags "`lags' lag`i'"
}
reghdfe y `leads' `lags', ///
    absorb(unit_id year) vce(cluster unit_id)`
```

###### 场景6：数据清洗中的隐藏坑

从Excel导入了收入数据，想做对数变换，结果很多观测值变成了缺失，Claude会带你一步步完整排查，

`* 第1步：检查原始数据
import excel "survey.xlsx", firstrow clear
codebook income    // 查看有无负值、异常值、字符串混入

* 第2步：诊断
count if income <= 0
count if missing(income)

* 第3步：规范处理
* 方案A：排除非正值（注意必须同时排除缺失值！）
gen log_income = log(income) if income > 0 & !missing(income)

* 方案B：Winsorize后取对数（防止极端值影响）
ssc install winsor2
winsor2 income, cuts(1 99) replace
gen log_income = log(income) if income > 0 & !missing(income)

* 方案C：IHS变换（可保留零值）
gen ihs_income = log(income + sqrt(income^2 + 1))

* 第4步：检验合理性
summarize log_income, detail
histogram log_income, normal`
```

###### 场景7：从零搭建规范的项目结构

想要一个符合论文复现标准的master do文件框架，Claude直接给出，

`/*===========================================================
  项目名称：[你的论文题目]
  作者：    [姓名]
  日期：    2026-03-21
  说明：    主文件，按顺序运行所有子do文件
  环境：    Stata 17+，需安装reghdfe ftools estout csDID
===========================================================*/

clear all
set more off
set scheme s2color
version 17           // 确保复现一致性

* 路径设置（只需修改此处）
global root   "D:/projects/myproject"
global data   "$root/data"
global output "$root/output"
global do     "$root/do"

* 自动创建输出目录
capture mkdir "$output/tables"
capture mkdir "$output/figures"
capture mkdir "$output/logs"

* 记录日志
local date = c(current_date)
log using "$output/logs/master_`date'.log", replace text

* 按序执行
do "$do/01_clean.do"          // 数据清洗与变量构造
do "$do/02_baseline.do"       // 基准回归（表2-3）
do "$do/03_event_study.do"    // 事件研究图（图2）
do "$do/04_robustness.do"     // 稳健性检验（表4-6）
do "$do/05_heterogeneity.do"  // 异质性分析（表7）

log close
display "=== 全部完成！输出文件位于：$output ==="`
```

##### 什么时候会自动触发？

凡涉及以下内容，stata Skill自动激活，
**
1.写或调试`.do`文件

2.任何Stata命令的语法咨询

3.数据清洗、回归、图形、输出表格

4.社区包的使用（reghdfe、CSDID、rdrobust等）

不管是第一次接触某个命令、做完整实证流程、调试报错代码、生成论文表格、处理数据陷阱，还是规范项目结构，stata skill做的事情始终是一件，即把Stata专家的知识，随时放在你身边。

### Skill二：stata-c-plugins

###### 它能解决什么问题？

这个Skill针对进阶需求，当Stata速度不够，或者你想把Python/R里的某个统计包移植到Stata里用时，它就派上用场了。

有3个核心使用场景，如下，
**
1.加速Stata命令，用C重写计算密集型操作（比如大规模模拟、自定义距离矩阵）；

2.移植Python/R包到Stata，把scikit-learn的随机森林、R的某个匹配包，做成能在Stata里`net install`安装的正式包；

3.跨平台插件发布，一次开发，支持macOS（ARM64/x86）、Linux、Windows。

阶段内容SDK入门`stplugin.h`数据读写接口、1-indexed索引规则内存安全`malloc`检查、`-fsanitize=address`调试`.ado`封装`preserve/restore`模式、插件加载跨平台路径跨平台编译macOS ARM64、Linux x86_64、Windows x86_64编译命令性能优化pthreads多线程、XorShift RNG、预排序索引、quickselect包装C++库`extern "C"`模式、Eigen矩阵、静态链接测试策略4层测试：复用原包测试集、Stata验证、集成测试、压力测试打包发布`.toc`/`.pkg`/`.sthlp`模板、`net install`分发
##### 典型使用案例

使用案例1：把Python的随机森林移植到Stata

想在Stata里用随机森林，但现有的`rforest`包太慢，Skill会引导Claude这样做，
**
1.检查scikit-learn是否有C后端（有，是`_forest.pyx`加底层C）；

2.优先包装现有C++后端，而非从头实现算法；

3.写`extern "C"`胶水层连接Stata SDK和C++库；

4.生成`.ado`包装器（处理`preserve/restore`、插件路径加载）；

5.跨平台编译并打包为可`net install`的Stata包。

使用案例2：加速大规模的模拟

`STDLL stata_call(int argc, char *argv[]) {
    if (argc < 2) {
        SF_error("需要至少2个参数\n");
        return 1;
    }

    int n = SF_nobs();
    double *data = malloc(n * sizeof(double));
    if (!data) { SF_error("内存分配失败\n"); return 1; }

    // 从Stata读入数据（注意：1-indexed！）
    for (int i = 1; i <= n; i++) {
        SF_vdata(1, i, &data[i-1]);
    }

    // ... 核心计算（C速度）...

    // 写回Stata
    for (int i = 1; i <= n; i++) {
        SF_vstore(2, i, result[i-1]);
    }

    free(data);
    return 0;
}`
```

跨平台编译一览，
目标平台编译命令macOS ARM64`clang -arch arm64 -bundle -o plugin.plugin plugin.c stplugin.c`macOS x86_64`clang -arch x86_64 -bundle -o plugin.plugin plugin.c stplugin.c`Linux x86_64`gcc -shared -fPIC -o plugin.so plugin.c stplugin.c`Windows x86_64`x86_64-w64-mingw32-gcc -shared -o plugin.dll plugin.c stplugin.c`
### 什么时候会自动触发skill？

当你提到以下任一内容，Skill自动激活，
**
1.创建Stata插件/C plugin；

2.加速某个Stata命令；

3.把Python/R包移植到Stata；

4.跨平台编译、`net install`分发。

来一个结尾性的总结，

`stata``stata-c-plugins`定位Stata全栈参考助手C/C++插件开发专家文档规模38个参考 + 20个包指南5个深度参考文档核心价值写对语法、避开陷阱、掌握最新方法突破性能瓶颈、移植他语言包上手门槛低，日常写do文件即可用高，需要C/C++基础典型触发词"帮我写一个csDID事件研究""把这个R包移植到Stata"
上面两个Skills的设计哲学是按需加载，Claude不会一次性读取所有文档，它会根据你的具体问题，精准调取相关的1–3个参考文件。这样既能快速响应，又不会因为知识过载而产生幻觉。

对于每天和Stata打交道的计量人来说，stata Skill几乎是刚需；如果你有开发自定义命令或移植算法的需求，stata-c-plugins则是目前市面上最完整的Claude Code开发指南。

 

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/5hq6GFHibJv76b4ln7CmXRTPsN5upB5QicWPTygTozKgapvmlTnJagBhiatmBicn8G44ibhbUAbiaibKlu0tKhdUzWQFp9bibvbfJp3le9PFqsJIMJ8/640?wx_fmt=png&from=appmsg&watermark=1&wxfrom=5&wx_lazy=1&tp=webp#imgIndex=2)
**
“1.[天塌了! 不到1小时,斯坦福教授用AI独立,自动完成1篇实证论文, 并且过程和结论都相当精准.](https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133505&idx=1&sn=2f1263896a16e7adb2854813e4905390&scene=21#wechat_redirect) 2.[太强悍! 6小时全自动完成一篇QJE级顶尖论文, AI的论文生成速度已碾压人类的验证速度.](https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133704&idx=1&sn=06c9f26f1ffe101556226b08bc7dfafd&scene=21#wechat_redirect) 3.[喜欢用DID的, 遇到麻烦了, 一智能体1个月完成了340篇DID论文, 具备经济学顶刊的水准.](https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133747&idx=1&sn=3829b2f49a1c193115b20362590c44af&scene=21#wechat_redirect) 4.[DID大牛Sant’Anna发布了一份超强工作流指南: 我的Claude Code配置.](https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133624&idx=1&sn=f61dbf89dd03cc8970b43c1bb0ea0e48&scene=21#wechat_redirect) 5.[经济学研究的34个神器! 当AI能自动生成顶刊论文, 经济学者靠什么立足? 该如何不被时代抛下?](https://mp.weixin.qq.com/s?__biz=MjM5OTMwODM1Mw==&mid=2448133764&idx=1&sn=9f7f00d958d9dd1d71bfd239517fc546&scene=21#wechat_redirect) 

 

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/5hq6GFHibJv5EWG46JeKgZ7THxIWxbNSlltzwgSAgZ6z64CSJ1iaoyiaKpRc5XPAOrLQ9vlic1lanMbw5RNOexAJiaVFTQbSvoicSnJAxw1TRVcSw/640?wx_fmt=jpeg&wxfrom=5&wx_lazy=1&wx_co=1&watermark=1&tp=webp#imgIndex=54)