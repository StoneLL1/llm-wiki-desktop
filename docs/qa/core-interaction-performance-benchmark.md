# 核心交互性能测量协议

## 1. 用途与判定边界

本协议为核心交互性能修复 Batch 0–6 提供可复现的本地证据。自动化测试只固定调用次数、状态发布频率、数据完整性和 bundle 闭包；启动、输入响应、long task、Graph 热返回等绝对时延必须在 packaged Tauri WebView 中测量。Vite dev server、浏览器标签页或 jsdom 数据不能替代桌面结论。

所有记录必须关联 commit、构建模式、应用/Tauri 版本、机器环境、fixture 版本和重复次数。没有真实 packaged 环境或无法读取环境字段时写 `Pending`，不得用估算值补齐。

## 2. 隐私与脱敏

允许保存汇总 JSON、bundle graph、脱敏 trace 和统计表。不得记录或提交：

- 知识库正文、真实文件名和绝对路径；
- API key、token、Cookie、凭据状态详情；
- raw provider payload、真实 Chat prompt/answer 或 Agent 命令参数；
- 用户名、设备名以及可反推私人项目的信息。

fixture 统一使用 `perf-page-0001.md`、`fixture-project` 等合成标识。trace 导出前检查 Network、Console、User Timing 和截图内容；只保留事件类型、相对时间、匿名计数和字节数。

## 3. 每次运行的环境记录

| 字段 | 记录要求 |
| --- | --- |
| Commit / dirty state | 完整 commit SHA；有未提交变更时列出本次 Batch 文件，不复制绝对路径 |
| 构建 | `production` / `debug`，执行命令，是否 packaged，Tauri app/CLI 版本 |
| 操作系统 | Windows/macOS/Linux 版本与 build |
| CPU / 内存 | CPU 型号或稳定标识、物理内存容量 |
| WebView | WebView2/WebKit/WKWebView 版本 |
| 图形环境 | GPU 型号、硬件加速或 fallback、驱动版本 |
| 会话环境 | 本地/远程桌面、电源模式、前后台状态 |
| 安全软件 | 实时防护开启/关闭；只写类别和状态，不写产品账户信息 |
| 显示 | 分辨率、缩放、刷新率 |
| Fixture | 生成器/测试版本、seed、页/节点/边/字节/delta 数 |

Windows 参考机应同时记录杀软开启和 GPU fallback/远程桌面 smoke；macOS/Linux 若没有真实 runner，发布前状态保持 `Pending`。

## 4. 固定 fixture

fixture 必须由确定性生成器或测试内存数据创建，固定 seed 为 `llm-wiki-perf-v1`，不得提交生成后的巨型目录或 trace。

| Fixture ID | 固定规模与内容 | 用途 |
| --- | --- | --- |
| `wiki-500-v1` | 原生测试知识库；500 个约 4 KiB Markdown 页面，稳定双向链接；无私人内容 | 普通板块重入、500 页 Graph |
| `graph-10k-v1` | 10,000 节点、40,000 条确定性边；测试内存数据或临时 cache | 压力 smoke，不作为普通产品门槛 |
| `chat-256k-1k-v1` | 最终 UTF-8 可见文本 262,144 B，恰好 1,000 个有序 delta，包含 CJK、代码块、数学和长段落 | 正常长流 publication/解析/滚动 |
| `chat-256k-10k-v1` | 与上一项逐字节相同终态，恰好 10,000 个有序 delta | 高频 delta 压力与尾部完整性 |
| `chat-history-long-v1` | 100 轮合成长 Markdown 历史，总可见文本至少 1 MiB | 历史渲染与生成期间输入 |
| `route-loop-20-v1` | 同一 `wiki-500-v1` 项目连续执行 20 次 Wiki → Chat → Graph → Exports → Lint → Wiki | 热切换、IPC、spinner、状态恢复 |
| `splitter-2s-v1` | sidebar、right panel、Wiki tree、Exports list、Lint details 分别连续拖动 2 秒 | input-to-paint、commit/storage、long task |

生成器必须写入显式临时目录并在运行结束后删除；不得覆盖现有目录。Chat fixture 的 1,000/10,000 delta 由同一最终 byte buffer 按确定性边界切分，验收时重新拼接并做 byte equality。

## 5. 冷/热定义与采样规则

