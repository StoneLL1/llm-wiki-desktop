# 小红书 Import 实际验收（2026-09-06）

本记录补充 [Import 架构验收](import-architecture-refactor-acceptance.md)，以当前公开笔记形成可重读 Source 为成功条件。仅解析出标题、只有原链接、等待登录或零篇提交，均不计作导入成功。

## 修复与行为

- URL 输入支持一条完整小红书分享文案，提取其中唯一链接，保留访问所需的签名参数。正式入队仍使用已有安全 URL 交接；敏感查询参数不进入队列展示和 Source Markdown。
- 小红书及其短链域名使用真实应用 HTTP 标识。原 Chrome 导航标识在同一条可公开阅读的笔记上被重定向到 JavaScript 登录空壳，导致此前误报结构变化。微信实测依赖既有请求配置，因此其他平台保留原配置；每次跳转继续执行既有 URL、DNS、范围和取消检查。
- 有完整配文的照片笔记可立即预览和保存，图片文字识别作为可选动作。当前轻量判断为：去除空白分隔的话题标签后，配文至少含 80 个 Unicode 字母或数字；这只是可读配文的启发式，不声称理解图片内容。短配文及纯话题标签轮播仍先启用 OCR，缺组件时复用现有“一次准备并继续”。
- 图片保持原顺序，每张识别文字直接位于对应图片下方；识别失败也定位到该图。没有可读配文且全部识别失败的条目不能保存成空 Source。
- 视频继续优先可靠字幕，无字幕时明确启用本地 ASR；复用现有媒体组件和 Source 事务，没有另建小红书下载器、登录服务或 AI 解析补救链。
- HTTP 200 的 `/404` 与已失效链接返回 `IMPORT_WEB_LINK_UNAVAILABLE`，提示重新复制完整当前分享链接；登录、验证码、内容移除保持独立状态，不误触发结构修复。

## 真实样本

环境为 Apple Silicon macOS、未优化 debug 应用。样本均来自公开页面观察到的实际链接，访问时保留完整签名查询；以下只记录笔记 ID，避免将临时访问参数提交到仓库。原始知识库未参与测试。

| 样本 | 结果 | 观测耗时与边界 |
| --- | --- | --- |
| 照片 + 完整配文 `6a754e4f0000000025006fb2`，9 张图 | 保留作者配文及 9 张本地原图，预览、保存、重读 Source 通过 | 最终跨平台回归候选 4.67 s；前次测量 1.77 s。此前强制识别 9 张照片需 99.88 s，且没有得到有用图片文字。网络及并行编译造成波动，不作为 p95 基准 |
| 图片文字轮播 `6a7baa430000000012029166`，10 张图 | 真实中文 OCR，10 张均有对应文字，保存、重读 Source 通过 | 最终后端复测 118.98 s，桌面一次确认到预览 160.45 s；官方已安装 OCR runner/模型，未新增下载；无未解析占位符 |
| 视频 `6a73fcf20000000005021c05` | 无公开字幕 → 明确启用 ASR → 真实本地转录 → 保存、重读 Source | 首次视频获取及识别 79.37 s，最终重复验收 25.36 s（不作为冷启动基准）；使用修复原生库签名后的开发 SenseVoice/CoreML 包，不能代表旧已发布包已修复 |
| 旧失败样本 `67ea1ed7000000001e00424e` | 其页面实际重定向到“当前笔记暂时无法浏览”；不生成 Source | 最终实测 0.75 s 返回 `IMPORT_WEB_LINK_UNAVAILABLE`，零篇提交；失效链接不代表当前笔记结构无法解析 |
| MDN Authentication + 微信 `FAWINkUQGaLSHLAWlgHtmA` | 两篇均保存并重读，验证请求修改没有影响已有文章路线 | 最终候选分别 3.24 s / 1.84 s；MDN 正文 13,423 B，微信 8,690 B |

## 验证入口

`src-tauri/tests/import_live_url_journey.rs` 现在默认要求每条输入都生成可提交预览，随后核验保存数、标题、正文、参数脱敏和图片占位符。故意验证拒绝场景时才显式指定 `IMPORT_LIVE_EXPECT_COMMITTED=0`。测试始终创建临时知识库；普通 CI 不联网、不下载模型。

```sh
# URL 必须是实际复制的完整当前链接；不要把签名参数提交到仓库。
IMPORT_LIVE_URLS='["<complete current public share URL>"]' \
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features \
  --test import_live_url_journey -- --ignored --nocapture

# 可选：从已受信安装目录加载 OCR；显式允许该次测试识别图片文字。
IMPORT_ACCEPTANCE_INSTALLED_ROOT='<installed-capabilities directory>' \
IMPORT_ACCEPTANCE_OCR=1 \
IMPORT_LIVE_URLS='["<complete current image-text note URL>"]' \
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features \
  --test import_live_url_journey -- --ignored --nocapture
```

稳定回归覆盖分享文案、签名参数交接、目标笔记识别、短链域名、重定向错误分类、配文与 OCR 分流、每图 OCR 放置、失败隔离和中英文动作文案。按用户要求，本轮由主代理进行设计/集成与独立盲点两种视角的本地审查，不重复调用 review 子代理。

## 最终桌面验证

最终 debug `.app` 打包成功。在独立标识的验收应用与临时知识库中完成整段分享文案输入：9 图照片笔记直接预览、保存；10 图文字笔记点击一次“确认并继续”，复用已装 OCR，切到 Wiki 后后台继续，返回时预览就绪，保存成功。退出并重启应用，从 Wiki 文件树重新打开两篇 Source，图像实际渲染、十段 OCR 按图排列、版本与内容摘要均保持一致。原有四篇 Source 和证据共 27 个文件摘要未变。

桌面验证未制造第二次缺 OCR 状态或再次下载同一组件；缺组件的一次准备、后台接续与断点恢复沿用已经完成的[架构验收](import-architecture-refactor-acceptance.md)。本轮实际覆盖已装 OCR 的复用。

最终 `npm run check` 从头完整通过，耗时 **17m 0.7s**：前端 **147 个文件 / 1331 个测试**，Rust **1289 个单元测试（4 个按设计 ignored）**及全部常规集成测试通过；lint、工具测试、生产前端构建、首屏包体、命令边界与 Rust GUI 编译通过。真实联网/模型测试额外执行，不计入普通 CI 的默认成功。两种视角本地审查检查了目标笔记绑定、参数脱敏、既有平台回归、OCR 放置与空正文保护；没有重复启动 review 子代理。现有 graphify CLI 不可用，未更新导航图。

## 未覆盖的边界

- 本轮未取得真实 `xhslink` 短链成功样本，短链识别/重定向仍只有稳定回归证据；实时成功使用完整公开长链接。
- 未验证登录账号后的受限笔记、验证码交互、已删除或仅 App 内可访问内容；这些状态不会被转换成“已成功导入”。
- 未验证 Windows、Intel macOS、Linux GUI，也未发布能力资产。旧已发布 macOS ASR 包的签名修复仍需走正式发布流程。
- 长 OCR 运行中，通用队列曾暂留旧的等待确认文案，后台任务持续识别，终态正常更新为可提交。本轮未修改通用批次的中间状态投影。
- 配文长度不能完全判断图片是否承载主要内容；用户仍可在保存前选择补充 OCR。模型精度与服务端变化不在此验收中作永久保证。

本地复核证据保留在 `/tmp/import-xhs-validation/`，包括真实 journey 日志和临时知识库位置。临时完整链接及页面快照不进入版本控制。
