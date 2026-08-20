# LLM Wiki Desktop 安全、权限与隐私审查

日期：2026-08-14

来源：从《第一性原理对抗性审查》拆分

范围：项目 trust/write authority、密钥和网络目的地、Agent/Git 外部进程、撤权竞态、路径边界、Markdown 网络隐私、供应链

## 1. 结论

项目已经具备一套正确的安全思想：canonical project identity、trust store、Workflow access snapshot、epoch-bound launch permit、Import transaction、URL fetch SSRF policy、OS keyring、candidate workspace。这些不是表面措施，而是可继续扩展的成熟基础。

当前主要风险来自**安全水位不一致**：最成熟的 Workflow/Import 路径有严格边界，另一些旧 command、Chat、LLM、Git 路径仍能绕过。安全门如果只靠调用者“记得调用”，就不是强制边界。

本轮未确认无前置条件即可全面失陷的 P0，但以下 P1 均应视为公开发布阻断项。

## 2. P1 发布阻断项

### SEC-P1-01 全局 Provider secret 可被发送到请求指定的任意 origin

**Batch 6 状态（2026-08-21）：代码与负向合同 Closed；packaged 对抗证据 Pending。** Commit `f99a2e8a` 将 credential 绑定到 project/config/canonical origin，并覆盖官方 origin、Custom 授权、redirect、loopback/private/DNS rebinding；Batch 6 的 `provider_secret_origin_contracts` 13/13 与最终 redline 为绿。真实签名包中的 provider/网络矩阵仍见 [`../release/batch-6-acceptance-evidence.md`](../release/batch-6-acceptance-evidence.md)，不得据此批准 public beta。

对应总报告：P1-01。

`test_llm_provider` 只做 project resolve，随后按请求中的 provider 类型从 OS keyring 取 secret，并向请求指定的 base URL 发送带秘密 header 的 POST：

- `src-tauri/src/commands/llm_commands.rs:149-159`
- `src-tauri/src/services/llm_service.rs:34-73,76-128`
- `src-tauri/src/services/secret_service.rs:21-60`

校验允许任意 `http`/`https` origin；secret 仅按 provider 类型保存，没有绑定 canonical origin/config id。`check_ollama_reachable` 还会对项目 settings 指定的任意 host/port 发 `/api/tags` GET：

- `src-tauri/src/commands/llm_commands.rs:89-128`
- `src-tauri/src/services/settings_service.rs:38-47`

**攻击/误用前提**：IPC 调用者控制 provider config，或用户在恶意/被篡改项目配置下执行连接测试。因此定为 P1，而非无前提 P0。

**影响**：OpenAI/Anthropic/Google key 可能发往攻击者主机；HTTP 可明文传输；Ollama probe 可用于内网/metadata 地址探测；项目配置与全局凭据发生信任域混淆。

**修复**：

- 读取 secret 前检查 external-AI authority；
- credential account 绑定 provider + canonical HTTPS origin + config id；
- 官方 provider 只允许官方 HTTPS origin；
- Custom 首次使用展示 host/scheme/credential binding 并由后端签发授权；
- HTTP 仅显式批准的本机 provider；
- 禁自动 redirect，或只允许 same-origin 且 origin 变化剥离秘密 header；
- Ollama 复用 Import URL fetch 的 DNS/IP/redirect policy。

**验收**：untrusted、`http://attacker`、307/308、link-local、IPv4-mapped IPv6、DNS rebinding、项目 A/B 不同 origin；攻击 server 请求计数必须为零。

### SEC-P1-02 mutation authority 可被大量生产 command 绕过

**Batch 6 状态（2026-08-21）：代码与静态 authority inventory Closed；packaged 权限矩阵 Pending。** Commit `8abc93a2` 引入后端不可伪造的 write/task/authority permits、命令与 service 边界及 revoke barrier；Batch 6 最终 redline 与相关 Rust 集成矩阵为绿。真实 read-only/restricted/untrusted/recovery 安装包旅程尚未完成。

对应总报告：P1-02。

