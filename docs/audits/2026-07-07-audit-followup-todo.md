# 审计修复跟踪待办

日期：2026-07-07

这份文档是对 `docs/audits/` 里三份审计报告的当前状态复盘。目标不是重新写一份大审计，而是把已经修掉的、还没完全收口的、以及剩下真正值得继续修的项目列成待办表，方便后续一项一项推进。

## 判断口径

- `已修掉`：代码里已经能看到实质修复，原审计项的主要风险已经被压下去。
- `还没完全修完`：已经做了一部分，但用户仍可能遇到误导、失败、卡顿或不可信的体验。
- `剩下值得修`：不是所有旧审计项都要立刻修。这里列的是继续修会明显提升产品可信度、稳定性或后续开发效率的项目。

## 已经修掉的

| 状态 | 原审计项 | 当前证据 | 为什么可以先算修掉 |
|---|---|---|---|
| [x] | 测试里 Graph/Sigma/jsdom canvas 噪声影响判断 | `src/test/setup.ts` 已 stub canvas；`SPEC/progress.txt` 记录 P0 Batch 1 已完成 | 以前测试输出里充满 canvas/WebGL 噪声，真正失败很容易被淹没。现在测试环境先把噪声源头处理掉，后续回归更可信。 |
| [x] | `waitForTaskTerminal` 可能永远 pending | `src/lib/waitForTaskTerminal.ts` 已有事件监听、`get_task` 轮询、timeout、cleanup | 长任务最怕“看起来还在跑，其实已经丢事件”。现在即使事件没收到，也会轮询后端；后端一直没结果时也会超时，不会让 UI 永远等下去。 |
| [x] | 首屏 bundle 被 Graph、Milkdown、Markdown renderer、Readability 拖大 | `src/components/app/AppShell.tsx` 已使用 `lazy`/`Suspense`；URL import 才动态加载 `readability`；Wiki editor 也 lazy | 用户打开 app 第一眼应该先看到工作台，而不是为一堆还没用到的重模块买单。现在重功能按需加载，首屏压力小很多。 |
| [x] | Search / Chat retrieval / Graph 重复全量扫 Markdown | `src-tauri/src/services/wiki_index.rs` 已存在；`SearchService` 持有 `WikiIndex` 并在 `scan_wiki` / `search` / retrieval 中复用 | 以前每次搜索、聊天召回、图谱刷新都可能重新读一遍 wiki。现在有 per-project in-memory index，没变的文件可以复用，性能和磁盘 IO 都更稳。 |
| [x] | Graph reducer 里每条边重复建 Map/Set，形成 O(E*N) 热点 | `src/features/graph/GraphView.tsx` 已用 `RenderSnapshot`，edge reducer 读取预计算 hidden set；视觉刷新使用 `skipIndexation: true` | 图谱交互卡顿通常不是用户能解释的问题，只会觉得“不好用”。现在隐藏节点、社区映射这些昂贵计算提前算好，hover/selection 也不再每次全量重建索引。 |
| [x] | URL import 的 Readability 静态进入首屏包 | `AppShell.tsx` 的 URL 分支里 `await import("../../lib/readability")` | 这项属于首屏拆包的一部分。Readability 只有用户真正导入 URL 时才需要，已经不再拖慢普通启动。 |

## 还没完全修完的

