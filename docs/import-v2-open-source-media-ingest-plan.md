# Import V2：小红书 / 抖音 / B 站图文视频导入实现方案

> 历史实现计划：本文可继续用于平台适配、开源组件和运行时证据参考，但不再定义产品行为。2026-07-24 已确认的登录态、OCR / ASR、字幕优先级、远程媒体保存、图文结构、复合来源、Source 提交与独立编译流程，以 [`superpowers/specs/2026-07-24-import-source-media-flow-design.md`](superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为唯一可信源。

> 状态：历史研究与实现记录，不可直接作为当前实施计划
>
> 历史适用范围：当时的 `Import V2` 平台研究；当前实现必须先回到上方权威规范校验产品行为。
>
> 目标：将小红书图文/视频、抖音图文/视频、Bilibili 视频稳定导入为“原始来源 + 本地媒体 + 转写/字幕 + 结构化 Markdown + 可追溯元数据”。

## 1. 结论先行

推荐采用“当前项目的 Import V2 编排内核 + 外部签名能力包”的组合，不把任何平台爬虫代码直接嵌进 Tauri 主进程，也不把一个单一爬虫项目当成三大平台的长期抽象。

```text
用户 URL / 文件
        │
        ▼
ImportV2Service
  ├─ URL 规范化、SSRF/私有地址校验、来源去重
  ├─ DomainRouter：按平台选择路线和回退
  ├─ ConnectorSession：用户显式登录的隔离浏览器 profile
  ├─ CapabilityPack：外部下载器、浏览器、ASR、OCR
  ├─ MediaRouter：平台字幕优先，ASR 作为授权后的续接步骤
  ├─ QualityGate：完整性、可读性、资产与时间轴质量
  ├─ Staging：先写暂存区，不直接污染 raw/wiki
  └─ Preview → 用户确认 → SourceRegistry / raw / extracted 提交
```

### 1.1 建议的开源组件组合

| 能力 | 首选 | 在本项目中的使用方式 | 许可证/边界 |
|---|---|---|---|
| 通用媒体与 B 站下载 | [yt-dlp](https://github.com/yt-dlp/yt-dlp) | 以签名 capability pack 或随应用发布的受校验二进制运行；通过 JSON 输出和文件清单交互 | 仓库源码为 Unlicense，但发行二进制包含第三方组件，必须随发行物核对 `THIRD_PARTY_LICENSES.txt` |
| 小红书元数据/图片/视频 | [XHS-Downloader](https://github.com/JoeanAmier/XHS-Downloader) | 作为可替换的 XHS provider；只取单条作品和媒体，不复制其 CLI/服务层到核心 | GPL-3.0；不建议静态链接或直接复制代码进主仓库 |
| 三平台研究型抓取参考 | [MediaCrawler](https://github.com/NanmiCoder/MediaCrawler) | 参考其浏览器登录态、页面数据模型和平台适配思想；如单独分发，必须独立审核 | 当前 LICENSE 是非商业学习许可，不作为商业发行核心依赖 |
| 抖音单条/批量媒体 | [douyin-downloader](https://github.com/jiji262/douyin-downloader) | 作为可选 Douyin provider 的实现参考或独立 sidecar；优先复用其重试、断点、SQLite 去重思想，不引入 SQLite 作为 Wiki 内容库 | MIT；平台抓取逻辑仍需单独维护，批量能力在本项目默认关闭 |
| 抖音 → Obsidian 输出契约 | [Douyin Capture Pro](https://community.obsidian.md/plugins/douyin-capture-pro) / [原始仓库](https://github.com/lyxdream/obsidian-douyin-capture) | 参考其“图片帖保留全部图片、视频帖本地 Whisper、Frontmatter、部分失败也生成笔记”的降级行为 | 参考输出契约，不直接复制插件运行时 |
| B 站 AI 笔记 | [BiliNote](https://github.com/JefferyHcool/BiliNote) | 参考字幕优先级、Markdown 章节、截图和跳转链接；不复用其应用级状态管理 | MIT；其 README 的部分平台支持描述存在交叉/待办项，必须以当前代码验证 |
| 中文本地 ASR | [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) + [FunASR](https://github.com/modelscope/FunASR) | `media.asr` capability pack；中文、粤语、英文等优先；输出带时间戳的 JSON/VTT | 工具与模型权利分开核对；模型不可只凭 Python 包许可证判断 |
| 通用/低资源 ASR | [faster-whisper](https://github.com/SYSTRAN/faster-whisper) | 作为跨平台 fallback 或与 WhisperX 组合；使用 CTranslate2 和量化模型 | MIT；模型权利另行核对 |
| 字级时间轴/说话人 | [WhisperX](https://github.com/m-bain/whisperX) | 二期可选；只有在用户启用“精确时间轴/说话人”时加载，不阻塞基础导入 | MIT；diarization 模型有额外授权要求 |
| OCR/图片文字 | [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | 独立 `ocr.cjk-accurate` capability pack；只做证据提取，默认不参与首期导入阻塞 | Apache-2.0；模型及推理依赖按版本随包登记 |

### 1.2 不推荐的做法

1. 不将 `MediaCrawler`、`XHS-Downloader` 或 GPL 项目源码复制进 Rust/TypeScript 主仓库。许可证、平台规则、更新频率和维护边界都会把核心项目绑定在一起。
2. 不让 React 直接执行 `yt-dlp`、读取 Cookie、访问浏览器 profile 或写 `raw/`。这些操作必须经过 Tauri command、ImportV2 service 和受限 capability runtime。
3. 不以“抓到一段文本”为成功标准。成功必须同时记录来源 URL、平台元数据、原始媒体/资源、提取路线、字幕/ASR 来源、内容哈希和质量结果。
4. 不默认批量抓取、评论抓取或账号画像。当前目标是用户主动提交的单条 URL/本地文件；批量能力应是显式开关、有限速率、可取消任务。
5. 不把 OCR、视频关键帧理解和 LLM 摘要塞进基础导入的同步路径。基础导入先提供可验证证据，增强处理在预览后或编译阶段异步执行。

## 2. 与当前项目的边界和复用点

当前项目已经有一套适合该场景的 Import V2 骨架，方案应扩展而不是另起一套导入系统。

| 当前模块 | 现有职责 | 本方案的复用/扩展 |
|---|---|---|
| `ImportV2Service` / `orchestrator.rs` | 会话、任务编排、能力注册、提交和恢复 | 增加媒体平台 provider 的路由，不在 connector 中实现提交逻辑 |
| `DomainRouter` | 微信、知乎、B 站、小红书及通用 Web 路由 | 将 `xiaohongshu` 从 release gate 后的占位路线扩展为 provider 链；抖音新增域名路线 |
| `CapabilityPackManager` / `ImportCapabilityRuntime` | 签名 manifest、版本、平台、许可证、文件哈希和健康状态 | 注册 `media.capture.xhs`、`media.capture.douyin`、`media.capture.bilibili`、`media.asr.sensevoice` 等能力 |
| `PackEngine` / `pack_protocol` | 外部进程、JSON-RPC、超时、取消、stderr 限额、路径校验 | 作为所有第三方下载器/ASR/OCR 的统一进程边界 |
| `ConnectorSessionService` | 隔离浏览器 profile、登录任务、绑定 import item、登录后一次性转交 profile | XHS/Douyin/B 站登录失败时使用，不存原始 Cookie JSON |
| `WebTargetStore` | 私有 URL 引用、过期、目标哈希、B 站 ASR 授权 | 复用授权与 URL 私密参数的 OS credential storage |
| `connectors/bilibili.rs` | B 站标题、作者、描述、字幕候选和 ASR 计划 | 保留为 native metadata connector；下载视频由 capability pack 完成 |
| `connectors/xiaohongshu.rs` | XHS 结构化 JSON 解析、图片 URL、登录/验证码错误 | 作为 provider 结果的校验器；不把页面 HTML 直接当最终 Markdown |
| `MediaRouter` | 人工字幕/平台字幕/自动字幕/嵌入字幕/ASR 的优先级 | 继续作为唯一字幕选择器；平台 provider 只提供候选 |
| `OcrRouter` | OCR pack 选择、缓存键、模型哈希和 CJK 路由 | 作为图片帖和视频关键帧的可选后处理 |
| `SourceRegistry` | locator/content hash 去重、版本、manifest、wikiPath | 作为三平台来源的统一身份和变更检测 |
| `QualityGate` | 文本覆盖、结构、资源、质量拒绝和回退 | 增加媒体下载完整性、字幕连续性、时间戳和元数据字段校验 |
| `raw/` + `.app/` | 原始来源、提取结果、会话和索引 | 不引入数据库存放 Wiki 内容；provider 的内部临时索引只能留在 staging 或 `.app` |

### 2.1 核心职责边界

```text
平台 provider：     认识平台、解析平台返回、列出媒体/字幕候选
能力 pack：         下载、浏览器登录、ASR、OCR等外部运行时
ImportV2：           路由、权限、路径、安全、任务、质量、暂存和提交
SourceRegistry：    内容身份、版本、去重、来源历史
Wiki compile：      用户确认后的摘要、链接、标签、图谱和知识页面
```

平台的 HTML、API、签名和反爬行为会变化，因此 provider 必须是可替换的；ImportV2 的数据契约和提交协议不能依赖某个平台的内部字段。

## 3. 导入对象模型

### 3.1 输入模型

一个用户可见的导入项必须在创建时分配稳定 `itemId`，后续每一次重试、登录续接和 ASR 续接都使用同一个 ID。

```json
{
  "itemId": "item-uuid",
  "kind": "url",
  "locator": "https://www.bilibili.com/video/BV...",
  "displayName": null,
  "requestedFeatures": {
    "downloadOriginalMedia": true,
    "platformSubtitles": true,
    "localAsr": "ask",
    "ocrImages": "off",
    "videoKeyframes": "off",
    "generateSummary": false
  },
  "limits": {
    "maxMediaBytes": 1073741824,
    "maxDurationSeconds": 7200,
    "maxImageCount": 100
  }
}
```

`generateSummary` 在 Import V2 阶段默认为 `false`。摘要应在 `PreviewReady` 之后、用户确认编译时执行，避免把不可验证的模型生成结果混入原始提取层。

### 3.2 统一的媒体来源模型

各平台 provider 必须把平台差异归一化到同一个模型：

```json
{
  "platform": "xiaohongshu",
  "contentType": "image_post",
  "canonicalUrl": "https://www.xiaohongshu.com/explore/abc",
  "platformId": "abc",
  "title": "标题",
  "author": {
    "id": "user-id",
    "name": "作者",
    "profileUrl": "https://..."
  },
  "publishedAt": "2026-07-20T08:00:00Z",
  "description": "正文和话题标签",
  "hashtags": ["AI", "Markdown"],
  "media": [
    {
      "assetId": "asset-1",
      "kind": "image",
      "sourceUrl": "https://...",
      "mime": "image/jpeg",
      "width": 1080,
      "height": 1440,
      "order": 0,
      "role": "content"
    }
  ],
  "subtitles": [],
  "chapters": [],
  "sourceEvidence": {
    "provider": "media.capture.xhs",
    "engineVersion": "1.0.0",
    "capturedAt": "2026-07-20T08:10:00Z",
    "responsePath": "evidence/provider-result.json"
  }
}
```

### 3.3 字幕和转写模型

字幕、平台自动字幕、本地 ASR 必须区分来源，不能只存一个 `transcript` 字符串。

```json
{
  "transcription": {
    "sourceKind": "platform_human",
    "language": "zh",
    "engineId": "bilibili-caption",
    "engineVersion": "platform",
    "modelSha256": null,
    "segments": [
      {
        "id": 0,
        "startMs": 0,
        "endMs": 2400,
        "text": "今天讨论本地优先知识库。",
        "speaker": null,
        "confidence": null
      }
    ],
    "quality": {
      "coverage": 0.99,
      "overlapCount": 0,
      "outOfOrderCount": 0,
      "emptyCount": 0
    }
  }
}
```

允许的 `sourceKind`：

- `platform_human`：平台提供的人工作者字幕。
- `platform_automatic`：平台自动字幕。
- `embedded`：视频文件或下载器发现的内嵌字幕。
- `local_asr`：本地 FunASR、SenseVoice、faster-whisper 或 WhisperX 生成。
- `none`：没有字幕/转写；这不是失败，而是必须在预览中说明。

## 4. 统一任务状态机

任务状态要可恢复、可取消、可显示进度，并支持“登录后继续”和“授权 ASR 后继续”。

```text
Queued
  ↓
Inspecting
  ├─→ WaitingCapability ── capability ready ──┐
  ├─→ WaitingLogin ─────── login complete ─────┤
  ├─→ WaitingAuthorization ─ user approves ────┤
  └─→ Failed / Skipped                         │
                                               ▼
Downloading / Capturing
  ↓
ExtractingMetadata
  ↓
SelectingSubtitles
  ├─→ PreviewReady（平台字幕/内嵌字幕）
  ├─→ WaitingAuthorization（需要本地 ASR）
  ├─→ Transcribing（用户已授权）
  └─→ PreviewReady（无字幕，明确降级）
                                               ↓
                                    OptionalOcr / OptionalKeyframes
                                               ↓
                                          PreviewReady
                                               ↓ 用户确认
                                       CommitToProject
                                               ↓
                                           Completed
```

建议的 item 状态：

| 状态 | 含义 | 可恢复动作 |
|---|---|---|
| `queued` | 已加入会话，尚未执行 | 开始/取消 |
| `inspecting` | 规范化 URL、识别平台、探测能力 | 等待或取消 |
| `waiting_capability` | 未安装/未通过校验的能力包 | 安装后重试；不自动安装 |
| `waiting_login` | 平台要求显式登录 | 打开隔离浏览器并绑定当前 item |
| `waiting_authorization` | 需要用户批准本地下载/ASR/私有资源 | 授权一次且绑定目标哈希 |
| `capturing` | 正在获取元数据、媒体或字幕 | 取消后保留可恢复 staging |
| `transcribing` | 正在执行本地 ASR | 取消，保留音频和 checkpoint |
| `extracting` | 正在解析字幕、封面、图片和媒体元数据 | 重试当前阶段 |
| `preview_ready` | 已生成可审阅的导入预览 | 修改选项、确认提交 |
| `partial` | 有可用结果但部分资源失败 | 重试失败资源或确认部分提交 |
| `failed` | 没有可安全提交的结果 | 查看错误、换路线、重试 |
| `cancelled` | 用户取消 | 恢复或清理 staging |
| `completed` | 已写入 raw/extracted 并更新索引 | 打开来源或进入编译 |

所有状态事件统一通过现有任务事件通道发出：`task_id`、`session_id`、`item_id`、`stage`、`progress`、`message`、`recoverable`、`requires_user_action`。日志只记录 provider、错误码和路径，不记录 Cookie、Authorization、完整私密 URL 或 ASR API key。

实现对齐说明：上面的 `waiting_authorization`、`transcribing` 和 `partial` 是导入流程语义，不代表当前 `ImportItemStatus` 已经存在同名枚举。当前代码已有 `WaitingCapability`、`WaitingLogin`、`Extracting`、`Validating`、`PreviewReady` 和 `Failed`。正式实现时按以下方式处理：

| 流程语义 | 当前/建议持久化方式 |
|---|---|
| 等待能力包 | 使用现有 `ImportItemStatus::WaitingCapability`，`ImportIssue.recovery_actions` 包含 `InstallCapability` 或 `Retry` |
| 等待登录 | 使用现有 `ImportItemStatus::WaitingLogin`，绑定 `ConnectorSession` 后回到 `Extracting` |
| 等待本地 ASR 授权 | 若 UI 需要跨重启恢复，新增 `ImportItemStatus::WaitingAuthorization` 和 `ImportRecoveryAction::AuthorizeLocalAsr`；在此之前不能把“已安装 ASR”当成“用户已授权” |
| 正在转写/OCR | 保持 item 为 `Extracting`，通过 `ImportStage`/TaskProgress 记录 `transcribing` 或 `ocr` 子阶段；不必为每个模型新增 item 状态 |
| 部分成功 | 保持 `PreviewReady`，使用现有 `QualityReport.level = Warning`、`warnings` 和失败 asset 列表表达；只有提交批次的部分提交才使用 `PartiallyCommitted` |

因此，Phase 0 必须先补齐授权 action、阶段事件和恢复测试，再实现本地 ASR；不能先写一个与当前 `models/import_v2.rs` 不一致的平行状态协议。

## 5. 三个平台的路线设计

### 5.1 小红书图文/视频

#### 路线顺序

```text
小红书公开 URL
  1. URL 规范化（xiaohongshu.com / xhslink.com）
  2. provider 获取单条笔记元数据和媒体清单
  3. 下载图片/视频/封面到 staging
  4. 提取正文、话题、作者、发布时间
  5. 视频：平台字幕 → 内嵌字幕 → 用户授权本地 ASR
  6. 图片：保留全部图片；OCR 作为可选后处理
  7. 生成 preview Markdown 和质量报告
```

#### 图文帖要求

- `description` 保留原始换行、表情符号和话题标签；标题从正文中推断时必须标记 `titleSource: inferred`。
- 每张图片使用稳定 `assetId` 和原始顺序；不得只保留封面。
- 图片下载失败不影响其他图片和正文落盘，结果进入 `partial`，并列出失败 URL 的脱敏标识。
- 默认不 OCR。用户在预览中打开“提取图片文字”后，才路由到 `ocr.cjk-accurate`。
- 不能把图片内文字直接写进正文而不标记来源；应写入 `extracted/ocr.json`，Markdown 使用 `<details>` 或引用小节。

#### 视频帖要求

- 必须同时保存视频文件或明确记录“用户选择不下载原视频”。
- 视频没有字幕时显示“未发现平台字幕，可授权本地转写”，而不是伪造空转写。
- 视频转写的默认语言可由平台/用户设置推断，但要把 `language` 和 `modelSha256` 写入元数据。

#### 失败和登录

`captcha`、`login_required`、`structure_changed`、`removed` 必须是可区分的错误码。验证码不是无限重试条件；最多按退避策略重试一次，然后进入 `waiting_login` 或人工处理。

### 5.2 抖音图文/视频

#### 路线顺序

```text
抖音公开 URL / v.douyin.com 短链
  1. 解析短链并保存 canonical/public URL
  2. provider 获取 note/video 类型、desc、作者、发布时间和资源清单
  3. 图集：按 order 下载全部图片
  4. 视频：下载视频/封面，获取平台字幕或内嵌字幕
  5. 没有字幕时，用户授权后执行本地 ASR
  6. 生成 Markdown、metadata、transcript 和质量报告
```

#### 图文帖要求

- 把 `desc` 作为正文来源，把话题和标签分成 `hashtags`，不要把全量描述重复拼接到标题。
- 记录图集顺序、原图/下载图 URL（按隐私策略脱敏）和本地文件哈希。
- 没有 OCR 时仍保留图片；OCR 是增强能力，不是图片帖导入成功的前置条件。

#### 视频帖要求

- 先取平台字幕，再检查下载文件中的字幕轨，最后才询问 ASR。
- 对无语音、音乐或方言内容，允许 `transcription.sourceKind = none`，质量报告写明原因/推测，不能把低置信度文本伪装成平台字幕。
- 采用部分成功策略：视频或封面下载成功但 ASR 失败时，仍生成 `partial` Markdown。

### 5.3 Bilibili 视频

#### 路线顺序

```text
bilibili.com / b23.tv
  1. 规范化 URL 并保存 BV/AV 等 platformId
  2. 优先 native Bilibili metadata connector
  3. 获取平台人工字幕/自动字幕候选
  4. 选择字幕；不下载视频也可以先预览文字
  5. 用户选择“保存原视频”时再调用 yt-dlp media pack
  6. 没有可用字幕时，显示本地 ASR 授权；授权后下载受限音频并执行 ASR
  7. 提取封面、章节、作者、描述、发布时间、分 P 信息
```

#### B 站字幕优先级

1. 平台人工字幕 `platform_human`。
2. 平台自动字幕 `platform_automatic`。
3. 下载文件中的内嵌字幕 `embedded`。
4. 用户明确批准后的本地 ASR `local_asr`。
5. 没有结果时生成无转写预览。

这与当前 `MediaRouter` 的“人工平台/人工本地优先于自动，再优先于嵌入，最后才 ASR”的设计一致。B 站 connector 只负责提供字幕候选和 `MediaRoutePlan`，不应在 connector 内部启动 Whisper。

#### B 站登录和 ASR 授权

- 会员、地区限制或高质量下载需要登录时，启动 `ConnectorSessionService` 的隔离 profile。
- 登录 profile 必须绑定 `projectId + importSessionId + itemId + targetSha256`，只交给同一个导入项使用。
- 本地 ASR 授权也要绑定目标哈希、过期时间和 item，不允许“对当前项目的所有 B 站 URL 一次放行”。
- profile、Cookie 和私密请求 URL 只存在 OS credential storage 或临时 profile 目录，不写 `raw/`、Markdown、任务日志或 Git。

## 6. Capability Pack 设计

### 6.1 为什么采用外部 capability pack

当前项目已有签名 manifest、目标平台、许可证表达式、文件库存、运行时完整性校验、超时和取消机制。这正好适合把第三方平台适配、下载器、ASR 和 OCR 隔离在进程边界之外。

这样做可以获得：

- 平台站点变化时只替换 provider pack，不修改 ImportV2 核心。
- GPL/非商业许可项目不进入核心编译链接图。
- 每个 pack 可以按平台和模型体积独立发布、撤回、禁用和回滚。
- Rust 只处理稳定 DTO、任务、路径和安全策略；Python/Node/独立二进制负责平台脆弱逻辑。
- 运行失败、卡死、输出过大和路径逃逸都能由 PackEngine 统一处理。

### 6.2 推荐能力 ID 与路线

```text
media.capture.bilibili  -> web.bilibili.video
media.capture.xhs       -> web.xiaohongshu.note
media.capture.douyin    -> web.douyin.item
media.subtitle          -> media.subtitle
media.asr.sensevoice    -> media.asr
media.asr.whisper       -> media.asr
media.ocr.paddle        -> ocr.cjk-accurate
media.keyframes         -> media.keyframes
```

不要求第一版真的拆成七个进程。可以先交付一个 `media-platform` pack，再保持 capability ID 和协议的逻辑独立，后续再按体积、许可证或升级频率拆包。

### 6.3 JSON-RPC 协议

每个 pack 使用一行一个 JSON-RPC 2.0 请求/响应。stdout 只能输出协议消息；诊断日志写 stderr。当前主程序已经固定由 `PackProcessEngine` 发送 `method = "import.execute"`，`params` 的真实类型是 `EngineRequest`，响应 `result` 的真实类型是 `EngineResult`。新增 pack 必须实现这个协议，不另起 `media.inspect` / `transcribe` 方法。

`EngineOperation` 目前只有 `inspect` 和 `extract`；ASR、OCR、媒体下载是 `extract` 操作里的 route/capability 行为，不通过 JSON-RPC 新方法扩张。

```json
{
  "jsonrpc": "2.0",
  "id": "request-uuid",
  "method": "import.execute",
  "params": {
    "protocolVersion": "2",
    "requestId": "request-uuid",
    "projectId": "project-1",
    "sessionId": "session-1",
    "itemId": "item-1",
    "taskId": "task-1",
    "operation": "inspect",
    "input": {
      "kind": "url",
      "displayName": "Bilibili video",
      "locator": "https://www.bilibili.com/video/BV...",
      "normalizedLocator": "https://www.bilibili.com/video/BV...",
      "sourceIdentity": null
    },
    "projectRoot": "C:/project",
    "stagingRoot": ".app/import-sessions/session-1/items/item-1/staging",
    "chainedInput": null,
    "localAsrAuthorized": false,
    "localOcrAuthorized": false,
    "mediaSaveMode": "extract_only"
  }
}
```

`projectRoot` 和 `stagingRoot` 由 Core 生成；pack 不能自行替换它们。`stagingRoot` 可以是当前协议允许的项目相对路径，PackEngine 会在受控进程内解析并校验它位于项目根目录内。所有 `EngineResult` 输出文件路径必须是 staging-relative。

```json
{
  "jsonrpc": "2.0",
  "id": "request-uuid",
  "result": {
    "sourceSnapshotPath": "source/source.bin",
    "markdownPath": "document.md",
    "assetPaths": ["assets/cover.jpg"],
    "metadataPath": "metadata.json",
    "title": "Bilibili video",
    "textCoverage": 1.0,
    "meaningfulImageCoverage": 1.0,
    "continuation": null,
    "warnings": []
  }
}
```

规则：

- `sourceSnapshotPath`、`markdownPath`、`metadataPath`、`assetPaths` 必须符合当前 `EngineResult` 字段；不能让 Rust 扫描整个目录猜结果。
- 所有输出路径必须是 staging 相对路径；禁止绝对路径、`..` 和符号链接逃逸。
- `engineId`、`engineVersion` 和 `route` 由 Core 的 `EngineDescriptor` 记录，不是 pack 自由扩展的响应字段。
- ASR/OCR 的续接使用现有 `EngineContinuation::LocalAsr` / `LocalOcr`，不能让 pack 直接创建特权 continuation。
- 进程退出码、stderr 摘要、超时、取消、JSON 解析失败分别映射到稳定 `IMPORT_V2_*` 错误码。
- 任何第三方 pack 都必须通过签名 manifest、固定 target triple、许可证白名单、archive SHA-256 和运行时文件清单校验。

### 6.4 yt-dlp 的调用约束

B 站媒体 pack 只使用稳定的机器接口，不解析人类可读 stdout：

```text
yt-dlp
  --dump-single-json
  --no-warnings
  --no-progress
  --write-info-json
  --write-thumbnail
  --write-subs
  --write-auto-subs
  --sub-langs "all"
  --continue
  --no-overwrites
  --paths <staging>
  --output "media/%(id)s.%(ext)s"
  <approved-url>
```

实际参数由 provider 版本固定，不能把任意用户输入拼成 shell 字符串；使用 `Command` 参数数组。下载原视频与“仅获取元数据/字幕”必须是两个明确操作，避免用户只是预览却意外下载大文件。

`ffmpeg/ffprobe` 若作为 yt-dlp 依赖，必须作为单独 capability 文件登记版本、SHA-256 和许可证。不能因为 yt-dlp 可以调用系统 PATH，就绕过能力包完整性校验。

## 7. 暂存、目录和提交策略

### 7.1 目录结构

```text
.app/
  import-sessions/<sessionId>/
    session.json
    events.jsonl
    items/<itemId>/
      item.json
      staging/
        evidence/provider-request.json
        evidence/provider-result.json
        source/source.json
        media/
        images/
        subtitles/
        transcript/
        ocr/
        frames/
      logs/pack.stderr.log
  source-index-v2.json

raw/
  sources/<sourceId>/<versionId>/
    source.json
    original.url
    original.description.md
  assets/<sourceId>/<versionId>/
    cover.*
    media.*
    images/001.*
    frames/001.jpg
  extracted/<sourceId>/<versionId>/
    document.md
    metadata.json
    transcript.json
    transcript.vtt
    ocr.json
    quality.json

wiki/
  ... user-confirmed compile output ...
```

说明：

- `raw/sources` 保存来源身份、描述和原始引用；它默认不可变。
- `raw/assets` 保存视频、音频、封面、图片和关键帧；是否保存原视频由用户选项决定。
- `raw/extracted` 保存机器提取的 Markdown、字幕、转写、OCR 和质量报告。
- `.app/import-sessions` 是可恢复任务和暂存区，不是用户 Wiki 内容；任务成功后可按保留策略清理 staging。
- provider 的内部 SQLite 可以存在于 capability pack 私有临时目录，但不能成为项目 Wiki 内容的事实来源，也不能替换 `SourceRegistry`。

当前代码已经固定了下面的路径契约，后续媒体 provider 必须服从它：

| 用途 | 当前 canonical path | 说明 |
|---|---|---|
| 会话暂存根 | `.app/import-sessions/<sessionId>/items/<itemId>/staging/` | capability 输入和 `EngineResult` 相对输出的根；前端预览通过 session/item 查找 |
| 来源原始文件 | `raw/sources/<sourceId>/<versionId>/original.<ext>` | `SourceVersion.raw_path`，只保存原始来源/原始文件 |
| 媒体资产 | `raw/assets/<sourceId>/<versionId>/` | `SourceCommitPlan.asset_root_path`，保存封面、图片、音频、视频和关键帧 |
| 提取结果 | `raw/extracted/<sourceId>/<versionId>/` | `SourceVersion.extracted_path`，当前 Markdown 文件名为 `extracted.md`，并保存 `quality.json` 等证据 |
| 基线快照 | `.app/source-artifacts/<sourceId>/<versionId>/baseline.md` | 用于冲突检查和外部编辑保护，不是用户 Wiki 内容 |
| 来源 manifest | `.app/sources/<sourceId>.json` | `SourceManifest`；保存版本、route、engine、raw/extracted/baseline 路径 |
| 来源索引 | `.app/source-index-v2.json` | 按 locator/content hash 指向 `sourceId/versionId` |

文档中的 `transcript/segments.json`、`media/audio.wav` 等均表示相对于当前 item staging 根的逻辑路径；提交时不能直接把它们解释成项目根相对路径。`SourceRegistry` 生成的 commit plan 才是 staging 到项目持久化目录的唯一映射。

### 7.2 提交顺序

```text
创建 staging
  → 写入并 fsync 原始/证据文件
  → 计算 SHA-256 和质量报告
  → 生成 Preview DTO
  → 用户确认
  → 检查 SourceRegistry 冲突
  → 对受影响路径创建 Git checkpoint
  → 原子移动/复制到 raw/sources、raw/assets、raw/extracted
  → 更新 source-index-v2.json 和 manifest
  → 写入导入日志
  → 触发 UI 刷新；不自动生成 wiki 页面
```

如果用户只想预览，截止到 `PreviewReady` 都不应改变项目 `raw/`、`wiki/` 或 `source-index-v2.json`。如果确认后提交中途失败，必须保留 staging、错误码和可恢复动作，不能删除原始文件或留下半写的索引。

### 7.3 去重和版本

使用现有 `SourceRegistry`：

- `canonicalUrl + platformId` 用于识别来源定位。
- 媒体/描述/字幕/转写的规范化内容哈希用于识别内容版本。
- 相同内容的新 URL 归为 `SameContentNewOrigin`，不重复导入媒体。
- 相同 URL 内容变化归为 `UpdatedOrigin`，创建新 `versionId`，旧版本保留。
- provider/ASR/OCR 版本变化不应悄悄覆盖原始版本；它们应记录在 `SourceVersion` 的 route/engine/quality 字段中，必要时新建提取版本。

## 8. Markdown 输出契约

### 8.1 基础模板

```markdown
---
type: source
source_platform: bilibili
source_id: BVxxxxxxxxx
source_url: https://www.bilibili.com/video/BVxxxxxxxxx
title: 视频标题
author: 作者
published_at: 2026-07-20T08:00:00Z
captured_at: 2026-07-20T08:10:00Z
content_type: video
route: web.bilibili.video
engine_id: media.capture.bilibili
engine_version: 1.0.0
content_hash: sha256:...
transcript_source: platform_human
transcript_engine: bilibili-caption
quality_status: passed
---

# 视频标题

> 来源：[Bilibili](https://www.bilibili.com/video/BVxxxxxxxxx)
>
> 原始媒体：`../../assets/source-id/version-id/media.mp4`

## 来源信息

- 作者：作者
- 发布时间：2026-07-20 16:00（Asia/Shanghai）
- 导入时间：2026-07-20 16:10（Asia/Shanghai）
- 导入路线：`media.capture.bilibili@1.0.0`
- 内容版本：`sha256:...`

## 原始描述

原始描述、话题和平台章节，保持原文，不做摘要改写。

## 字幕 / 转写

### 00:00

今天讨论本地优先知识库。

### 00:02

……

> 来源：平台人工字幕。完整机器结构化结果见 `transcript.json` / `transcript.vtt`。

## 图片文字与视觉证据

仅在启用 OCR/关键帧后出现；每段都标记 `source: ocr` 或 `source: frame`，不把模型推断写成原始事实。

## 资源

- [封面](../../assets/source-id/version-id/cover.jpg)
- [原视频](../../assets/source-id/version-id/media.mp4)
- [完整字幕](../../extracted/source-id/version-id/transcript.vtt)

## 导入质量

- 元数据：通过
- 媒体：通过
- 字幕覆盖率：99%
- 警告：无
```

### 8.2 小红书/抖音图片帖模板差异

图片帖不生成“视频转写”小节；正文、话题、图片资产和 OCR 证据分开：

```markdown
## 原始正文

这里是平台原始描述……

## 话题

`#AI` `#知识库`

## 图片

1. ![第 1 张](../../assets/source/version/images/001.jpg)
2. ![第 2 张](../../assets/source/version/images/002.jpg)

## OCR 证据

> OCR 仅作为图片文字提取结果，不等同于作者正文。

- `001.jpg`：本地 OCR 置信度 0.94
```

如果编译阶段需要摘要、标签、反向链接或知识图谱关系，应放进 Wiki 页面或编译产物，不回写 `raw/extracted/document.md` 的原始证据段落。

## 9. 质量门禁

### 9.1 必须通过的通用检查

- URL 经过规范化，public URL 和 secure request URL 可校验对应。
- `platformId`、标题、canonical URL 至少有一项可靠来源；完全空结果不能进入预览成功。
- 所有输出路径在 staging 根目录内；没有 `..`、绝对路径或未允许的符号链接。
- 所有媒体、图片、字幕文件都计算 SHA-256 和字节大小。
- 图片/视频文件可被 `ffprobe` 或图像解码器读取；扩展名与 MIME 不一致时记录警告。
- 字幕按 `startMs/endMs` 排序，无负数、重叠、空文本和异常超长段落。
- 不允许 provider 响应中的 URL 直接覆盖最终 Markdown 链接；Markdown 只引用项目内相对路径，外部 URL 作为 frontmatter 的来源引用。
- 失败资源、回退路线和用户授权必须可见，并写入 `quality.json`。

### 9.2 平台特定门禁

| 平台 | 通过条件 | 允许的部分成功 |
|---|---|---|
| 小红书图文 | 正文/标题可读，至少一项图片或正文成功 | 个别图片失败、OCR 未安装 |
| 小红书视频 | 元数据成功，视频或字幕至少一项成功 | 视频下载成功但无字幕/ASR |
| 抖音图集 | `desc` 或至少一张图片成功 | 部分图片失败 |
| 抖音视频 | 元数据成功，视频文件成功或可获得字幕 | ASR 失败、封面失败 |
| B 站视频 | 标题和 BV/AV 识别成功 | 没有字幕、用户不下载原视频 |

### 9.3 错误码建议

```text
IMPORT_V2_PLATFORM_UNSUPPORTED
IMPORT_V2_URL_NORMALIZATION_FAILED
IMPORT_V2_PLATFORM_LOGIN_REQUIRED
IMPORT_V2_PLATFORM_CAPTCHA
IMPORT_V2_PLATFORM_REMOVED
IMPORT_V2_PLATFORM_STRUCTURE_CHANGED
IMPORT_V2_CAPABILITY_UNAVAILABLE
IMPORT_V2_CAPABILITY_INVALID
IMPORT_V2_MEDIA_DOWNLOAD_FAILED
IMPORT_V2_MEDIA_INTEGRITY_FAILED
IMPORT_V2_SUBTITLE_PARSE_FAILED
IMPORT_V2_ASR_NOT_AUTHORIZED
IMPORT_V2_ASR_FAILED
IMPORT_V2_OCR_UNAVAILABLE
IMPORT_V2_PARTIAL_RESULT
IMPORT_V2_SOURCE_CONFLICT
IMPORT_V2_STAGING_COMMIT_FAILED
```

错误必须返回现有 `BackendError` 结构：`code`、用户可读 `message`、安全 `details`、`recoverable`、`user_action_required`。详细 provider stderr 只进入受限任务日志，不直接回传给前端。

## 10. UI / 交互要求

导入页保持当前项目的 review workflow：

1. 顶部：URL 输入、文件选择、文件夹选择、粘贴 Markdown/文本。
2. 左侧/中部：导入项列表，显示平台、类型、大小、阶段、状态和警告。
3. 右侧预览：标题、作者、来源 URL、描述、图片/封面、字幕/转写、媒体文件和质量报告。
4. 能力提示：明确显示“未安装能力”“需要登录”“需要本地 ASR 授权”“OCR 未启用”，每个操作都说明影响。
5. 底部：取消、重试、仅保留证据、确认导入、确认并进入 Wiki 编译。
6. 任务抽屉：显示阶段进度、可复制的错误码、日志摘要、取消和后台运行。

关键确认文案应具体：

- “将下载约 420 MB 视频到 `raw/assets/`，是否继续？”
- “将使用本地 SenseVoice 模型在本机转写音频，音频不会上传到云端，是否授权？”
- “检测到同一 URL 内容发生变化，将创建新来源版本，不覆盖旧版本，是否继续？”
- “该平台要求登录；将打开隔离浏览器 profile，登录信息不会写入项目文件，是否打开？”

Wiki 编译不能因为用户点击“确认导入”就自动运行；必须有独立的编译确认，并显示将修改的 Wiki 路径、Git checkpoint 和 Markdown diff。

## 11. 安全、隐私、合规

### 11.1 凭证和浏览器登录

- 不从前端接收或持久化 Cookie 字符串。
- 浏览器登录使用隔离 profile，profile 目录权限最小化，任务完成后按策略销毁或由用户明确保留。
- `WebTargetStore` 的 secure URL 引用、ASR 授权和 profile 引用均使用 OS credential storage 或内存句柄。
- 日志中禁止出现 `Cookie`、`Authorization`、`SESSDATA`、完整签名 URL、API key 和浏览器 profile 内容。

### 11.2 网络和资源限制

- 只允许用户提交的 public URL 或经过明确授权的 secure URL；仍由 `UrlPolicy` 做私网、回环、DNS rebinding 和重定向检查。
- 单项设置最大响应体、媒体字节数、图片数量、视频时长、并发数和总超时。
- 每个域名使用 `DomainLimiter` 限流、退避和熔断；验证码/登录失败不做无界重试。
- capability 进程清空继承环境，只注入必要环境变量；不允许任意 shell、任意路径、任意子进程链。
- 下载完成后校验文件大小、MIME、媒体可读性和哈希；失败资源不冒充成功。

### 11.3 许可证和平台规则

发行前必须建立每个 pack 的 SBOM/NOTICE：组件、版本、源码地址、许可证表达式、模型许可、二进制 SHA-256、目标平台和打包方式。

特别注意：

- [yt-dlp 的 README](https://github.com/yt-dlp/yt-dlp#licensing) 明确说明仓库源码许可证与发行二进制中第三方组件的许可证可能不同。
- [MediaCrawler LICENSE](https://github.com/NanmiCoder/MediaCrawler/blob/main/LICENSE) 是面向学习/研究/非商业使用的许可，不能默认用于商业发行。
- [XHS-Downloader LICENSE](https://github.com/JoeanAmier/XHS-Downloader/blob/main/LICENSE) 为 GPL-3.0，若采用应保持进程隔离并由发行前法律审查确认。
- ASR/OCR 模型权利、训练数据限制和平台内容下载权利独立于代码许可证；应用应向用户提示“仅对有权使用的内容导入”。

本方案不试图绕过登录、验证码、付费墙、地区限制或平台访问控制。只支持用户主动提交、明确授权和适度频率的个人知识库导入。

## 12. 分阶段实施顺序

### Phase 0：契约和测试夹具

交付：

- `MediaSource`、`SubtitleCandidate`、`Transcription`、`QualityReport`、provider result JSON Schema。
- 统一错误码、状态机事件和 staging 目录策略。
- 不联网的固定 fixture：小红书图文 JSON、抖音图集 JSON、B 站 metadata/subtitle JSON、损坏响应、短链重定向结果。
- 许可证清单和 capability manifest 样例。

退出条件：所有 fixture 可以走到 `PreviewReady` 或可预测的失败状态。

### Phase 1：B 站最小闭环

先实现 B 站，因为当前项目已有 B 站 connector 和 MediaRouter：

1. native connector 获取标题、作者、描述、章节、字幕候选。
2. `media.capture.bilibili` 使用 yt-dlp 获取 metadata/封面/可选视频。
3. 平台字幕 → 内嵌字幕 → `waiting_authorization` → 本地 ASR。
4. 写 `raw/sources`、`raw/assets`、`raw/extracted` 和 SourceRegistry。
5. 完成重复 URL、内容变化、取消、无字幕和私有资源测试。

退出条件：普通公开 BV URL 无需登录即可生成可靠 Markdown；无字幕时能明确降级；用户授权后能续接本地 ASR。

### Phase 2：小红书和抖音 provider

1. 扩展 `DomainRouter` 的 canonical URL 识别和短链解析。
2. provider 统一返回 `MediaSource`，不返回平台专属 Markdown。
3. 图文/图集优先打通，确保图片顺序、原始描述、话题和部分成功。
4. 视频路线复用 `MediaRouter`，不重复实现字幕/ASR优先级。
5. 登录/验证码走 `ConnectorSessionService`，登录 profile 与 item 绑定。

退出条件：三个平台的相同 Markdown 预览组件可以渲染，平台差异只出现在 metadata 和能力提示中。

### Phase 3：本地 ASR

推荐顺序：

1. 先接 `faster-whisper`，验证跨平台能力包、音频抽取、VTT/JSON 输出。
2. 中文体验稳定后增加 `SenseVoice/FunASR` pack。
3. 需要字级时间轴/说话人时再加入 WhisperX，不把其重量级依赖放入基础安装。

ASR 任务必须支持模型下载进度、模型 SHA-256、可取消、断点和设备信息；模型和音频都属于本地敏感资产，失败后按用户策略保留或清理。

### Phase 4：OCR、关键帧和视觉证据

1. 图片帖 OCR：PaddleOCR CJK pack。
2. 视频关键帧：ffmpeg scene detection/定时采样。
3. 对关键帧做 OCR；视觉理解交给后续 Agent/Skill。
4. 所有 OCR/关键帧结果带 `sourceAssetId`、时间戳、模型版本和置信度。

退出条件：关闭 OCR/关键帧时，基础媒体导入路径的行为、耗时和输出完全不受影响。

### Phase 5：编译集成

在 `PreviewReady` 后增加独立“编译为 Wiki”操作：

- 读取原始描述、字幕/转写、OCR、媒体元数据和当前项目 `purpose.md/schema.md`。
- 生成摘要、主题、标签、链接、章节和图谱候选。
- 先 Git checkpoint，再展示 Markdown diff，再由用户确认写入 `wiki/`。
- 生成内容必须引用 `sourceId/versionId/assetId`，保持从 Wiki 回溯到原始证据的能力。

## 13. 验收清单

### 功能

- [ ] 小红书图文：正文、话题、作者、发布时间和全部图片可导入。
- [ ] 小红书视频：视频、封面、正文和字幕/ASR 状态可导入。
- [ ] 抖音图集：顺序、正文、话题和部分图片失败可见。
- [ ] 抖音视频：平台字幕优先，无字幕可授权本地 ASR。
- [ ] B 站视频：BV/AV、标题、作者、章节、封面、平台字幕和可选原视频可导入。
- [ ] 三个平台都能生成统一 Markdown，不需要在前端写平台专属解析。
- [ ] 只预览不污染项目目录；确认后才提交 raw/extracted。
- [ ] 取消、失败、重启应用后可恢复或安全清理 staging。

### 数据和安全

- [ ] CJK、Emoji、空格、大小写冲突和 Unicode 规范化路径测试通过。
- [ ] 重复 URL、相同内容不同 URL、内容更新和外部 Markdown 编辑冲突测试通过。
- [ ] URL 重定向、私网地址、DNS rebinding、超大媒体、损坏媒体测试通过。
- [ ] Cookie、token、secure URL、ASR API key 不进入项目文件和日志。
- [ ] capability pack 签名、目标平台、许可证、archive 和 runtime 文件哈希验证失败时 fail-closed。
- [ ] 进程 stdout 污染、stderr 超限、超时、僵尸进程、路径逃逸和非零退出码测试通过。

### 质量

- [ ] 字幕排序、时间戳、覆盖率、重叠、空段检测可解释。
- [ ] 每个媒体/图片/字幕都有大小、MIME、哈希和原始来源证据。
- [ ] 平台原文和模型生成摘要在文件结构上严格分开。
- [ ] `raw/sources` 默认不可变，替换和版本变化需要确认并保留旧版本。
- [ ] Markdown 可被现有渲染器正常显示，资源引用全部是项目相对路径。

## 14. 最终落地建议

第一版只交付以下闭环：

```text
Bilibili + yt-dlp + 平台字幕
  → 没字幕时用户授权 faster-whisper/SenseVoice
  → 统一 Preview Markdown
  → 用户确认后写入 raw/sources、raw/assets、raw/extracted
  → SourceRegistry 去重和版本
```

第二步补齐小红书和抖音的图文/图集，再补视频。第三步再加 OCR、关键帧、说话人和 AI 编译。这样能最大化复用当前项目已有的 `ImportV2Service`、`PackEngine`、`ConnectorSessionService`、`MediaRouter`、`OcrRouter` 和 `SourceRegistry`，并把最易变化的平台逻辑隔离在可签名、可回滚的开源能力包中。

一句话决策：

> 用 yt-dlp 解决成熟的通用媒体下载，用平台 provider 解决小红书/抖音/B 站元数据和字幕差异，用 FunASR/SenseVoice/faster-whisper 解决本地转写，用 PaddleOCR 解决可选图片文字提取；所有组件通过当前项目的 Import V2 能力包和 staging/preview/confirm 提交流程接入，原始来源永远优先于模型生成内容。

## 15. 代码级开源实现参考与当前项目映射

本节把三个指定项目的实现落实到源码文件、函数职责和当前 Import V2 模块。代码链接指向 2026-07-22 的主分支/默认分支；正式实现时必须记录实际采用的 commit SHA，不能让生产构建隐式跟随主分支。

这里参考的是可验证的实现行为和数据契约，不把第三方项目的完整应用代码复制进本仓库。所有平台连接器都应重新实现为当前项目的 Rust provider，第三方下载器、ASR、OCR 和浏览器自动化则通过 capability pack 隔离。

本节中出现的代码块分为两类：标记为“当前代码形状”的片段必须以现有 Rust 类型为准；标记为“伪代码”的片段只表达上游算法顺序，不能直接复制编译。所有 capability 输入/输出示例都必须遵守第 6.3 节的现有 `EngineRequest` / `EngineResult` 协议。

### 15.0 当前 Import V2 的真实边界

当前仓库并不是“connector 负责所有网络请求”的结构：

| 层 | 当前实际职责 |
|---|---|
| `services/import_v2/bilibili.rs` | B 站公开 metadata/player API、CID、WBI playurl 和媒体 URL 的 deterministic engine 路线 |
| `services/import_v2/generic_web_engine.rs` | 通用 HTTP、平台 HTML/meta、图片/媒体/字幕下载、平台资源校验和本地 staging |
| `services/import_v2/connectors/bilibili.rs` | 已规范化平台 JSON 的字段解析/校验和 B 站 connector 结果转换 |
| `services/import_v2/connectors/xiaohongshu.rs` | 已规范化 XHS JSON 的字段解析/校验和图片/视频候选转换 |
| `services/import_v2/platform_provider.rs` | 多平台嵌入 JSON 的通用递归提取、平台 ID 匹配和 URL 收集 |
| `services/import_v2/pack_engine.rs` | 外部 capability 进程、JSON-RPC、超时、取消、环境清理、staging 路径和结果校验 |

因此，后续实现应先判断路线：能由 deterministic/native engine 完成的 metadata 和公开字幕继续留在 Rust；需要平台脆弱逻辑、浏览器登录、下载器、FFmpeg、ASR 或 OCR 的部分才进入 capability pack。不要为了“复刻上游项目”把所有逻辑移动到新 `connectors/*.rs` 文件中。

### 15.1 red-blue-cp：采集、转写、视觉识别和 Markdown 的源码参考

| 上游源码 | 实际实现行为 | 在本项目中的对应设计 |
|---|---|---|
| [`app/extract/fetcher.py`](https://github.com/MuChengZJU/red-blue-cp/blob/main/app/extract/fetcher.py) | B 站解析 BV/短链，调用视频信息接口得到 CID，再调用播放器接口获取字幕和媒体地址；小红书请求页面并从 `window.__INITIAL_STATE__` 中寻找笔记对象，提取 `noteId`、作者、正文、`imageList` 和视频地址 | `connectors/bilibili.rs`、`connectors/xiaohongshu.rs` 只实现 metadata/候选资源提取；下载交给 `WebFetch` 或 capability pack |
| [`app/extract/extractor.py`](https://github.com/MuChengZJU/red-blue-cp/blob/main/app/extract/extractor.py) | 先按平台判断内容类型；B 站有字幕就使用字幕，否则调用 ASR；小红书图文逐张图片调用视觉模型，视频调用 ASR | `PlatformDocument` → `MediaRoutePlan`；`MediaRouter` 统一字幕优先级，不在平台 connector 中直接调用模型 |
| [`app/extract/model.py`](https://github.com/MuChengZJU/red-blue-cp/blob/main/app/extract/model.py) | ASR 先上传媒体、提交异步转写任务、轮询结果，再整理成文本和时间片段；VLM 使用图片输入提取可见文字并描述画面；LLM 清洗保持原意、不添加新信息 | ASR/VLM 拆成 `media.asr.*`、`ocr.*` capability；模型输出只落 `raw/extracted`，不直接覆盖来源正文 |
| [`app/extract/markdown.py`](https://github.com/MuChengZJU/red-blue-cp/blob/main/app/extract/markdown.py) | 按平台、日期、作者、标题和 ID 生成安全文件名，写 frontmatter 和正文，使用临时文件后原子替换 | 复用 `Staging` + `SourceRegistry` + `commit_items_cancellable`；Markdown 生成器只接收结构化证据和本地 asset path |
| [`app/extract/pipeline.py`](https://github.com/MuChengZJU/red-blue-cp/blob/main/app/extract/pipeline.py) | 编排 provider、媒体保存、提取、Markdown 写入、哈希和用量事件 | 对应 `orchestrator.rs`，但需要保留当前项目的取消、恢复、质量门禁、用户确认和 Git checkpoint |

#### 15.1.1 B 站源码行为的 Rust 化实现

不复制 Python 函数，而是复刻其数据流：

```text
normalize_url()
  → extract BV/AV
  → GET web-interface/view
  → read cid/pages/owner/title/desc
  → GET player subtitle endpoint
  → collect subtitle candidates
  → GET playurl only when media download is requested
```

关键约束：

1. 只获取预览所需的 metadata 和字幕时，不应默认下载完整视频。
2. 字幕 URL 只是候选，不代表内容可用；必须下载、解析、校验时间轴后才能标记 `PreviewReady`。
3. 平台字幕 JSON、VTT、SRT、ASS 最终都转换为同一个 `TranscriptSegment` 数组。
4. 媒体播放 URL 可能包含短期签名参数，只能存在于当前任务内存或 staging 请求记录，不能写进 Markdown。

当前代码形状（使用现有 `Platform` 和 `PlatformDocument`，网络获取函数名为实现阶段待补的 provider 函数）：

```rust
fn parse_bilibili_document(
    source_body: &str,
    public_url: &str,
) -> Result<PlatformDocument, BackendError> {
    extract_platform_document(Platform::Bilibili, source_body, public_url)
        .ok_or_else(|| BackendError::new(
            "IMPORT_V2_PLATFORM_STRUCTURE_CHANGED",
            "Bilibili metadata could not be normalized.",
            true,
            true,
        ))
}
```

上面是可以直接对应当前 `platform_provider.rs` 返回类型的解析片段；真实网络请求仍由 `bilibili.rs::fetch` 或 capability engine 负责。它只负责检查和返回候选，不直接调用 `yt-dlp` 或 ASR。

当前 `MediaRouter` 的调用形状是：

```rust
let input = MediaInput {
    kind: MediaKind::Video,
    subtitles: subtitle_candidates,
    cover_path: cover_path.map(str::to_owned),
};
let plan = MediaRouter::default().plan(&input, asr_available);
```

用户授权、能力可用性和 `local_asr_authorized` 由 `EngineRequest`/ImportV2 编排层决定，不作为不存在的 `MediaRouter::plan` 参数传入。

#### 15.1.2 小红书源码行为的 Rust 化实现

小红书单条笔记的参考算法是页面状态提取，而不是把整页 HTML 当正文：

```text
GET note page with controlled headers/session
  → detect removed/login/captcha markers
  → locate __INITIAL_STATE__ / SSR JSON
  → locate object matching requested noteId
  → read noteCard/noteDetail
  → classify image_note or video
  → collect ordered image URLs / master video URL
```

当前 [platform_provider.rs](../src-tauri/src/services/import_v2/platform_provider.rs) 已有脚本 JSON 的平衡括号提取和平台 ID 匹配逻辑，可作为通用 fallback；小红书专属字段匹配放在 [connectors/xiaohongshu.rs](../src-tauri/src/services/import_v2/connectors/xiaohongshu.rs)，页面获取和媒体落盘继续由 `generic_web_engine.rs` / `web_fetch.rs` 负责。

图文和视频的后处理必须分开：

```rust
// 伪代码：ContentType/ImagePost 是设计层分类，正式代码映射到
// PlatformDocument.content_type == "image_post" / "video"。
match document.content_type.as_str() {
    "image_post" => {
        let images = capture_ordered_images(&document.images, &staging)?;
        // 基础导入只保存图片；OCR/VLM 是用户开启后的后处理。
        write_image_manifest(images)?;
    }
    "video" => {
        let input = MediaInput {
            kind: MediaKind::Video,
            subtitles: document
                .subtitles
                .iter()
                .map(to_subtitle_candidate)
                .collect(),
            cover_path: document.cover_url.clone(),
        };
        let subtitle_plan = media_router.plan(&input, asr_available);
        execute_subtitle_or_asr(subtitle_plan, &staging)?;
    }
    _ => {}
}
```

VLM 的提示词应明确区分“看见的文字”和“模型描述”：

```text
请只提取图片中实际可见的文字，并单独给出画面描述。
不要根据常识补充图片中没有出现的信息。
输出 text、description、confidence，不要生成摘要。
```

模型结果保存为 `extracted/ocr/<assetId>.json` 或 `extracted/visual/<assetId>.json`，Markdown 只引用这些证据，不把它们冒充为小红书原始正文。

### 15.2 BiliNote：视频转写、截图锚点和长文本分块的源码参考

| 上游源码 | 实际实现行为 | 在本项目中的对应设计 |
|---|---|---|
| [`backend/app/downloaders/bilibili_downloader.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/downloaders/bilibili_downloader.py) | 使用 `yt-dlp` 下载音频/视频，必要时写临时 Netscape cookie 文件；视频使用 FFmpeg 合并/抽音频 | 由 `media.capture.bilibili` capability 处理；主进程不创建任意 cookie 文件，不把 cookie 传到前端 |
| [`backend/app/downloaders/bilibili_subtitle.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/downloaders/bilibili_subtitle.py) | 先调用 B 站播放器字幕接口，选出可用字幕；取不到时才回退到 `yt-dlp` 的字幕发现/下载 | 复用当前 `MediaRouter` 的字幕优先级；provider 只返回候选，`subtitle.rs` 决定是否可解析 |
| [`backend/app/services/note.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/services/note.py) | `NoteGenerator` 先复用缓存转写，没有转写才下载和 ASR；GPT 生成后扫描截图占位符，并从本地视频生成截图 | Import V2 只保存时间片段和可选 `ScreenshotMarker`；截图生成放在用户确认后的可选后处理，不阻塞原始导入 |
| [`backend/app/gpt/universal_gpt.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/gpt/universal_gpt.py) | 把带时间的片段组织成模型输入，支持文本/图片混合输入、重试、checkpoint 和分段结果合并 | 参考其 checkpoint 和 chunking；实现放在编译/Agent 层，不放入基础平台 connector |
| [`backend/app/gpt/request_chunker.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/gpt/request_chunker.py) | 按请求大小和片段边界切分长转写，避免一次请求超限 | 为后续 BYOK/Agent 编译提供 `TranscriptChunk`，每块保存起止时间和原始 segment ID |
| [`backend/app/downloaders/douyin_downloader.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/downloaders/douyin_downloader.py) | 通过抖音 Web 详情接口、`msToken`、`a_bogus` 等参数请求作品详情，再读取视频/音乐地址 | 只参考返回 DTO 和失败分类；不把签名计算复制进主进程，优先走浏览器 session 或独立 provider pack |
| [`backend/app/transcriber/transcriber_provider.py`](https://github.com/JefferyHcool/BiliNote/blob/master/backend/app/transcriber/transcriber_provider.py) | 用 provider registry 选择 faster-whisper、MLX Whisper、远程转写等实现 | 复用“转写器注册表”思想，但能力由签名 pack manifest 注册，不把模型库编译进 Tauri |

#### 15.2.1 转写缓存和回退

BiliNote 的重要行为不是“每次都重新下载”，而是先检查已有转写。当前项目应把缓存键扩大为来源版本和引擎版本：

```rust
pub struct TranscriptCacheKey {
    pub source_version_id: String,
    pub media_sha256: String,
    pub engine_id: String,
    pub engine_version: String,
    pub language: Option<String>,
}
```

读取顺序（左侧路径均相对于当前 item 的 staging 根目录 `.app/import-sessions/<sessionId>/items/<itemId>/staging/`）：

```text
staging/transcript/segments.json
  → raw/extracted/<source>/<version>/transcript.json
  → platform subtitle candidate
  → embedded subtitle
  → authorized local ASR
```

任何 ASR 失败都要保留已经下载的媒体和 ASR checkpoint，下一次重试从失败阶段继续，而不是重新执行页面解析。

#### 15.2.2 时间锚点和截图

`BiliNote` 的 `Content-[mm:ss]`、`Screenshot-[mm:ss]` 是适合编译层的输出契约，但不适合直接作为原始证据的唯一结构。建议同时保存机器结构化 JSON：

```json
{
  "markers": [
    {
      "kind": "screenshot",
      "timeMs": 310000,
      "sourceSegmentIds": [42, 43],
      "status": "pending"
    }
  ]
}
```

确认导入后，如果用户开启截图：

```text
marker.timeMs
  → capability media.keyframes
  → frames/<frameId>.jpg
  → replace marker with project-relative asset link
```

截图失败时保留时间标记并写入 `quality.json`，不能让 LLM 删除原始时间锚点。

### 15.3 dy-note：资产优先和抖音视觉补证的源码参考

| 上游源码 | 实际实现行为 | 在本项目中的对应设计 |
|---|---|---|
| [`SKILL.md`](https://github.com/Rimagination/dy-note/blob/main/SKILL.md) | 按 quick/evidence/research 等强度组织流程，要求字幕/ASR 为事实主干，视觉结果带证据等级 | Import V2 使用 `requestedFeatures` 和 `QualityReport` 表达成本/证据等级；默认走 evidence-light，不自动执行视觉 AI |
| [`scripts/extract_douyin_text.py`](https://github.com/Rimagination/dy-note/blob/main/scripts/extract_douyin_text.py) | 优先复用已有 `transcript.txt`、`segments.json`、`metadata.json`；需要转写时用 FFmpeg 生成单声道 16k 音频 | `SourceRegistry` + staging checkpoint；媒体转换作为 `media.audio.prepare` capability |
| [`scripts/run_qwen_asr.py`](https://github.com/Rimagination/dy-note/blob/main/scripts/run_qwen_asr.py) | 调用 Qwen3-ASR，输出模型信息、文本和带起止时间的 segments JSON | 统一转换到 `TranscriptArtifact`，记录 model ID、版本、设备、dtype 和模型哈希 |
| [`scripts/douyin_web_ai_brief.py`](https://github.com/Rimagination/dy-note/blob/main/scripts/douyin_web_ai_brief.py) | 通过登录 Chrome/CDP 触发抖音“问 AI/识别画面”，读取章节和时间线；不可用时标记弱证据或走备用模型 | 作为 `media.visual.douyin-web` 可选能力；不把页面 AI 结果当字幕，不在基础导入同步执行 |
| [`scripts/archive_dy_note_assets.py`](https://github.com/Rimagination/dy-note/blob/main/scripts/archive_dy_note_assets.py) | 将字幕、转写、评论、元数据和 AI brief 归档为资产，并生成 manifest | 对应 `raw/assets`、`raw/extracted` 和 `SourceRegistry`；最终 Markdown 是资产的派生视图 |

#### 15.3.1 抖音低转写密度策略

不要用“转写文本长度小”直接推断视频内容很少；应将其作为视觉增强提示：

```rust
fn choose_douyin_enhancement(quality: &TranscriptQuality) -> EnhancementRoute {
    // 0.35 / 3 只是本项目的初始 fixture 阈值，必须通过样本集校准。
    if quality.coverage < 0.35 || quality.segment_count < 3 {
        EnhancementRoute::OfferKeyframesOrWebAi {
            reason: "transcript_density_low",
            requires_user_action: true,
        }
    } else {
        EnhancementRoute::None
    }
}
```

视觉结果需要单独的证据等级：

```rust
pub enum VisualEvidenceLevel {
    PlatformCaption,
    PlatformVisualBrief,
    LocalKeyframeOcr,
    ModelInference,
}
```

`ModelInference` 只能在 Markdown 中标记为推测/草稿，不能写入原始正文；`PlatformVisualBrief` 也只能补充画面和时间线，不能伪装成完整字幕。

#### 15.3.2 抖音 provider 的可实施契约

当前 `DomainRouter` 已能识别 `douyin.com` / `iesdouyin.com` 并生成 `WebRouteKind::Douyin`，但还没有真正的 `connectors/douyin.rs`。新增 provider 不能只写“使用浏览器登录”，至少要固定下面的请求/结果契约。

**provider 输入（由 Core 的 `EngineRequest` 组装，不新增 RPC 方法）：**

```json
{
  "route": "web.douyin.metadata",
  "publicUrl": "https://www.douyin.com/video/123",
  "normalizedUrl": "https://www.douyin.com/video/123",
  "profileRef": null,
  "requestedFeatures": {
    "metadata": true,
    "images": true,
    "media": false,
    "subtitles": true,
    "comments": false
  },
  "limits": {
    "maxMediaBytes": 1073741824,
    "maxImageCount": 100,
    "timeoutMs": 180000
  }
}
```

这里的 `profileRef` 只是逻辑字段；实际登录 profile 由现有 `WebTargetStore`/`ConnectorSessionService` 绑定，并由 `PackProcessEngine` 以受控环境变量注入，不得把 Cookie JSON 放进 provider JSON。

**provider 结果（写入 `EngineResult.metadata_path` 指向的 JSON）：**

```json
{
  "platform": "douyin",
  "canonicalUrl": "https://www.douyin.com/video/123",
  "platformId": "123",
  "contentType": "video",
  "title": "视频标题",
  "author": {"id": "user-1", "name": "作者"},
  "description": "原始描述",
  "publishedAt": null,
  "images": [],
  "media": [{"assetId": "video-1", "kind": "video", "relativePath": "media/video.mp4"}],
  "subtitles": [],
  "authRequired": false,
  "partial": false,
  "warnings": []
}
```

**路线优先级：**

```text
短链规范化
  → web.douyin.metadata / public HTML provider
  → 成功：返回 metadata、图片、字幕候选和媒体候选
  → challenge/login_required：停止当前路线，不做无界重试
  → ConnectorSessionService 打开隔离 profile
  → 同一个 item 续接 web.generic.browser 或 web.douyin.browser
  → 仍没有字幕：用户授权 local_asr
  → transcript density 低：预览中提供 keyframe / web AI 选项
```

登录生命周期必须是可恢复的：

1. Provider 返回 `IMPORT_V2_PLATFORM_LOGIN_REQUIRED` 或 challenge 错误，并带 `user_action_required = true`。
2. Core 创建/打开与 `projectId + sessionId + itemId` 绑定的隔离 profile。
3. 用户完成登录后，profile 只被一次续接任务消费；成功或取消后释放/销毁按用户策略处理。
4. Provider 结果中只保存 `authRequired`、路线和质量，不保存 Cookie、Token 或签名 CDN URL。

必须准备的离线 fixture：

- 短链跳转到 `/video/<awemeId>`。
- 公开 HTML 能得到标题和封面但没有媒体地址。
- 已登录页面包含 `aweme_detail` 和图集 URL。
- challenge/login 页面。
- 10 张图片中 1 张 CDN 失败。
- 视频下载成功但字幕和 ASR 均不可用。

这让抖音路线成为可测试的 provider，而不是依赖某一次手动浏览器操作的产品假设。

### 15.4 当前项目的代码落点

| 实现职责 | 首选修改/复用文件 | 具体动作 |
|---|---|---|
| 域名与短链识别 | `services/import_v2/domain_router.rs`、`url_policy.rs` | 增加/校验 B 站、XHS、抖音 canonical URL；短链解析结果保留 public URL 和一次性 request target |
| 平台页面检查 | `services/import_v2/bilibili.rs`、`generic_web_engine.rs`、`connectors/*.rs`、`platform_provider.rs` | deterministic engine 负责真实网络/API；connector 负责规范化结果；通用 JSON 只做 fallback，不负责平台签名 |
| 字幕候选 | `services/import_v2/subtitle.rs`、`media_router.rs` | 统一 JSON/VTT/SRT/ASS 解析，输出 `TranscriptSegment`；候选失败后继续下一路线 |
| 媒体下载 | `services/import_v2/web_fetch.rs`、`pack_engine.rs` | 小文件/公开资源可由 Rust 流式下载；受平台适配和媒体合并的任务使用 capability pack |
| 浏览器登录 | `services/import_v2/connector_session.rs`、`web_target_store.rs` | 隔离 profile、用户显式登录、item 绑定；不读系统主浏览器 profile |
| ASR/OCR/关键帧 | `media_router.rs`、`ocr_router.rs`、`capability_runtime.rs` | 只调度能力包并接收 JSON 结果；模型版本和哈希进入质量报告 |
| 资产和版本 | `services/import_service/artifacts.rs`、`source_registry.rs` | 写 staging manifest、SHA-256、assetId、versionId；用户确认后按现有 commit plan 提交到 raw/extracted，并更新 `.app/sources/<sourceId>.json` |
| 质量和错误 | `quality_gate.rs`、`errors/error_codes.rs` | 增加媒体完整性、字幕覆盖率、时间轴和部分成功规则 |
| 任务和恢复 | `orchestrator.rs`、`tasks/task_events.rs` | 每个阶段可取消、可重试、可恢复；ASR 失败不重跑已成功的 metadata/media 阶段 |
| 前端 | `src/features/import/*` | 只显示 route/capability/auth/quality 状态；React 不直接处理网络、Cookie、文件和模型 |

### 15.5 建议的统一 DTO（本项目拟议，不是上游现成契约）

下面的 DTO、`VisualEvidenceLevel`、VLM 的 `text/description/confidence` 输出和抖音的低密度阈值都是本项目拟议设计，不是三个上游项目共同定义的协议。正式实现时应合并到现有 `models/import_v2.rs`，不能再建立一套平行字符串协议：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptArtifact {
    pub source_kind: TranscriptSourceKind,
    pub language: Option<String>,
    pub engine_id: String,
    pub engine_version: String,
    pub model_sha256: Option<String>,
    pub segments: Vec<TranscriptSegment>,
    pub quality: TranscriptQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualEvidenceArtifact {
    pub asset_id: String,
    pub level: VisualEvidenceLevel,
    pub text: Option<String>,
    pub description: Option<String>,
    pub confidence: Option<f32>,
    pub engine_id: String,
    pub engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub metadata_ok: bool,
    pub media_coverage: Option<f32>,
    pub transcript_coverage: Option<f32>,
    pub warnings: Vec<String>,
    pub failed_assets: Vec<String>,
    pub status: QualityStatus,
}
```

`PlatformDocument` 只描述平台返回的来源；`TranscriptArtifact` 描述字幕/转写；`VisualEvidenceArtifact` 描述 OCR、关键帧或平台视觉摘要；`QualityReport` 描述结果是否可预览、是否部分成功。四者不要合并成一个“LLM 输出字符串”。

### 15.6 Capability Pack JSON-RPC 调用示意

能力包不定义新的 `transcribe` JSON-RPC 方法。按照当前 `PackProcessEngine`，ASR/OCR/媒体操作都通过 `import.execute` 调用，具体 route 由注册的 `EngineDescriptor` 和 `EngineRequest.operation` 决定。能力包只接受 staging 内可校验的输入和相对输出，不接受任意本地路径：

```json
{
  "jsonrpc": "2.0",
  "id": "task-123:asr:1",
  "method": "import.execute",
  "params": {
    "protocolVersion": "2",
    "requestId": "task-123:asr:1",
    "projectId": "project-1",
    "sessionId": "session-1",
    "itemId": "item-1",
    "taskId": "task-123",
    "operation": "extract",
    "input": {
      "kind": "url",
      "displayName": "Douyin video",
      "locator": "https://www.douyin.com/video/123",
      "normalizedLocator": "https://www.douyin.com/video/123",
      "sourceIdentity": null
    },
    "projectRoot": "C:/project",
    "stagingRoot": ".app/import-sessions/session-1/items/item-1/staging",
    "chainedInput": "media/audio.wav",
    "localAsrAuthorized": true,
    "localOcrAuthorized": false,
    "mediaSaveMode": "extract_only"
  }
}
```

返回值必须是结构化结果：

```json
{
  "jsonrpc": "2.0",
  "id": "task-123:asr:1",
  "result": {
    "sourceSnapshotPath": "source/source.bin",
    "markdownPath": "transcript/transcript.md",
    "assetPaths": ["transcript/segments.json"],
    "metadataPath": "transcript/metadata.json",
    "title": "Douyin video transcript",
    "textCoverage": 0.96,
    "meaningfulImageCoverage": null,
    "continuation": null,
    "warnings": []
  }
}
```

`metadata.json` 中再记录 `sourceKind`、`engineId`、`engineVersion`、`modelSha256`、segment count 和 duration。这样不会扩张当前 `EngineResult`，同时保留 ASR 专属审计信息。stdout 只能输出 JSON-RPC。进度写任务事件，诊断信息写受限 stderr 日志；stdout 污染、超时、非零退出、输出路径越界和结果文件哈希不一致都必须失败关闭。

### 15.7 实施顺序和代码审查点

#### Phase A：先实现稳定的 B 站字幕路径

修改：

```text
connectors/bilibili.rs
media_router.rs
subtitle.rs
platform_provider.rs
quality_gate.rs
```

验证：

- BV/AV/短链规范化。
- 官方字幕 JSON、VTT、SRT、空字幕、登录错误和损坏 JSON fixture。
- 有字幕时不启动下载器和 ASR。
- 无字幕时进入 `waiting_authorization`，授权后只执行媒体/ASR阶段。
- 取消 ASR 后保留音频和 checkpoint。

#### Phase B：小红书图文和视频

修改：

```text
connectors/xiaohongshu.rs
platform_provider.rs
web_fetch.rs
artifacts.rs
```

验证：

- `window.__INITIAL_STATE__`、SSR JSON、缺字段和结构变化。
- 图集顺序、重复图片、封面和视频地址区分。
- 10 张图片中失败 1 张时进入 `partial`，其他 9 张仍可预览。
- 图片 OCR 关闭时不加载 OCR pack。

#### Phase C：抖音浏览器/能力包路径

新增：

```text
connectors/douyin.rs
capability packs/media.capture.douyin
capability packs/media.asr.*
```

验证：

- 短链、公开 URL、登录必需、移除内容和页面结构变化。
- 不记录 Cookie、Token、签名 URL。
- 先复用已有 transcript/metadata，再补缺失阶段。
- 低转写密度只产生视觉增强提示，不自动把视觉推测写成事实。

#### Phase D：截图/OCR/LLM 编译

这一步不属于基础导入。只在 `PreviewReady` 和用户确认后执行：

```text
TranscriptArtifact / VisualEvidenceArtifact / SourceMetadata
  → BYOK/Agent chunking
  → candidate Markdown
  → source anchors / screenshot markers
  → Markdown diff
  → Git checkpoint
  → user confirmation
  → wiki/
```

### 15.8 不应直接复制的代码

以下代码只作为行为参考，不能直接放入主进程：

1. BiliNote 的 FastAPI、SQLite、前端和应用级任务模型；当前项目已经有 Tauri IPC、TaskService 和 Markdown/JSON 持久化。
2. BiliNote 抖音下载器中的 `msToken`、`a_bogus` 和 Web 签名计算；它们应作为可替换 provider，并在每次升级时重新验证。
3. red-blue-cp 的 FastAPI/Jinja/Typer/SQLite 外壳；只复刻 `fetcher → extractor → model → renderer` 的数据分层。
4. dy-note 的 Agent Skill 调度代码；只吸收资产目录、证据等级、低密度视觉 fallback 和“不重复处理”的状态逻辑。
5. 任何使用系统浏览器 Cookie、任意 shell、固定私有 CDN URL 或未脱敏日志的实现。

### 15.9 代码引用和许可证记录

三个指定仓库的当前 GitHub 页面显示为 MIT License，但“代码许可证允许复用”不等于“平台访问行为可以无条件复制”。BiliNote README 还明确说明其抖音下载部分参考了其他项目，因此抖音相关实现需要单独建立来源和许可证记录。

如果最终直接复用第三方源代码，而不是独立重写，应在 `THIRD_PARTY_NOTICES.md` 或 capability pack manifest 中记录：

```text
component
source_repository
source_commit
license_expression
copyright_notice
modified_files
distribution_mode
transitive_dependencies
```

平台 provider、下载器和模型的版本都必须固定到 commit/tag 和 SHA-256；不能只写一个浮动的 GitHub URL。

### 15.10 发行级体积和许可证门禁

“能力包独立”只能降低主程序体积，不能代替发行审计。每个 capability manifest 必须补充下面的字段：

```json
{
  "packId": "media.asr.faster-whisper",
  "version": "1.0.0",
  "sourceCommit": "<pinned-commit>",
  "archiveSha256": "sha256:...",
  "licenseExpression": "MIT",
  "thirdPartyNoticesPath": "THIRD_PARTY_NOTICES.txt",
  "modelLicensePath": "MODEL_LICENSE.txt",
  "compressedSizeBytes": 0,
  "installedSizeBytes": 0,
  "modelCacheMaxBytes": 0,
  "temporaryWorkspaceMaxBytes": 0,
  "peakRssBytes": 0,
  "supportedTargets": ["x86_64-pc-windows-msvc"],
  "distributionMode": "optional_download"
}
```

Import V2 的资源限制至少分四层：

1. 单个 HTTP 响应和单个媒体的 `maxResponseBytes` / `maxMediaBytes`。
2. 解压、转码和能力包 staging 的 `temporaryWorkspaceMaxBytes`。
3. 一个 session 的 `maxTotalMediaBytes`、`maxImageCount`、`maxDurationSeconds` 和并发数。
4. 安装能力包前的磁盘剩余空间 `requiredFreeBytes`，以及模型缓存上限。

基础 Tauri 包不内置大型 ASR/OCR 模型；远程 VLM 不增加本地安装体积；FFmpeg、yt-dlp、ASR 和 OCR 只能在用户选择能力后下载。能力包安装前显示压缩大小、安装大小、模型缓存大小和许可证；缺少 NOTICE、模型许可证、SHA-256、支持平台或实际体积数据时 fail-closed，不允许发布。

### 15.11 开发环境一键可测与正式发行隔离

这是本项目的开发体验约束：开发者执行 `npm run tauri dev` 后，应能够直接测试 Import V2 的完整能力路由，不需要手工生成签名、维护 `install-catalog.json`、搭建 HTTPS 下载站或先理解能力包发行流程。能力包签名是正式发行的供应链安全门禁，不应成为日常开发的前置条件。

#### 15.11.1 两种运行模式必须明确隔离

| 模式 | 能力包来源 | 签名要求 | 目标 |
|---|---|---|---|
| Debug / `npm run tauri dev` | 本地开发能力包目录或用户级开发缓存；可由仓库内 runner 和一次性准备脚本生成 | 不要求正式签名；仍校验路径、大小、入口、协议、目标平台和文件清单 | 一条命令测试完整 UI、Import V2 状态机、JSON-RPC、浏览器、ASR、OCR 和恢复流程 |
| Release / `npm run tauri build` | 正式 HTTPS catalog 和正式发布 ZIP | 强制 Ed25519 manifest 签名、catalog SHA-256、文件清单和许可证门禁 | 面向用户的可信发行 |
| CI verification | 临时目录中的最小 fixture pack 和测试密钥 | 使用测试密钥走完整验签链路 | 自动验证 Release 安全路径，不依赖人工操作 |

禁止用运行时环境变量让用户构建的 Release 版本随意关闭验签。开发模式应通过 Rust 的 Debug 编译条件或明确的开发 Cargo feature 进入，Release 编译不得包含“接受任意未签名远程能力包”的路径。

#### 15.11.2 `npm run tauri dev` 的目标行为

启动流程应收敛为：

```text
npm run tauri dev
  → Debug 构建识别 development capability mode
  → capability-dev-prepare 检查本地 runner、依赖和模型缓存
  → 缺少时只执行一次本地准备/下载，并写入用户级开发缓存
  → ImportCapabilityRuntime 从 dev root 注册 browser / media / ASR / OCR / document routes
  → Import V2 可以直接进入 inspect → route → extract → validate → preview
```

开发能力包不应写入用户 Wiki 的 `raw/`、`wiki/` 或项目 `.app/` 内容目录。推荐使用应用数据目录下的独立开发根目录，例如：

```text
<app-data>/capabilities-dev/<capabilityId>/<version>/
```

仓库内的 `capabilities/<id>/` 继续作为 runner 源码、依赖声明、测试 fixture 和许可证资料；它不是用户项目内容，也不是正式下载 catalog。`capability-dev-prepare` 可以从这些源码生成最小 fake pack，或者在开发者明确启用真实能力时准备真实 runtime/model，并在用户级缓存中复用。

开发模式仍必须保留以下安全和可诊断约束：

- 能力包只能从显式的本地 dev root 或受控缓存读取，不能因为 Debug 模式而接受任意 URL；
- 禁止路径穿越、符号链接、特殊文件、超限解压和越界 entrypoint；
- 检查 manifest 的 `packId`、版本、协议、目标平台、许可证、entrypoint 和文件 inventory；
- fake runner 必须仍然遵循当前 `import.execute` / `EngineRequest` / `EngineResult` JSON-RPC 协议；
- 缺少真实模型时必须显示“开发能力未准备”，不能静默切换成看似成功的空结果；
- 启动准备、下载、缓存命中、失败原因和可重试动作必须进入任务日志。

#### 15.11.3 当前代码落点与实施要求

当前 `ImportCapabilityRuntime::load_installed` 使用 `CapabilityPackManager` 和嵌入式 `trusted-keys.json`，正式安装命令使用 `catalog_entry` 和 `install_catalog_entry`。因此当前仓库中 `install-catalog.json` 为空时，正式安装链路必然返回“没有适用于当前目标的签名能力包”。开发模式应新增独立的加载分支，而不是修改正式 catalog 来伪装开发包：

```text
services/import_v2/capability_runtime.rs
  ├─ load_installed_release(...)
  └─ load_installed_development(...)

services/import_v2/capability_dev.rs
  ├─ resolve_dev_root()
  ├─ prepare_or_reuse_dev_pack(...)
  ├─ validate_dev_manifest(...)
  └─ register_dev_pack(...)
```

`CapabilityPackManager` 的正式验签逻辑应继续保持 fail-closed。开发分支可以使用显式的 `CapabilityTrustMode::Development`，但只允许本地 root，并复用相同的文件、路径、协议和 runtime integrity 校验；不要给正式 `resolve` 增加一个可以由用户输入控制的 `skip_signature` 布尔参数。

开发准备完成后，UI 不应再弹出“安装签名能力包”对话框。能力已存在时直接显示已准备；能力缺失时显示一次性准备任务和日志。用户只需要重新执行 `npm run tauri dev` 或点击重试，不需要生成私钥。

#### 15.11.4 验收标准

实现此需求后必须满足：

1. 在 `install-catalog.json` 和 `trusted-keys.json` 为空的开发源码树中，`npm run tauri dev` 仍能加载最小开发能力包并测试完整导入流程。
2. 浏览器、媒体、ASR、OCR 和文档能力至少各有一个可运行的 dev fixture；真实 ASR/OCR 模型可以是首次准备并缓存的可选资源。
3. 开发能力包缺失、runner 崩溃、模型缺失、协议错误和取消操作都能在任务日志中看到明确原因。
4. `npm run tauri build` 不读取 dev root，不接受未签名远程包；正式 catalog、公钥和签名包仍按 15.6、15.10 的发行门禁工作。
5. CI 至少执行一次 signed fixture 的下载/解压/验签/运行/恢复测试，确保为了方便开发而增加的 Debug 分支没有覆盖或削弱 Release 校验。

## 16. 参考项目

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — 通用媒体下载、元数据、字幕、断点续传。
- [MediaCrawler](https://github.com/NanmiCoder/MediaCrawler) — 多平台浏览器登录和页面适配参考；注意其非商业学习许可。
- [XHS-Downloader](https://github.com/JoeanAmier/XHS-Downloader) — 小红书作品信息、图片/视频地址和下载记录参考。
- [douyin-downloader](https://github.com/jiji262/douyin-downloader) — 抖音图集/视频、重试、去重、浏览器 fallback 参考。
- [Douyin Capture Pro](https://community.obsidian.md/plugins/douyin-capture-pro) — 抖音到 Markdown/Obsidian 的输出与降级体验参考。
- [BiliNote](https://github.com/JefferyHcool/BiliNote) — B 站字幕优先、章节、截图和 AI 笔记参考。
- [FunASR](https://github.com/modelscope/FunASR) — VAD、标点、说话人和中文 ASR 能力参考。
- [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) — 中文、粤语、英文、日文、韩文语音识别参考。
- [faster-whisper](https://github.com/SYSTRAN/faster-whisper) — CTranslate2 优化的 Whisper 推理参考。
- [WhisperX](https://github.com/m-bain/whisperX) — 字级时间轴和说话人对齐参考。
- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) — 图片/PDF 结构化 OCR 参考。