`resolve_project_context` 只验证 project id、canonical root、layout：`src-tauri/src/app_state.rs:483-490`。正确写门已经存在：`src-tauri/src/app_state.rs:670-735`。但以下代表路径仍只持普通 context：

- Import V2 final confirm/commit：`src-tauri/src/commands/import_v2_commands.rs:1264-1353`
- Source 生命周期：`src-tauri/src/commands/source_commands.rs:77-117,742-805,830-859`
- Wiki save/create/rename：`src-tauri/src/commands/wiki_commands.rs:154-217`
- Settings/provider 写入：`src-tauri/src/commands/settings_commands.rs:37-46`、`src-tauri/src/commands/llm_commands.rs:41-47`
- Graph cache/layout 写入：`src-tauri/src/commands/graph_commands.rs:13-61,129-134`

**影响**：registered path 被误当成写权限；untrusted-but-writable 项目可被 IPC 写入；Recovery/read-only 的失败语义不一致；revoke 可发生在 resolve 与实际写入之间。

**修复**：mutation facade 只能接收后端不可伪造的 `ProjectWritePermit`；commands 统一通过 `with_current_project_write_access`；内容写再绑定 permitted root；read API 的 cache 使用 `MemoryOnly` policy。

**验收**：trusted/untrusted × writable/read-only × healthy/recovery 全命令矩阵；barrier 后并发 revoke；失败时目录树逐字节不变并返回统一 typed error。

### SEC-P1-03 Chat 撤权不能阻断检索后的外部启动和写回

**Batch 6 状态（2026-08-21）：代码与竞态合同 Closed；真实 provider/Agent packaged barrier Pending。** Commits `8abc93a2`、`11b187d3` 将外部执行绑定 project epoch/lease，并为 revoke 增加取消、drain 和写回重验；Batch 6 聚焦前端/Rust矩阵为绿。签名包中的真实 Chat/BYOK/Agent 外发计数仍未取证。

对应总报告：P1-03。

Chat 在开始/检索前检查权限，真正 Agent launch 或 BYOK send 前没有取得 epoch-bound permit：

- `src-tauri/src/commands/chat_commands.rs:125-175,220-276`
- Agent：`src-tauri/src/commands/chat_commands.rs:278-335`
- BYOK：`src-tauri/src/commands/chat_commands.rs:346-387`

当前 revoke barrier 只关闭 Workflow owner：`src-tauri/src/app_state.rs:1035-1145`。

**影响**：用户点击撤销信任后，慢检索结束仍可能把项目内容发给外部模型，回答也可能写回 `.app/chats/`；“撤销成功”不代表外部活动已停止。

**修复**：把 Workflow launch registry 泛化为所有 Agent/BYOK/Skill 的 project execution epoch；spawn/send 前持 lease；revoke 取消 root 下全部外部 task 并等待 barrier；写回使用 epoch compare-and-commit。

**验收**：retrieval/launch、request/commit 两个 barrier 竞态；revoke 后 provider/Agent 调用和 Chat 新写入都为零。

### SEC-P1-04 普通 Git runner 可执行项目 hooks/fsmonitor

**Batch 6 状态（2026-08-21）：代码与 fake-process 负向合同 Closed；packaged 恶意 Git fixture Pending。** Commit `11b187d3` 统一 hardened Git/process lifetime，禁 hooks/fsmonitor/diff/credential helper/prompt 并收口环境、超时、取消与进程树；源码总门通过。真实安装包、AV/RDP/平台进程树证据仍 Pending。

对应总报告：P1-04。

assessment runner 已禁 hooks/fsmonitor/prompt 并有 timeout；checkpoint/status 使用的普通 runner 没有这些保护：

- 安全 runner：`src-tauri/src/services/git_service.rs:1117-1180`
- 普通 runner/status/commit：`src-tauri/src/services/git_service.rs:368-419,1249-1326`

**影响**：项目 `.git/config` 可让 checkpoint/status 隐式启动本机程序、读取继承环境或永久挂住；安全检查自身反而成为执行入口。

