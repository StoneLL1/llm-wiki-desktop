---
title: "五个Skill串起一篇论文"
url: "https://mp.weixin.qq.com/s/qqCmCjSbB0UlK1_pysLT4g"
source: "微信公众号"
fetched: 2026-04-18
sha256: a3cc872d50714be6
---

![](https://mmbiz.qpic.cn/sz_mmbiz_png/VWSTuDDOXGPKVj0so1gR1LedJpOJqickpUs0VCYD1lXtXq8kd7HxX04dCkKcrdUMd98HQOnicsnicpq0sdnWKicMX0ibKC6OprBs78O9wJwbN5ZQ/640?wx_fmt=png&from=appmsg)


真要把论文写作这条线跑顺，其实只要抓好“题目—正文—交付”三个节点。本期挑了 5 个我自己常用、仓库链接已经复查过的 skills，少量搭配就能覆盖大多数投稿场景。


---

### 01｜从题目到答辩的最小组合

环节对应 Skill使用感受立项/开题`luwill/research-skills/research-proposal`Nature Reviews 风格模板，40+ 参考文献占坑，仓库 426★架构规划`lishix520/academic-paper-skills/strategist`拆出 7 个评审节点，提示要补的证据位，当前 172★正文写作`academic-paper-skills/composer` + `K-Dense-AI/claude-scientific-writer`composer 出骨架，scientific-writer 负责润色，K-Dense 主仓库 1.7 万★数据/统计`K-Dense-AI/claude-scientific-skills/statistical-analysis`自带 pandas + statsmodels 工作流，和 scientific-writing 在同一目录排版交付官方 `docx`/`pptx`/`pdf` skills + `ndpvt-web/latex-document`Word、PPTX、PDF 三个方向全部覆盖，LaTeX skill 负责期刊版式
❝
快速验真：`git clone https://github.com/luwill/research-skills.git` 后能看到 `research-proposal/paper-slide-deck/medical-imaging-review` 三个子目录；`lishix520` 仓库内含 strategist/composer；`K-Dense-AI` 仓库位于 `scientific-skills/` 下，statistical-analysis、scientific-writing 等模块一应俱全。

❞

![image.png|500](https://mmbiz.qpic.cn/mmbiz_png/VWSTuDDOXGPicSqpBzM0RD7MPCPliaLd87cSYF7YankWNI2BrDpI4fl53x6mhsgjAO0kv4fFHMYzdR0n40SvEbJSoyCQG17GOLXtxribjVVN6E/640?wx_fmt=png&from=appmsg)
image.png|500
---

### 02｜两个高频 workflow


- **开题 + 文献综述**

 - 在 `research-proposal` 里填题目、语种、字数 → 输出版本 0.9 的开题稿；
 - 接上 `kthorn/research-superpower` 去 PubMed/Semantic Scholar 批量拉文献；
 - 用 strategist 拆章节、标注证据缺口；
 - 把 `SUMMARY.md` 和引文 JSON 扔给 composer，很快能得到一版可读草稿。

- **写作 + 交付**

 - composer 产出 Markdown → scientific-writer 负责润色和术语统一；
 - `docx` skill 导出 Word 终稿，`pdf` skill 做合并/压缩；
 - `luwill/paper-slide-deck` 或 `tfriedel/claude-office-skills` 提取 PPTX，答辩前再用官方 `pptx` skill 精修版式。


---

### 03｜安装与使用注意事项


-
**K-Dense 技能包**：用 `npx skills add https://github.com/K-Dense-AI/claude-scientific-skills --skill scientific-writing` 就能装，statistical-analysis 等子模块同理。如果只想复制文件，记得进入 `scientific-skills` 目录再挑模块。


-
**LaTeX Document Skill**：仓库根目录没有 `skills/` 文件夹，直接保留整个仓库，按 README 运行 `setup.sh`，再执行 `scripts/compile_latex.sh`。


---

### 04｜我自己的搭配顺序


- **选题阶段**：`research-proposal` 起草 → superpower 校验引用 → strategist 调整结构；
- **写作阶段**：composer + scientific-writer 双轨输出正文，statistical-analysis/官方 `xlsx` skill 负责数据与显著性检验；
- **交付阶段**：`docx`、`pdf` skill 生 Word/PDF，`latex-document` 备份 LaTeX 版，`tfriedel/claude-office-skills` 出 PPTX。
这样走下来，基本不用安装超过 6 个 skill。流程清爽，就可以生成一份初稿。


---

### 05｜仓库速查表


- luwill/research-skills： https://github.com/luwill/research-skills
- lishix520/academic-paper-skills： https://github.com/lishix520/academic-paper-skills
- K-Dense-AI/claude-scientific-skills： https://github.com/K-Dense-AI/claude-scientific-skills
- K-Dense-AI/claude-scientific-writer： https://github.com/K-Dense-AI/claude-scientific-writer
- kthorn/research-superpower： https://github.com/kthorn/research-superpower
- ndpvt-web/latex-document-skill： https://github.com/ndpvt-web/latex-document-skill
- tfriedel/claude-office-skills： https://github.com/tfriedel/claude-office-skills
- 官方 docx/pptx/pdf skills： https://github.com/anthropics/skills
这里是学术废物收容所。如果你觉得这篇文章对你有帮助，欢迎点赞分享，科研路上，我们一起进化！


![image.png|500](https://mmbiz.qpic.cn/mmbiz_png/VWSTuDDOXGMGsVSAcTlegVT2NflRH0DNsyWX3vQFPGrL4cibOxtmutgCZU1fRF21csqX4RjicvraAKazyvU7k8Q6sDclnzhibay9GBYuvm8pqE/640?wx_fmt=png&from=appmsg)
image.png|500

