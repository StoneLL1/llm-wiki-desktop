# LLM Wiki Desktop 发布、测试与运维成熟度审查

日期：2026-08-14

来源：从《第一性原理对抗性审查》拆分

范围：能力包交付、桌面安装与更新、三平台真实产物、测试分层、供应链、性能门禁、离线与支持诊断、文档一致性

## 1. 结论

源码级工程质量明显强于发布工程：仓库有三 OS CI matrix、1,000+ 前端和 1,000+ Rust 测试、较多 CJK/路径/并发合同，但没有证据证明“用户拿到的安装包”能够完成同样的旅程。

当前最明显的断层是：

- capability release workflow 会生成能力制品，但 desktop build 不会把 catalog 注入应用；
- 设置中出现更新功能，但后端 updater、签名、endpoint 和 release workflow 不存在；
- CI 运行 jsdom/Vite/Rust，却不安装、启动、升级和卸载真实 Tauri 产物；
- 真实 WebView、keyring、系统对话框、tray/notification、GPU、进程树、断网和强杀恢复都没有 release gate。

因此当前状态适合内部开发/alpha，不满足可公开交付的运维闭环。

## 2. P1 发布阻断项

### RELENG-P1-01 干净构建没有可安装 capability catalog

**Batch 6 状态（2026-08-21）：release 注入/校验实现 Closed；真实四 target 安装 Not Closed。** Commits `1209f9e9`、`b55b7007`、`5eac9315` 已建立 reusable non-publishing capability build、4×5 catalog/embedded contracts、安装恢复和同一 release transaction 注入。Batch 6 release/config 30/30 与 capability 工具 Node 66/66、Python 9/9 通过；但生产 signing key/trusted catalog 与四平台签名包不存在，因此 strict redline 正确保持 RED，详见 [`../release/batch-6-acceptance-evidence.md`](../release/batch-6-acceptance-evidence.md)。

对应总报告：P1-09。

`capabilities/install-catalog.json:1-4` 的 `entries` 为空；文件被 `include_str!` 编入应用：`src-tauri/src/services/import_v2/capability_installer.rs:44-52`。catalog 不命中时后端返回 source build 无 signed artifact，前端禁用安装按钮：

- `src-tauri/src/commands/import_v2_presentation_commands.rs:1116-1126`
- `src/features/import/ImportCapabilityDialog.tsx:70-77`

现有测试只对 entries 执行 `all(valid)`，空列表会真空通过：`src-tauri/src/services/import_v2/capability_installer.rs:574-585`。

与此同时 `.github/workflows/capability-release.yml` 已生成四 target × 五 pack 的 signed catalog，但没有 desktop release job 下载并注入。

**影响**：新装用户遇到 OCR、ASR、browser 等缺失能力时，只能看到永久不可安装；Import 的可恢复产品承诺中断。

**修复**：desktop release job 下载与 tag/commit/target 精确匹配的 catalog + trusted key，替换 build-time input 后再 `tauri build`；空/缺项/hash/tag 不匹配直接 fail。

**验收**：四 target 干净 VM 完成 download → signature/hash → install → health → 原 Import session continue；空 catalog release 必败。

### RELENG-P1-02 应用更新是不可达的占位功能

**Batch 6 状态（2026-08-21）：源码更新器与 UI Closed；真实旧版升级 Not Closed。** Commits `8db5b4ca`、`9f8bc2ac` 已实现固定 HTTPS endpoint、签名/manifest 边界、ephemeral offer、下载/取消/重试、全局偏好、安装 receipt 与 dirty/task/confirmation restart guard；Batch 6 updater 14/14、前端聚焦组和签名 verifier 均通过。stable `latest.json` 仍为 404，四平台 old→new signed upgrade 没有证据。

对应总报告：P1-10。

`UpdateSettings.tsx` 只读取当前版本，`latestVersion` 永远为空；所谓下载只改本地文案：`src/features/settings/UpdateSettings.tsx:25-55,88-95`。Tauri/Cargo 无 updater plugin、endpoint 或 signing 配置。

**影响**：用户无法安全升级、回滚；“自动检查更新”开关没有实际效果；安全修复无法可靠送达用户。

**修复**：未实现前隐藏或明确标为不可用；随后建立签名 installer、updater manifest、显式同意、staged rollout、rollback 和旧版本升级路径。

**验收**：三平台旧签名版本检测、下载、安装新签名版本；离线、源不可达、manifest/signature 错误都有明确可恢复状态。