**修复**：所有 app-owned Git 统一 hardened runner；禁 hooks/fsmonitor/external diff/textconv/credential helper/pager/prompt；环境白名单、stdin null、输出上限、timeout/cancel、process-tree termination。

**验收**：hook/fsmonitor marker 不生成；无限 hook 按时失败；项目 A 卡住不阻塞项目 B；敏感 env 不可被 hook/diff 读出。

### SEC-P1-05 Agent Chat 给不可信项目内容过宽的本机能力

**Batch 6 状态（2026-08-21）：代码与 bounded candidate/process 合同 Closed；真实 sandbox 攻击旅程 Pending。** Commit `11b187d3` 将 Chat/Source AI 收口到无工具或 private candidate snapshot、环境白名单和受限写范围；Batch 6 Rust workflow/Agent 矩阵通过。项目外 sentinel 与敏感环境的四平台 packaged 证据尚未完成。

对应总报告：P1-05。

Chat prompt 鼓励 Agent 继续读 Wiki；Claude Chat 预批准 `Read/Grep/Glob`，convenience 允许 `Edit/Write/Bash`，Codex 可获得 shell：

- `src-tauri/src/services/chat_service/retrieval.rs:355-413`
- `src-tauri/src/services/agent_service.rs:758-852`

普通 streaming Chat 使用 `persist_output_logs=true`，但环境清理只在 `!persist_output_logs` 时调用：

- `src-tauri/src/services/agent_service.rs:1634-1650,1756-1777`
- 成熟安全实现：`src-tauri/src/services/agent_service.rs:2558-2774`

**影响**：Wiki 中的 prompt injection 可能诱导 CLI 读取项目外文件、环境变量，或在 convenience 路径造成项目外副作用。项目内 Git rollback 无法撤回项目外写入。

**修复**：普通 Chat 由应用完成 bounded retrieval，交给无工具模型；如需读文件，使用 OS 级只读 snapshot，只包含授权 Markdown；所有 Agent 路径无条件 `env_clear`；写操作只在 candidate workspace。

**验收**：恶意 Wiki 尝试读项目外 sentinel、输出敏感 env、写项目外 marker，均无法成功；需要真实 sandbox 测试，不接受只靠 prompt 文案。

### SEC-P1-06 Windows mutation 仍有 path-based TOCTOU

**Batch 6 状态（2026-08-21）：生产 primitive 与确定性 race 合同 Closed；真实 packaged 持续切换矩阵 Pending。** Commit `17af8c19` 引入 retained directory-handle mutation capability，并迁移 Import/FileStore/高风险调用；Batch 6 `project_mutation_binding_contracts` 3/3。Windows junction 与 macOS/Linux symlink 的签名包压力证据仍是 release blocker。

对应总报告：P1-06。

Import transaction 在 Unix 使用 handle/descriptor-relative 操作；非 Unix `bound_replace_existing` 忽略 binding 并走 `MoveFileExW`，remove 也是 revalidate 后按路径删除：

- `src-tauri/src/services/import_v2/transaction.rs:1355-1359,1506-1509,1799-1804,1999-2029`

**影响**：同用户外部进程若在验证与 mutation 之间切换 junction/reparse point，可能把覆盖、删除、rename 或 rollback 引向项目外。

**修复**：Windows retained directory handle、reparse-safe open、handle-relative rename/disposition；通用 FileStore 不再消费裸 `PathBuf`。

**验收**：真实 Windows runner 持续切 junction；全部 mutation fail closed，项目外 sentinel 不变。Unix symlink 做同矩阵。

## 3. P2 安全与隐私项

### SEC-P2-01 远程 Markdown 图片会自动联网，WebView CSP 为空

对应总报告：P2-07。

Wiki reader 对非本地图片保留远程 URL，Chat ReactMarkdown 没有统一 image override；Tauri 配置 `csp: null`：

- `src/features/wiki/MarkdownReader.tsx:273-280`
- `src/features/chat/MessageContent.tsx:78-91`
- `src-tauri/tauri.conf.json:24-26`