| 状态 | 项目 | 当前情况 | 为什么还要继续修 |
|---|---|---|---|
| [ ] | Chat 引用仍然是“检索命中”，不是“答案实际引用” | `ChatService::build_retrieval_context` 先把 Top-N hits 转成 citations，模型回答后没有重新对齐引用 | 这会让用户误会“引用列表就是证据”。如果模型没用某个页面，UI 仍显示它；如果模型说了没证据的话，UI 也可能挂着一堆引用。知识工具最怕这种假确定性。 |
| [ ] | Agent Chat 和 BYOK Chat 的能力边界还混在同一段 prompt 里 | prompt 里写了 Agent 可读 `wiki/`，但 BYOK 模型没有文件系统和工具调用能力 | 用户不会理解“同一个聊天框为什么 Agent 能答、BYOK 答不全”。最好让 BYOK prompt 明说只能看 Sources，Agent prompt 才说可以读本地 wiki。 |
| [ ] | Chat history 没有按 provider context window 做预算裁剪 | 仍按最后 8 turns 拼 prompt；没有根据 `context_window` 粗略估算 | 聊天越久越容易突然 provider 报错，用户看到的可能只是“请求失败”，不知道其实是上下文太长。修这个能减少 BYOK 的随机失败感。 |
| [ ] | Compile 输出校验仍偏弱 | `validate_manifest` 主要查路径安全、重复路径、三张核心页；还不查 derived page 的 `sources` frontmatter 和 `> Sources:` | 编译产物如果没有机器可读 sources，后面的图谱、引用、追溯都会变虚。看起来生成了 wiki，其实证据链断了。 |
| [ ] | 编译仍主要依赖“一锤子生成” | prompt 已经强调 derived pages 和 sources，但没有 few-shot、页面计划阶段、逐页重试 | 弱一点的模型很容易生成骨架页、源文件摘要页，或者 JSON 格式飘掉。分阶段和 few-shot 能让输出更像可维护的 wiki，而不是一次性作文。 |
| [ ] | Anthropic / Google BYOK 请求还没有统一 `temperature: 0` | OpenAI/Custom 已设 0；Anthropic/Google 请求体还没看到等价设置 | 编译 manifest、lint JSON 这类任务需要稳定输出。温度不统一会让“同样操作这次成功下次失败”的感觉更明显。 |
| [ ] | Deep lint 仍只给每页 240 字符摘要 | `DEEP_LINT_EXCERPT_CHARS = 240` | 240 字符基本只能看到开头一小段，模型很难判断重复主题、矛盾、弱关联。它说“没问题”时，用户会以为整库真没问题。 |
| [ ] | Deep lint 没有拿到本地 lint baseline | deep lint prompt 只列 pages，不列本地已发现的死链、孤立页、frontmatter 问题 | Agent 会重复劳动，或者把已知结构问题又换个说法报一遍。把 baseline 传进去，可以让它专心看语义问题。 |
| [ ] | Import preview 的失败原因还不够直接 | 表格显示 failed badge；右侧详情显示 unsupported 提示，但 failed reason/parser/archive-only 状态还不够显眼 | 用户导入失败时最想知道“是文件太大、没文字层、编码不支持，还是解析器不行”。只给 Failed，会让人以为 app 坏了。 |
| [ ] | Task/Checkpoint 可见性做了一部分，但还可以更统一 | Task drawer、等待逻辑已有改善；但高风险任务是否有 checkpoint、影响路径、是否回滚，并非每个入口都同样清楚 | 本地文件工具的信任来自“我知道你改了哪里，我能撤回”。这块越统一，用户越敢让 Agent/编译/auto-fix 动手。 |

## 剩下最值得修的待办