- **Cold start**：结束所有应用进程，确认无后台 WebView；使用新启动的 packaged binary。不得清理操作系统文件缓存冒充普通冷启动；若额外测试“清 OS cache”，须单独命名。
- **Warm start**：同一构建、同一 fixture 已成功打开一次，正常退出后 30 秒内再次启动；不修改项目文件或设置。
- **Cold route**：该进程中首次进入目标板块。
- **Hot return**：目标板块至少成功显示一次，期间未修改 fixture，按规定路由序列返回。

启动各做至少 10 次；板块往返、Graph 热返回、Chat 和 splitter 各做至少 20 次。原始样本按场景分开，失败/取消样本不得静默删除；异常值只有在记录明确外部原因后才可另表展示。

统计采用 nearest-rank：排序后 `p50 = x[ceil(0.50 × n)]`，`p95 = x[ceil(0.95 × n)]`（下标从 1 开始）。同时记录 n、min、max 和失败数。持续动作的 input-to-paint 对每个输入样本计算，再汇总 p50/p95；不要先平均每次运行再算百分位。

## 6. Trace 起止与场景步骤

每次 trace 开始前静置 2 秒，结束动作完成后再静置 2 秒。使用相同窗口尺寸和系统缩放。

| 场景 | Trace 起点 | 固定动作 | Trace 终点 / 记录项 |
| --- | --- | --- | --- |
| Cold/warm 启动 | 启动 packaged binary 前 | 打开应用并等待 shell 可交互 | 首次 shell paint、interactive、long task、入口文件读取 |
| 项目状态探测 | 点击打开 `wiki-500-v1` 前 | 打开项目并保持 Dashboard | shell 稳定；Git/Agent/provider IPC 次数和完成时间 |
| 普通板块热切换 | 第一次 Wiki 已稳定 | 执行 `route-loop-20-v1` | 每次导航到可交互、全页 loading、完整扫描 IPC、p50/p95 |
| Graph | 点击 Graph 前 | 首次进入、操作选择/过滤/搜索/镜头，离开后返回 | 首次/热返回、500 页 p95、状态恢复；另跑 10k smoke |
| Chat stream | 发送 fixture 消息前 | 回放指定 1k/10k delta；中途输入、拖 pane、滚离底部 | terminal 后持久会话稳定；publication、React commit、parse、scroll、GC、byte equality |
| Splitter | pointerdown 前 | 每种 splitter 按固定路径拖 2 秒 | pointerup 后一帧；input-to-paint、store/storage 次数、long task |

用户主动滚离 Chat 底部后必须单独标记，确认后续 delta 不强制拉回。Chat terminal 分发前的同步 flush 作为一次允许的额外 publication 单独计数。

## 7. 指标与通过条件

| 指标 | 采集方式 | 门槛 |
| --- | --- | --- |
| 初始 JS 闭包 | `npm run build` 后 `npm run check:bundle` | Batch 0 非回归：raw ≤ 1,610,000 B；gzip ≤ 470,000 B；JS 文件 ≤ 45 |
| 项目 facts IPC | Tauri command 计数/脱敏 trace | 首次全部 shell consumer：Git/Agent/provider 各 1 次；新鲜命中 0 次新增 |
| Pane 写入 | Profiler + store/storage spy + trace | 连续 drag 只 commit 1 次、`localStorage.setItem` 1 次；move 不直接写 store |
| Chat publication | store instrumentation | ≤ 25 Hz，terminal 前允许额外 flush 1 次；10,000 delta 终态 byte-equal |
| 普通热切换 | User Timing / trace | 新鲜 cache p95 ≤ 150 ms，无全页 loading、无重复完整扫描 |
| 500 页 Graph 热返回 | User Timing / trace | p95 ≤ 500 ms，选择/过滤/搜索/镜头可预测恢复 |
| 主线程输入 | Performance trace | 本轮路径无 >50 ms long task；input-to-paint p95 ≤ 100 ms |

Batch 5 需要相对本文件 Batch 0 基线至少降低 raw 30%、gzip 25%，并把预算收紧到实测值上方的小余量；不得通过全量 preload、粗粒度 `manualChunks` 或增加单一巨大 vendor chunk 达标。

## 8. Batch 0 重建基线（2026-08-15）

本节只记录 production bundle；尚未执行 packaged Tauri 交互测量。