### RELENG-P1-03 CI 没有验证真实 packaged desktop

**Batch 6 状态（2026-08-21）：atomic workflow 与 local fixture Closed；真实 packaged release gate Not Closed。** Commit `5eac9315` 提供四 target 同 run build/sign/manifest/SBOM/provenance/smoke/draft transaction，只有 protected final publisher 获得 `contents: write`。本地 asset rehearsal 通过，但 canonical repository 匿名不可达、GitHub 授权失效且签名身份/reviewer 未配置，未产生 workflow URL、draft 或安装包；public beta 保持 No-Go。

对应总报告：P1-10 及测试缺口。

`.github/workflows/ci.yml` 的三 OS matrix 是良好基础，但只覆盖 jsdom/Vite/cargo check/test；没有 `tauri build`、签名/公证、安装器、WebView 启动和核心旅程。

jsdom 中 Tauri IPC 被 mock，Canvas/WebGL 返回 null；这无法验证：

- WebView2/WKWebView/WebKitGTK 差异；
- OS keyring 与权限弹窗；
- file picker、tray、notification；
- Sigma/WebGL 与 GPU fallback；
- 真 Agent/capability 二进制及 process-tree cancellation；
- 安装、升级、卸载、强杀恢复。

**影响**：源码 gate 绿色不等于安装包可用；最容易漏掉的恰好是桌面平台差异和发布配置。

**修复**：三平台 packaged smoke 作为 release required check；安装真实 artifact 后启动并执行固定旅程。

## 3. P2 发布与运维项

### RELENG-P2-01 capability 下载没有断点恢复和启动清理

**Batch 6 状态（2026-08-21）：代码与测试 Closed；真实 25%/75% crash matrix Pending。** Commit `b55b7007` 覆盖 partial identity、Range/重下、orphan reaping、取消区别、最终 hash/signature 与 health rollback；真实 target 证据仍需 draft artifacts。

对应总报告：P2-08；可靠性主报告详述。

大包强杀后重头下载并可能留下 orphan。发布 capability catalog 后，这个问题会立即从潜在风险变成用户可见问题。

**验收**：25%/75% 强杀恢复、Range/无 Range 两种服务、主动取消与进程 crash 区分、无 orphan 增长。

### RELENG-P2-02 供应链和构建可复现性不足

**Batch 6 状态（2026-08-21）：workflow contract Closed；remote provenance/signing evidence Pending。** Commits `1209f9e9`、`5eac9315` 固定 actions/toolchains/lockfiles，生成 checksums、SBOM、provenance/attestation 并验证 exact tag/commit/run。没有远端 draft run，故不能声称真实供应链 attestation 已生成。

对应总报告：P2-15。

普通 CI 使用 floating action tags、Rust `stable`、Node major；cargo check/test 没有统一 `--locked`；桌面 artifact 无 SBOM、license/provenance、dependency audit、attestation。capability release 已 SHA pin，说明仓库已有可复用模式。

**修复**：

- GitHub Actions SHA pin；
- 精确 Node/Rust toolchain；
- `npm ci`、`cargo --locked`；
- cargo/npm audit policy和例外台账；
- installer + updater manifest + capability pack 同一 provenance 链；
- SBOM、签名和构建 attestation 随 release 发布。

### RELENG-P2-03 权威文档存在当前状态漂移

**Batch 6 状态（2026-08-21）：Closed。** `SPEC/SPEC.md` 16.7 已按当前 `App -> AppShell -> WorkspaceRouter -> NoProjectWorkspace` 与 typed assessment 实现修正，并继续以架构合同禁止恢复独立 launch 页面或普通资料目录原地初始化。

对应总报告：P2-16。

`SPEC/SPEC.md:725-729` 仍把独立 `ProjectStartView`、二元 assessment 描述成当前状态；实际 App 已始终渲染 shell，并由 `NoProjectWorkspace` 承担无项目工作台：

- `src/app/App.tsx:10-27`
- `src/components/app/WorkspaceRouter.tsx:64-67`

**影响**：后续实现者和 Agent 可能按旧“当前差距”复活已废弃首屏，或错误判断哪些安全流程未落地。

**修复**：更新当前状态，明确 legacy component unreachable；架构测试禁止无项目路径绕过 shell。

### RELENG-P2-04 缺少离线、强杀、磁盘与真实进程测试

local-first 产品的核心承诺不是“网络正常时可用”。当前需要补齐：