| 优先级 | 待办 | 建议做法 | 为什么值得修 |
|---|---|---|---|
| P0 | [ ] Chat 引用改成“模型实际引用” | 给每个检索 hit 编号，例如 `[1] wiki/concepts/x.md`；要求模型在回答里用 `[n]`；保存消息前解析实际出现的编号，只保留这些 citations；没证据的 claim 标 `[unverified]` | 这是产品可信度的核心。现在引用更像“我找到了这些上下文”，不是“我的答案由这些证据支撑”。如果不修，用户点引用时发现对不上，会很快失去信任。 |
| **P0** | **[ ] 拆 Agent/BYOK Chat prompt** | Agent prompt 可以说能读 `wiki/`；BYOK prompt 顶部写清楚“你没有文件系统和工具，只能使用下面 Sources”；UI 也标注 BYOK 是检索片段回答 | 这能减少很多“同样是 Chat，为什么结果差这么多”的困惑。大白话说，就是别让 BYOK 模型假装自己有手有脚。 |
| **P0** | **[ ] URL 导入支持有限重定向** | **把 `Policy::none()` 改成 limited redirect；每次跳转继续做 public IP / SSRF 检查** | **太多正常网页都会 301/302。现在这种失败不是用户的问题，是 app 太挑。修完以后 URL 导入成功率会立刻变高。** |
| **P0** | **[ ] URL 导入支持 charset 解码** | 后端按 `Content-Type` / HTML meta charset 解码；引入 `encoding_rs` 或等价方案；非 UTF-8 不要直接拒绝 | 中文互联网、老博客、政府站、部分知识库页面经常不是纯 UTF-8。这个不修，中文用户会频繁遇到“明明网页能打开，app 导不进来”。 |
| P0 | [ ] `confirm_import` 改成部分成功 | 每个文件单独确认、单独记录失败原因；成功项照常归档；只有 archive path 被篡改这类安全问题才整批失败 | 批量导入 50 个文件，不应该因为第 47 个被移动了，前 46 个也白干。用户更关心“哪些进来了、哪些失败、为什么”。 |
| P0 | [ ] 统一导入文件大小策略 | 统一 extraction/hash 上限；给超大文件 archive-only；确认导入不要对已经标记 nohash 的文件再硬重哈希 | 现在 64MiB 和 100MiB 两套限制会互相打架。大文件至少应该能安全归档，哪怕暂时不能提取文本。 |
| P1 | [ ] Compile manifest 做语义校验 | 对 derived pages 检查 frontmatter `sources` 非空、正文有 `> Sources:`，路径必须在允许目录；失败返回明确 path 和错误码 | 这能把模型输出从“看起来能用”变成“真的能被图谱、引用、lint 继续使用”。越早校验，越少脏页面进入 wiki。 |
| P1 | [ ] Compile 增加 few-shot 和两阶段生成 | prompt 放一个好页面例子；先生成 page plan，再逐页生成；某页失败只重试某页 | 现在模型像是在一口气写一本小册子，容易漏格式。给例子和分阶段，就像让它先列目录再写正文，稳定很多。 |
| P1 | [ ] 给 Anthropic / Google 也设 deterministic 参数 | Anthropic messages 请求加 temperature 0；Google 走 generation config 设置 temperature 0 | 这项不大，但能减少 JSON/manifest 输出飘忽。对“结构化产物”来说，稳定比创意重要。 |
| P1 | [ ] Deep lint 摘要提升到 800-1200 字符 | 先把常量提上去；后续对重复主题/矛盾候选页再给全文 | 语义检查靠 240 字符太像闭着眼摸一下。多给一点上下文，模型才有机会看出页面是不是重复、矛盾或缺来源。 |
| P1 | [ ] Deep lint prompt 加本地 lint baseline | 先跑 local lint，把 `(path, issueType, severity)` 塞进 deep lint prompt；告诉 Agent 不要重复报这些，重点看语义问题 | 本地规则擅长死链、缺 frontmatter；Agent 擅长语义。两者分工清楚，报告才不会又长又重复。 |
| P1 | [ ] Import UI 展示 parser、失败原因、archive-only | 表格可加一列或 tooltip；右侧 panel 显示 `extractionError`、parser name、是否仅归档 | 用户导入失败时不用猜。失败信息越具体，用户越能自己判断是换文件、换格式，还是继续让 app 归档即可。 |
| P1 | [ ] 保存 Chat answer 到 wiki 时使用实际引用源 | `wiki/queries/` frontmatter 的 `sources` 应来自模型实际 `[n]` 引用，而不是检索 Top-N | 保存到 wiki 的内容会变成长期资产。长期资产的来源如果一开始就不准，后面 lint、graph、export 都会被污染。 |
| P2 | [ ] 清掉生产路径里的 `panic!` / `unreachable!` | 把可恢复情况改成 `BackendError`；保留真正“不可能”的测试或内部断言 | 桌面 app 里 panic 的代价比服务端还糟，用户只会觉得后端炸了。模板缺失、预检不一致这类问题应该给出能看懂的错误。 |
| P2 | [ ] 继续瘦 `AppShell` 和大 service 文件 | 逐步抽 `useImportController`、`useTaskToasts`、`ImportPreviewUseCase` 等，不做一次性大重构 | 现在功能越来越多，很多流程都挤在大文件里。短期还能跑，长期每改一个入口都容易误伤别的入口。 |
| P2 | [ ] Graph edge explanation | 给边加 relation、来源页、依据片段、是否 LLM 生成/待验证 | 图谱不是越花越好，关键是用户能问“为什么这两个点连在一起”。能解释的图谱才像知识工具，不像装饰图。 |
| P2 | [ ] Purpose / Schema 产品化 | Dashboard/Settings 显示是否存在、最后更新、模板来源、lint 是否发现 schema 问题 | `purpose.md` 和 `schema.md` 是 wiki 长期质量的方向盘。现在它们更像隐藏配置，用户不一定知道该维护。 |
| P2 | [ ] Export provenance | 导出记录里写入源 wiki hash、导出配置、时间、包含范围 | 导出的 HTML/PDF 如果以后被质疑“这版从哪里来”，需要能追溯。对长期知识资产来说，可复现很重要。 |

## 暂时不建议优先修的

| 状态 | 项目 | 理由 |
|---|---|---|
| [ ] | 大规模视觉重做 | 当前 shell 已经基本贴近 Codex desktop。现在更大的风险是证据链、导入可靠性和任务信任，不是再调一轮外观。 |
| [ ] | 把 wiki 内容放进数据库 | 项目硬规则是 Markdown + JSON + local files。索引/cache 可以是内存或 `.app/` 派生物，但用户内容不该进数据库。 |
| [ ] | 一口气重构所有 Rust service | 大文件确实存在，但马上大拆会引入风险。更好的顺序是先修热路径和用户可见问题，再沿着 use-case 慢慢拆。 |
| [ ] | 立刻做完整 Agent local HTTP API | 这个方向有价值，但会牵涉权限、token、只读边界、工具调用协议。可以进 roadmap，不适合插在当前 P0 前面。 |

## 建议执行顺序

1. 先修 Chat 引用真实性和 Agent/BYOK prompt 分离。
2. 再修 URL/导入确认/文件上限这组导入可靠性问题。
3. 然后补 Compile manifest 校验、few-shot 和 deterministic 参数。
4. 接着补 Deep lint 上下文和 local baseline。
5. 最后做 Task/Checkpoint 统一展示、Graph edge explanation、Purpose/Schema 产品化。

这条路线的重点是：先让用户相信“答案有证据、导入不会莫名失败、自动化改了什么我看得见”。这些修完以后，再加新功能才比较稳。