| 字段 | 当前记录 |
| --- | --- |
| Commit | `1ea9ad2d2609756a16fc270cdeca3b7bd9dffed1`，另含 Batch 0 构建门禁的未提交改动；无运行时 feature 改动 |
| Build | `npm run build`，production/minified；app `0.1.0`，Tauri CLI `2.9.5` |
| OS | Windows NT `10.0.26200.0` |
| CPU | `AMD64 Family 26 Model 36 Stepping 0` |
| 内存 / WebView2 / GPU / 显示 / 杀软 / RDP | `Pending` |
| Packaged 指标 | `Pending`；不得用本次 Vite build 时间代替 |

机器可读图为构建产物 `dist/bundle-graph.json`（被 `dist/` 忽略，不提交）；模块路径相对仓库或归一为 `node_modules/...`，不写本机绝对路径。

| 指标 | 重建值 | Batch 0 上限 |
| --- | ---: | ---: |
| 初始 JS 文件数 | 45 | 45 |
| 初始 JS raw | 1,593,268 B | 1,610,000 B |
| 初始 JS gzip | 462,400 B | 470,000 B |
| 初始 CSS 文件数 | 1 | 仅记录 |
| 初始 CSS raw | 244,349 B | 仅记录 |
| 初始 CSS gzip | 52,937 B | 仅记录 |

最大静态可达模块贡献者依次为 KaTeX、React DOM client、英文 locale、中文 locale 和 i18next；这些是 Batch 5 的依赖泄漏诊断输入，不在 Batch 0 改动或拆包。

## 9. Batch 4C Graph 有界保活决策（2026-08-16）

在 Windows 参考机使用 packaged debug executable 和本文件固定 fixture 完成 Graph 决策测量。完整环境、原始 20 次样本、计数、heap 与 fallback 结果保存在 [`results/2026-08-16-core-interaction-performance-batch-4c.json`](results/2026-08-16-core-interaction-performance-batch-4c.json)。本次仅使用临时 build-only 计数器，最终源码和验证构建不包含诊断入口。

| 场景 | 结果 | 判定 |
| --- | --- | --- |
| `wiki-500-v1` 热返回 | n=20；p50 33.8 ms；p95 44.1 ms；max 45.8 ms | 通过 p95 ≤ 500 ms |
| `wiki-500-v1` 主线程 | 0 个 >50 ms long task | 通过 |
| `wiki-500-v1` 热循环资源 | `get_graph` 0；renderer create/kill 20/20；worker start 0 | 现有卸载模型释放完整，无 stale worker/timer |
| `graph-10k-v1` smoke | p95 350.0 ms；20/20 次存在 >50 ms long task；JS heap max 128,364,997 B | 记录为压力风险，不替代 500 页产品门槛 |
| WebGL 强制不可用 | 显式 fallback 可见；2 秒内仅 1 次 init failure；0 次成功 create | 通过，不重复创建 |
| GPU memory / RDP | `Pending` | WebView2 不暴露 GPU memory；真实 RDP 会话留给 Batch 6/release gate |

Batch 4C 的强制分支结论是 **停止，不实现 Graph warm host**。500 页热返回同时满足 p95 和 long-task 门槛；继续保活 renderer 只会增加隐藏 DOM、GPU/heap 常驻、listener/TTL/project-switch 竞态，并违反“先测后决定”。现有 4B camera/data 恢复与正常 mount/unmount 生命周期保持不变；Batch 5 继续只负责 lazy 边界和首屏预算。

## 10. Batch 6 整体验收（2026-08-16）

Windows 参考机使用 packaged debug executable 完成最终自动化、启动、路由、Graph、Chat 与 splitter 测量。脱敏汇总、失败项和 Pending 平台项保存在 [`results/2026-08-16-core-interaction-performance-batch-6.json`](results/2026-08-16-core-interaction-performance-batch-6.json)；CDP/Ollama 测量驱动位于 [`../../scripts/run-core-interaction-packaged-benchmark.mjs`](../../scripts/run-core-interaction-packaged-benchmark.mjs)。Chat 的 256 KiB fixture 由脚本确定性生成；本次 500 页 route fixture（500 个 Markdown、2,069,919 B）在独立临时项目中准备，脚本尚未自动生成或校验该规模，因此 route 数据只作观察证据。原始合成回答、绝对路径和 raw trace 未提交。