- 完全断网仍可阅读、搜索、编辑可信可写项目；
- provider/update/capability 失败不拖死 shell；
- 长任务 cancel、强杀、重启恢复；
- 磁盘满、文件被占用、权限突然变化；
- 真 Agent/capability 子进程 5 秒内清理；
- 真实 keyring denied/locked；
- read-only/restricted/untrusted/recovery 全旅程。

### RELENG-P2-05 缺少性能与包体 release budget

Vite 配置没有 initial JS raw/gzip/module-count budget、chunk allowlist 或“重型库不得进入 shell”的断言。真实 packaged WebView 也没有 cold/warm startup、route p50/p95、long tasks、heap 或 stream stress gate。

**影响**：性能可以在所有功能测试通过时持续退化，直到用户主观反馈才发现。

**修复**：构建产物预算 + packaged runtime trace；预算变化必须在 PR 中显式说明。

### RELENG-P2-06 缺少自动化可访问性 release gate

现有 role/键盘单测与 skip link 是良好基础，但没有 axe、真实 WebView screen reader、高对比、缩放和 reduced-motion release checklist。

**修复**：自动 axe 作为盲区筛查，三平台读屏/键盘 smoke 作为发布清单；不能用单一 axe 分数代替人工核心旅程。

## 4. 推荐测试金字塔

### 4.1 L0：纯函数与静态合同

- DTO/schema/locale parity；
- command 权限分类；
- bundle boundary/chunk allowlist；
- path/URL normalization；
- parser、cache identity、revision/CAS；
- a11y role/name/state 基础合同。

### 4.2 L1：前端 jsdom + Rust service tests

- store/project epoch；
- error presentation；
- stream batching publication；
- authority permit/revoke barriers；
- transaction/fault injection；
- LLM/download/scan budgets；
- Git/Agent fake process lifetime。

### 4.3 L2：Tauri integration

- 真实 IPC serialization；
- local filesystem、keyring、Git、process tree；
- WebView resource/lazy failure；
- file picker/dialog/tray/notification；
- GPU/WebGL fallback；
- network redirect/DNS/private-address policy。

### 4.4 L3：Packaged release journey

三平台安装真实签名 artifact 后：

1. 启动；
2. 新建知识库；
3. 打开空格/CJK/长路径 native 与 compatible vault；
4. Wiki/Chat/Graph/Import/Source/Lint/Exports/Workflows/Settings 全板块切换；
5. 导入小文件、需要 capability 的文件、取消长任务；
6. 强杀与重启恢复；
7. restricted/read-only/untrusted/recovery；
8. tray/notification/system dialog；
9. 旧版本升级到当前版本；
10. 卸载并验证用户项目未被删除。

## 5. Release artifact 必备清单

| 项目 | 要求 |
| --- | --- |
| Installer | 三平台真实安装/卸载，用户项目不随卸载删除 |
| Signing | Windows/macOS/Linux 对应签名与校验策略 |
| Notarization | macOS 公证与 Gatekeeper smoke |
| Updater | signed manifest、显式同意、staged rollout、rollback |
| Capability catalog | target/tag/hash/key 精确绑定，空目录 fail |
| SBOM | app + Rust/npm + capability packs |
| Provenance | commit、toolchain、dependencies、artifact hash 可追踪 |
| Support data | 用户显式导出、路径/内容/secret redaction |
| Recovery | interrupted install/update 不破坏旧可运行版本 |
| Offline | 本地核心功能不被网络模块失败阻塞 |

## 6. 建议的发布门

### Internal alpha

- 源码 gate 通过；
- 高风险功能只给受控测试数据；
- 已知 P1 有明确 owner 和隔离说明。

### Public beta

- 安全审查 P1 全部关闭；
- capability 和 updater 真实可用；
- 三平台 packaged smoke 通过；
- 数据迁移/回滚/强杀恢复通过；
- 核心 UX 性能预算通过；
- 隐私、离线、a11y checklist 有证据。

### Stable

- staged rollout + rollback 演练；
- 至少一次旧版本到新版本真实升级；
- crash/support trace 可由用户安全导出；
- SBOM/provenance/签名随 release；
- 已建立 release incident 与紧急撤回流程。

## 7. 推荐顺序

1. desktop build 注入 capability catalog；
2. 三平台 unsigned packaged smoke，先证明真实产物可运行；
3. installer signing/notarization；
4. updater + staged rollout/rollback；
5. supply-chain pinning、SBOM、provenance；
6. 扩充离线/强杀/keyring/GPU/进程树矩阵；
7. 把性能、a11y 和文档一致性纳入 release required checks。