**影响**：打开不可信文档即可向图片 host 暴露请求时间和 IP，形成 tracking beacon；缺少 CSP 使 WebView 失去重要的防御纵深。ReactMarkdown 默认不执行 raw HTML，只是降低直接脚本风险，并没有消除网络隐私问题。

**修复**：默认阻止/代理远程图片，按 host 明示授权；建立严格 CSP；回归 Markdown、iframe、export preview、remote script/connect，同时保留 app local scheme、KaTeX、highlight。

### SEC-P2-02 authority revalidation 把慢 Git/fsync 放在全局锁内

对应总报告：P2-12；性能交叉项。

这不仅慢，也扩大了安全操作的可用性攻击面：恶意项目 Git 可以让其他项目的权限检查和 revoke 一起等待。

**修复**：慢 I/O 锁外执行，完成后 CAS authority epoch；per-project lock；统一 hardened Git timeout。

### SEC-P2-03 供应链与可复现发布不足

对应总报告：P2-15。

普通 CI 仍使用 floating action tags、Rust `stable`、Node major，cargo 未统一 `--locked`；桌面 artifact 无 SBOM、provenance、dependency audit 和 release attestation。

**影响**：同一源码在不同时间可能使用不同工具或 action；依赖风险和产物来源难以证明。

**修复**：actions SHA pin、精确 toolchain、locked dependency、cargo/npm audit policy；installer 同时发布 SBOM、provenance、签名。

### SEC-P2-04 通用 FileStore pathname open 仍有跨进程竞态

对应总报告中的路径安全延伸项。

`resolve_project_write_path` 返回普通 `PathBuf`，安全 helper 自身承认后续 pathname open 可被外部进程替换；FileStore 再按路径创建 temp/rename：

- `src-tauri/src/models/paths.rs:145-201`
- `src-tauri/src/utils/path_safety.rs:144-148`
- `src-tauri/src/services/file_store.rs:74-98,364-421`

**修复**：平台级 `SafeProjectDirHandle`；Unix `openat/openat2 + O_NOFOLLOW + renameat`；Windows directory handle + reparse-safe identity revalidation。

## 4. 已有成熟防护

1. canonical root 与 project id 绑定、identity/layout drift 自动撤权；
2. Workflow access 重验 trust/health/filesystem/Git；
3. Workflow launch 有 epoch permit 和 revoke barrier；
4. Import URL fetch 有 DNS/IP 校验、连接 pin、手工 redirect、流大小和取消；
5. Source delete/replace 有精确确认、preview token、Git checkpoint、transaction；
6. secret 生产存储使用 OS keyring，不写项目 JSON；
7. isolated Agent candidate 路径已有 env allowlist、输出 cap、deadline、process-tree termination。

真正需要做的是让这些成熟机制成为通用 infrastructure，而不是为每条新 command 复制一份“记得调用”的约定。

## 5. 安全验收门

公开 beta 前应有以下 release gate：

- 所有 Tauri command 被静态枚举为 `read / mutation / network / external-process / secret` 之一；
- mutation 必须持不可伪造 write permit；network + secret 必须持 origin-bound authorization；
- external process 必须经 hardened environment、timeout、cancel、process-tree guard；
- trust revoke 必须关闭并等待该 root 的全部外部执行，而不仅是 Workflow；
- Windows junction 与 Unix symlink 竞争测试不得 skip；
- 恶意项目配置、Markdown prompt injection、redirect/DNS/内网地址矩阵进入 CI/packaged test；
- 安全失败必须是可本地化 typed error，且不会产生部分写入或秘密日志。

## 6. 推荐顺序

1. secret-to-origin 绑定与 Ollama SSRF；
2. `ProjectWritePermit` 和全命令迁移；
3. Chat/Agent/BYOK execution epoch + revoke barrier；
4. hardened Git 和统一 process lifetime；
5. Agent bounded retrieval/snapshot 与无条件 env hardening；
6. Windows/通用 handle-relative filesystem mutation；
7. CSP、远程图片策略和供应链 attestation。