| 场景 | 结果 | 判定 |
| --- | --- | --- |
| 聚焦测试 / bundle / quick | 43 files、533 tests；38 个初始 JS 文件，616,366 B raw / 187,483 B gzip；quick gate 通过 | 通过 |
| fresh-profile / warm-profile debug 启动 | 各 n=10；interactive p50 约 9.62 s，p95 分别 9.77 / 9.77 s | 仅记录；前者每次使用全新 WebView profile，不等同本协议的稳定 profile 进程冷启动 |
| 首次项目 facts IPC | single-flight/TTL/force/retry/A-B 自动化合同通过 | packaged command 计数 `Pending`；Tauri `invoke` 在 WebView 中不可写，未把不可观测结果伪报为 0 |
| 20 次板块往返 | 500 页临时 fixture 的 click→`aria-current`+2 RAF CDP proxy 五路 p95 23.8–94.0 ms；追加合成 Chat session 后 p95 24.0–99.9 ms | 仅观察；没有 route-specific ready、逐次 loading 或 IPC 证据，`PERF-P2-01` 不关闭 |
| 500 / 10k Graph | 复用 Batch 4C：500 页 p95 44.1 ms 且无 long task；10k p95 350.0 ms 但 20/20 有 long task | 普通产品门槛通过；10k 保持压力风险 |
| 256 KiB Chat | 1k/10k delta 各 20 个已完成样本，40/40 终态 262,144 B byte-equal；中途输入、pane、滚离底部和 Wiki↔Chat 往返可执行 | 数据完整通过；终态附近相关 long task 峰值约 1.7 s 且路由往返后草稿丢失，`PERF-P1-01` 不关闭 |
| 五种 splitter | 每种 20 个已完成 drag、2,400 个 input 样本；input→next-RAF CDP proxy p95 17.82–18.03 ms | 不是 presentation/input-to-paint 指标；稳态 trace 仍有 >50 ms long task，部分重复 CDP drag 未产生有效宽度变化，`PERF-P2-02` 不关闭 |
| Windows fallback / RDP / AV | WebGL fallback 通过；GPU memory、真实 RDP、杀软开启状态不可得 | 后三项 `Pending` |
| macOS / Linux | 无真实 runner | 发布前 `Pending` |

本次是验收 Batch，不把失败测量转化为未授权的后续实现。仅 `PERF-P1-02` 有足够证据关闭；`PERF-P1-01`、`PERF-P1-03`、`PERF-P2-01`、`PERF-P2-02` 因真实失败或缺失 packaged 证据保持未关闭。最初采样版本只在整轮成功后输出 JSON，未保存中断/失败尝试，因此结果中的次数均表述为“已完成样本”，不推导零失败；提交的脚本已加入逐次 attempt ledger、CDP timeout/socket-close rejection 和 `finally` 清理，供后续复测。后续应另行批准终态渲染主线程归因与优化、Chat draft 路由保留、build-supported IPC 计数、route-specific ready 观测与 element-targeted splitter trace；不得在本 Batch 顺带扩大重构。

可复现命令形态如下；`<...>` 必须指向显式临时目录或合成项目，禁止对真实知识库运行 Chat fixture：

```text
node scripts/run-core-interaction-packaged-benchmark.mjs --mode startup --exe <packaged-exe> --app-data <isolated-appdata> --webview-root <isolated-webview-root> --runs 10
node scripts/run-core-interaction-packaged-benchmark.mjs --mode routes-and-splitters --endpoint http://127.0.0.1:<debug-port> --splitter-repetitions 20
node scripts/run-core-interaction-packaged-benchmark.mjs --mode chat --endpoint http://127.0.0.1:<debug-port> --project-id <synthetic-project-id> --project-root <.../fixture-project> --confirm-synthetic-fixture yes --chat-repetitions 20
```

Chat 模式会修改 provider 配置并创建 session，只能用于显式确认、运行后整体丢弃的 `fixture-project`；不得指向真实知识库。脚本默认输出汇总和不含原文的 attempt ledger；仅在本地诊断需要逐样本/browser metrics 时追加 `--output-detail raw`，且 raw 输出不得直接提交。

## 11. 结果记录模板

```text
Commit / build / packaged:
OS / CPU / RAM / WebView / GPU / AV / RDP / display:
Fixture ID / seed / exact counts:
Scenario / repetitions / failures:
Metric: min / p50 / p95 / max / unit:
Long tasks (>50 ms):
IPC / store publications / React commits / storage writes:
Correctness checks (byte equality, state restoration, focus/ARIA):
Trace filename (desensitized) / notes / Pending items:
```
