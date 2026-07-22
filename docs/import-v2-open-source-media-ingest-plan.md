# Import V2：小红书 / 抖音 / B 站图文视频导入实现方案

> 状态：设计方案，可直接交给开发实施
>
> 适用范围：当前 LLM Wiki Desktop 的 `Import V2`，只负责“来源导入、证据保全、标准化提取与预览”；Wiki 编译、摘要、知识图谱与问答仍在用户确认后由 Agent / BYOK 流程完成。
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

每个 pack 使用一行一个 JSON-RPC 2.0 请求/响应。stdout 只能输出协议消息；诊断日志写 stderr。请求和响应必须与当前 `pack_protocol` 的版本/id 校验一致。

```json
{
  "jsonrpc": "2.0",
  "id": "request-uuid",
  "method": "media.inspect",
  "params": {
    "protocolVersion": "2",
    "platform": "bilibili",
    "locator": "https://www.bilibili.com/video/BV...",
    "publicUrl": "https://www.bilibili.com/video/BV...",
    "stagingRoot": "C:/.../.app/import-sessions/session/items/item/staging",
    "profileRef": null,
    "limits": {
      "maxMediaBytes": 1073741824,
      "maxImageCount": 100,
      "timeoutMs": 180000
    },
    "requestedFeatures": {
      "metadata": true,
      "media": false,
      "subtitles": true
    }
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "id": "request-uuid",
  "result": {
    "canonicalUrl": "https://www.bilibili.com/video/BV...",
    "platformId": "BV...",
    "metadataPath": "evidence/provider-result.json",
    "sourceSnapshotPath": "source/source.json",
    "assetPaths": ["assets/cover.jpg"],
    "subtitleCandidates": ["subtitles/zh-CN.vtt"],
    "quality": {
      "metadataComplete": true,
      "mediaComplete": false,
      "warnings": []
    }
  }
}
```

规则：

- 所有输出路径必须是 staging 相对路径；禁止绝对路径、`..` 和符号链接逃逸。
- `sourceSnapshotPath`、`metadataPath`、`assetPaths` 必须列在响应中，不能让 Rust 扫描整个目录猜结果。
- 响应必须带 `engineId`、`engineVersion`、`warnings` 和 `quality`。
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

## 15. 参考项目

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
