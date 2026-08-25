# LLM Wiki Desktop 三平台 CI 收尾与首次发布准备计划

> 日期：2026-08-24  
> 状态：Active / implementation not started  
> 工作分支：codex/rework-cross-platform-ci  
> 基准提交：7bc7b3afb8a7d4933c411701b38ce640c3a81081  
> 失败证据提交：b464941a70b2413ee195266c6d3d20b97f0729fb  
> 失败运行：https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/32482972129  
> 计划所有者：当前 CI 收尾任务  
> stable 发布权限：保留给用户最终明确批准

## 1. 计划目的

本计划分成两个严格串行的阶段。

阶段 A 的唯一完成标准是：同一个提交 SHA 在 GitHub Actions 的 Windows、macOS、Ubuntu 三个平台上全部完成且结论为 success，并且该提交按受保护流程合入 master。任何本地通过、部分 Job 通过、被取消运行或重新运行不同 SHA 都不能替代这个标准。

阶段 B 只有在阶段 A 完成并合入 master 后才能启动。阶段 B 按发布 Runbook 准备受保护的签名 Draft 候选和四平台安装、升级、启动、卸载验收。没有用户最终明确批准，不创建或推送正式 stable tag，不批准 publish-stable，不发布 stable GitHub Release，也不修改 latest updater channel。

本文重点详述阶段 A。阶段 B 仅定义进入条件、边界和交接清单；发布执行仍以 docs/release/release-runbook.md 为唯一流程权威。

## 2. 不可破坏的边界

本任务无论为了复现、修复还是缩短 CI 时间，都必须保持以下约束：

1. 不运行 git clean，不删除或批量移动主 worktree 的未跟踪文件。
2. 不 apply、pop 或 drop stash@{0}。
3. 不 force-push 当前或新远端分支。
4. b464941a 只作为失败证据和历史参考，不直接 cherry-pick。
5. 不以增加 sleep、扩大 timeout、全局串行、忽略测试、吞掉错误或重试到通过作为根因修复。
6. 不弱化路径 containment、symlink/junction/reparse-point 拒绝、Git 配置隔离、子进程树回收、项目信任、项目可写性和健康状态的 fail-closed 行为。
7. 不让测试连接真实 OS keyring、真实账号、生产签名输入或外部网络，除非该测试本身明确是受控集成验收。
8. 不把密钥、证书、密码或其值读取、打印、写入日志、文档、工作区或聊天。
9. 所有生产代码修复必须先有能说明缺陷的聚焦回归测试；测试失败原因必须与远端证据一致。
10. 修改高风险共享代码后必须从仓库根目录重新运行完整 npm run check。
11. 修改代码后运行 graphify update .，只提交与本次改动有关的 Graphify 产物。
12. 执行两轮独立 review：共享上下文的集成审查和 fresh-context 的盲点审查。

## 3. Git 与工作树基线

### 3.1 主 worktree

- 路径：D:\Users\Aletta\Desktop\Works\llm-wiki-desktop
- 实际分支：master
- 实际 HEAD：2d908263a5857cb0540533cb323ba1a354a0b67d
- 大量未跟踪文件保持原位。
- stash@{0} 仍为 pre-rollback-7bc7b3af-tracked-2026-08-24。
- 主 worktree 不作为本次修复的编辑目录。

### 3.2 隔离 worktree

- 路径：D:\Users\Aletta\Desktop\Works\llm-wiki-desktop-ci-rework
- 分支：codex/rework-cross-platform-ci
- 起点：7bc7b3afb8a7d4933c411701b38ce640c3a81081
- 创建方式：linked worktree；未切换主 worktree。

Graphify 查询会更新 .vocab.txt 和 last_query_stamp。它们在提交前必须与真实代码图更新一起审查；纯查询噪声不得混入 CI 修复提交。

## 4. 权威输入

实现前已经核对以下输入，后续出现冲突时按此顺序裁决：

1. AGENTS.md 与用户本任务指令。
2. SPEC/SPEC.md 第 16 节的安全、信任、文件、进程和发布约束。
3. 当前代码与聚焦测试。
4. progress.txt 与 gotchas.txt 顶部的三平台 CI 记录。
5. .github/workflows/ci.yml。
6. docs/superpowers/plans/2026-08-16-pre-release-final-four-remediation-plan.md。
7. docs/release/batch-6-acceptance-evidence.md。
8. docs/release/release-runbook.md。
9. docs/release/release-identity-and-access.md。
10. b464941a 对应 GitHub Actions 的完整失败日志。

Graphify 查询使用图内真实词表：

rust, test, git, process, project, trust, secret, identity, path, symlink, timeout, workflow

查询确认计划必须覆盖 AppState/ProjectRegistry、SecretService、Import V2 transaction、GitService、AgentService、Workflow project identity 和高风险写入检查点链路。Graphify 只用于导航，最终结论必须回到源码、测试和真实运行日志验证。

## 5. 已确认的远端失败事实

### 5.1 运行汇总

| 平台 | Job ID | Rust 测试结果 | 退出码 |
| --- | --- | --- | --- |
| Ubuntu | 96773176319 | 1171 passed, 10 failed, 4 ignored | 101 |
| Windows | 96773176546 | 1169 passed, 1 failed, 4 ignored | 1 |
| macOS | 96773176561 | 1174 passed, 7 failed, 4 ignored | 101 |

三个平台都只在 Run Rust service tests 失败。此前 npm ci、release contracts、frontend tests、capability tests、lint、frontend build、console scan 和 Tauri GUI Rust compile 均通过。

### 5.2 Ubuntu 失败清单

| 测试 | 真实错误 | 初步分类 | 稳定性判断 |
| --- | --- | --- | --- |
| app_state::project_registry_tests::project_write_critical_sections_do_not_block_unrelated_roots | worker unwrap PROJECT_WRITE_STATE_UNAVAILABLE，主线程随后收到 Disconnected | 项目健康/信任重验证或并发状态；根因待复现 | 未证实稳定 |
| app_state::project_registry_tests::project_writes_require_trust_writable_health_and_a_specific_content_root | PROJECT_WRITE_STATE_UNAVAILABLE | 项目健康评估或路径身份；根因待复现 | 未证实稳定 |
| services::git_service::tests::app_owned_git_disables_hooks_fsmonitor_and_textconv | GIT_COMMAND_TIMEOUT；日志 args 为 status --porcelain=v1 -z -uall | Git policy scan、真实 Git 子进程或资源回收问题 | 负载相关，不能称 flake |
| services::git_service::tests::blocked_project_git_lane_does_not_block_another_project | project B Git should not wait on project A's lane: Timeout | lane 隔离测试被真实 Git 进程混入，或真实跨根阻塞 | 负载相关，待拆分证据 |
| services::import_v2::connector_session::binding_tests::media_connector_uses_one_persistent_profile_per_platform | SECRET_BACKEND_FAILED；org.freedesktop.secrets 不存在 | 测试错误依赖真实 OS keyring | 对 headless Ubuntu 高概率稳定 |
| services::import_v2::connector_session::binding_tests::platform_revoke_removes_a_profile_without_a_live_session | 同上 | 测试错误依赖真实 OS keyring | 对 headless Ubuntu 高概率稳定 |
| services::import_v2::source_lifecycle::tests::refresh_executes_the_registered_route_instead_of_reusing_current_markdown | 同上 | 测试构造使用 ImportV2Service::default 而不是内存 SecretService | 对 headless Ubuntu 高概率稳定 |
| services::import_v2::transaction::tests::restart_recovery_preserves_same_content_external_new_replacement | unwrap_err 收到 Ok(()) | dev/inode 身份在删除重建后被复用；生产 fail-closed 缺口 | 文件系统相关但安全意义真实 |
| services::import_v2::transaction::tests::rollback_preserves_same_content_external_replacement_by_identity | unwrap_err 收到 Ok(()) | 同上 | 文件系统相关但安全意义真实 |
| services::lint_service::reports::tests::in_memory_project_key_changes_when_same_path_is_recreated | assert left != right，但两个 identity revision 相同 | 目录 dev/inode 重用；共享项目身份或测试契约问题 | 文件系统相关，需单独设计 |

### 5.3 Windows 失败清单

| 测试 | 真实错误 | 初步分类 | 稳定性判断 |
| --- | --- | --- | --- |
| services::agent_service::tests::streaming_process_enforces_real_cancel_timeout_nonzero_and_kill_on_limit | 非零退出用例预期 AGENT_EXIT_FAILED，实际 IMPORT_AGENT_TIMEOUT | 测试用 PowerShell cold start 承担非零退出语义；测试夹具错误 | hosted runner 负载相关 |

该测试的非零退出分支只需要证明“子进程快速返回 exit 7 被映射为 AGENT_EXIT_FAILED”，不需要 PowerShell。提高生产超时不能证明该语义。

### 5.4 macOS 失败清单

| 测试 | 真实错误 | 初步分类 | 稳定性判断 |
| --- | --- | --- | --- |
| app_state::project_registry_tests::external_ai_access_requires_current_project_trust | grant 后仍为 PROJECT_EXTERNAL_AI_REQUIRES_TRUST | 信任状态与 inspect_current health/path alias 组合问题 | 未证实稳定 |
| app_state::project_registry_tests::non_task_external_request_drains_before_project_revocation_returns | begin execution 返回 PROJECT_EXTERNAL_AI_REQUIRES_TRUST | 同上 | 未证实稳定 |
| app_state::project_registry_tests::project_execution_epoch_revocation_cancels_root_waits_and_unbinds_persistence | 创建写任务返回 PROJECT_WRITE_STATE_UNAVAILABLE | 健康/布局重验证 | 未证实稳定 |
| app_state::project_registry_tests::project_writes_require_trust_writable_health_and_a_specific_content_root | PROJECT_WRITE_STATE_UNAVAILABLE | 健康/布局重验证 | 未证实稳定 |
| services::chat_service::saved_answers::tests::save_answer_overwrite_requires_allow_flag_then_hash_matches | 预期 FILE_HASH_MISMATCH，实际 GIT_CHECKPOINT_FAILED | 校验顺序错误；无效 stale hash 不应先启动 Git checkpoint | 可通过受控故障稳定复现 |
| services::git_service::tests::app_owned_git_disables_hooks_fsmonitor_and_textconv | GIT_COMMAND_TIMEOUT；日志 args 为 rev-parse --short HEAD | Git policy scan 或真实命令阶段不透明 | 负载相关，不能称 flake |
| services::git_service::tests::blocked_project_git_lane_does_not_block_another_project | project B Git timeout | lane 测试与真实 Git 进程耦合 | 负载相关，待拆分证据 |

## 6. 反复修复的范围审计

| 提交 | 规模 | 评估 |
| --- | --- | --- |
| 6838ebda | 7 files, +235/-20 | 已扩展到 AppState 路径规范化、macOS 内存探测、Git 配置扫描、Agent 夹具等生产代码，超出单一 runner 测试修复 |
| 2cfe512c | 8 files, +118/-82 | 继续修改 Import commit/orchestrator、lint rules 和 safe_project_dir 的生产写入语义，范围进一步扩大 |
| 7bc7b3af | 4 files, +98/-11 | 相对收敛：markerless Git 短路与 Windows 进程树测试夹具；作为重做基准 |
| b464941a | 6 files, +16/-5 | CI 全局 test-threads=2，加 migration scanner symlink 元数据修改；三平台仍失败，不能作为正确方案直接复用 |

计划的范围控制原则：

- 能以测试构造修复的，不修改生产默认行为。
- 能在单一服务内部修复的，不扩散到 AppState、命令 DTO 或前端。
- 身份与安全修复必须明确序列化兼容和 fail-closed 语义，不能因 CI 而简化。
- Git 与 AppState 在获得分阶段证据前不改生产超时和全局锁模型。
- 每个问题组单独红灯、单独绿灯、单独审查 diff。

## 7. 修复成功的共同不变量

### 7.1 项目信任与写权限

- canonical path 注册不等于信任。
- external AI/Agent 必须同时满足当前信任和健康状态。
- 项目写入必须同时满足 trusted、writable、healthy 和具体内容根可用。
- 撤销必须等待已发布的外部执行退出，并阻止晚到持久化。
- 不同 canonical root 的写入和 Git lane 不能互相阻塞。

### 7.2 文件与身份

- symlink、junction、reparse point、hard-link race 和父目录替换继续 fail closed。
- 同内容的外部替换也必须被识别为外部对象，不能只比较内容 hash。
- 旧 recovery journal 不能因身份格式升级被静默误读。
- 不使用目录 ctime 作为长期项目身份，因为正常新增/删除子项会改变目录 ctime。
- 不通过在普通材料目录写隐藏 sentinel 来解决身份问题。

### 7.3 Git

- repository hooks、fsmonitor、diff.external、textconv、clean/process filters 和 includes 继续被禁用或拒绝。
- markerless 项目不得为了返回 non-repository 状态而启动 Git。
- 任何 policy scan 失败都必须阻止后续写入，不能降级为允许。
- cancellation、timeout、output limit 和进程树回收保持有效。

### 7.4 密钥

- 生产默认仍使用 OS credential storage。
- 单元测试使用 SecretService::memory 或显式 fake。
- 不以启动 D-Bus Secret Service、写假 HOME 或跳过 revoke 断言来让 Ubuntu 通过。

## 8. 总体执行顺序

1. 冻结隔离基线并记录工具版本。
2. 在 Windows 本机做所有可运行的聚焦基线与重复运行。
3. 先完成确定性最高、范围最小的测试夹具修复。
4. 分开处理 transaction 文件身份与 lint 项目身份，禁止把二者混成一个通用时间戳修复。
5. 修复 saved-answer 校验顺序，并保留写入前最终 CAS 校验。
6. 对 Git 超时增加分阶段证据，拆开 lane 逻辑与真实 Git 集成语义，再决定最小修复。
7. 对 AppState 失败增加 authority、layout、health 和 canonical root 的聚焦断言，再根据 macOS/Ubuntu 证据修复。
8. 运行所有聚焦回归。
9. 运行完整 npm run check。
10. 更新 Graphify、执行两轮 review、修复 review 发现，再从头运行 npm run check。
11. 小提交推送，等待同一 SHA 三平台完整结束。
12. 任一平台失败：先下载完整日志、更新证据表，再开始下一次修复。
13. 三平台全绿后记录 progress，按受保护流程合入 master。
14. master 绿后才开始发布阶段。

## 9. 阶段 0：安全基线与可复现记录

### 9.1 必做检查

~~~powershell
git status --short --branch
git rev-parse HEAD
git branch --show-current
git stash list
git worktree list --porcelain
node --version
npm --version
rustc --version
cargo --version
git --version
~~~

预期：

- 分支为 codex/rework-cross-platform-ci。
- HEAD 起始为 7bc7b3af。
- 主 worktree 与 stash 未变化。
- 只有明确归属于本任务的文档或 Graphify 查询辅助变更。

### 9.2 构建目录隔离

若已有 Tauri/Cargo 进程锁住默认 target，使用隔离目录，不停止用户正在运行的应用：

~~~powershell
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) ".target-ci-rework"
~~~

该目录保持未跟踪，不提交。

### 9.3 每次复现必须记录

| 字段 | 内容 |
| --- | --- |
| commit SHA | 精确 SHA |
| OS/runner | Windows 本机或 Actions runner image |
| test filter | 完整过滤器 |
| 并发 | 默认、test-threads=1 或诊断值 |
| 重复次数 | 通过/失败计数 |
| elapsed | 单次和总耗时 |
| error code | 精确 BackendError code |
| error details | 脱敏后的 stage/args/path 类型 |
| leftover | 是否留下子进程、锁、临时目录 |

不允许只记录“偶尔失败”。

## 10. 阶段 1：聚焦复现矩阵

### 10.1 列出精确 Rust 测试名

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features -- --list
~~~

将真实列表中的全名复制到执行记录，避免模块路径猜测。

### 10.2 单测独跑

以下命令使用函数名过滤；取得 --list 输出后改成精确全名和 --exact：

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features project_write_critical_sections_do_not_block_unrelated_roots -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features project_writes_require_trust_writable_health_and_a_specific_content_root -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features external_ai_access_requires_current_project_trust -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features non_task_external_request_drains_before_project_revocation_returns -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features project_execution_epoch_revocation_cancels_root_waits_and_unbinds_persistence -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features app_owned_git_disables_hooks_fsmonitor_and_textconv -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features blocked_project_git_lane_does_not_block_another_project -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features media_connector_uses_one_persistent_profile_per_platform -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features platform_revoke_removes_a_profile_without_a_live_session -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features refresh_executes_the_registered_route_instead_of_reusing_current_markdown -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features restart_recovery_preserves_same_content_external_new_replacement -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features rollback_preserves_same_content_external_replacement_by_identity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features in_memory_project_key_changes_when_same_path_is_recreated -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features save_answer_overwrite_requires_allow_flag_then_hash_matches -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features streaming_process_enforces_real_cancel_timeout_nonzero_and_kill_on_limit -- --nocapture
~~~

### 10.3 重复运行策略

- 测试夹具类：至少连续 20 次。
- 真实 Git/进程类：至少连续 10 次，并记录耗时分布。
- 身份删除重建类：至少连续 50 次或直到复现一次 inode 重用；不能用 sleep 作为通过条件。
- AppState 并发类：独跑 20 次，再与同模块测试默认并发运行 10 次。

诊断时可以比较：

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features TEST_FILTER -- --test-threads=1 --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features TEST_FILTER -- --test-threads=2 --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features TEST_FILTER -- --nocapture
~~~

这些参数只用于分类。不得因为 test-threads=1 通过就把整个 CI 固定为串行。

## 11. 工作流 A：Ubuntu Secret Service 依赖

### 11.1 证据

三个 Ubuntu 测试最终调用真实 SecretService::default，keyring 尝试连接 org.freedesktop.secrets。ConnectorSessionService 结构体内已有 secrets: SecretService；ImportV2Service 已提供 with_secret_service。

### 11.2 先红后绿方案

1. 保留或新增一个测试，证明 SecretService::memory 的 delete missing credential 返回 Ok，且不会访问 keyring。
2. connector_session.rs 同模块测试使用显式测试构造：
   - sessions/grants 仍为真实内存状态；
   - secrets 使用 SecretService::memory；
   - 不改变 ConnectorSessionService::default。
3. source_lifecycle_tests.rs 的 refresh 测试改用：
   - ImportV2Service::with_secret_service(SecretService::memory())；
   - 继续注册真实 RefreshFixtureEngine；
   - 继续断言真实 route 被执行，而不是复用旧 Markdown。
4. 单独运行三个失败测试至少 20 次。

### 11.3 允许的改动

- 测试模块内的 struct literal 或 crate-private 测试构造。
- 已存在的 with_secret_service 测试注入。
- 对内存 SecretService 行为的聚焦测试。

### 11.4 禁止的改动

- 把生产 Default 改成内存 secret。
- 在 SECRET_BACKEND_FAILED 时静默成功。
- 在 Ubuntu CI 启动临时 keyring 服务掩盖单元测试依赖。
- 删除 revoke 对 cookie credential 清理的断言。

### 11.5 退出条件

- 三个原失败测试在无 D-Bus Secret Service 环境下通过。
- 生产 SecretService::default 仍走 OS credential storage。
- secret_service 现有真实 keyring 错误映射测试不被删除或弱化。

## 12. 工作流 B：Windows 非零退出夹具

### 12.1 证据

失败发生在非零退出分支。测试用 PowerShell -NoProfile -Command exit 7，但 hosted runner 的 PowerShell 冷启动超过测试传入的 5 秒 max_runtime，导致测试实际测到 timeout。

### 12.2 修复方案

1. 保留 cancellation、25ms timeout、非法 UTF-8、output limit 和 descendant kill 的原断言。
2. 只把“快速非零退出”子用例改成轻量本机进程：
   - 首选 cmd.exe /D /S /C exit /B 7；
   - program 使用 SystemRoot 下的绝对路径或项目已有安全解析方式；
   - cwd 保持测试临时 workspace。
3. 若 cmd 路径解析本身有平台不确定性，复用当前 Rust test executable 的受控 fixture 模式；不得改回 shell cold start。
4. 增加断言，确认失败不是 IMPORT_AGENT_TIMEOUT 且退出码映射为 AGENT_EXIT_FAILED。
5. 连续运行完整 Windows 测试函数至少 20 次，确保其他进程树语义仍通过。

### 12.3 禁止的修复

- 把 5 秒改成 30 秒。
- 将 IMPORT_AGENT_TIMEOUT 也接受为成功。
- 删除 real cancel、kill-on-limit 或 descendant cleanup 子用例。

### 12.4 退出条件

- 非零退出分支不依赖 PowerShell startup。
- timeout 分支仍稳定返回 IMPORT_AGENT_TIMEOUT。
- cancel 和 output-limit 后标记文件不出现，证明进程树被回收。

## 13. 工作流 C：Import transaction 同内容外部替换

### 13.1 证据与风险

BoundFileIdentity 当前只保存两个 u64。Unix 是 dev + ino；Windows 是 volume serial + file index。Ubuntu 删除后快速重建同路径、同内容时可以复用 inode，导致 recovery/rollback 把外部新对象误认成事务自己安装的对象。

这是生产 fail-closed 问题，不是单纯错误测试假设。不能通过放宽 expect_err、改变测试内容或增加 sleep 解决。

### 13.2 设计约束

- transaction 身份用于短期所有权与 recovery journal。
- 不用 `ctime`、wall clock 或文件系统分配器是否复用 inode 作为所有权证明；这些值在平台、挂载类型和元数据操作之间没有足够稳定的契约。
- Unix 候选文件在安装前创建同 inode 的 hard-link identity pin；目标和 pin 的身份必须都从已绑定父目录下的 no-follow 对象读取，不能重新按不受约束的路径跟随。
- identity pin 使用既有 `recovery_artifacts` 字段持久化，不改变 journal schema，也不把 pin 冒充新的公开 identity 版本。
- pin 必须在 `InProgress` journal/安装动作可能被观察前创建并持久化；单文件事务同步一次 pin 父目录，cohort 按唯一父目录各同步一次，不能按条目制造 O(n) 重复 fsync。
- 单文件事务的第一次 durable journal 发布必须同时包含 intent 与当时已创建的 temp、原始 guard、candidate pin，不能依赖随后多次重写补齐 recovery artifact。
- restart reconciliation 必须从已绑定 journal 目录能力枚举条目；绑定后 lexical 目录被换名时仍处理原目录，不能把换成的空目录误判为“无需恢复”。
- `recovery_artifacts` 只接受严格 UUID 生成名，并必须绑定到同一父目录的 journal entry；存在的候选 artifact 校验 desired hash + installed identity，原始/delete guard 校验 previous hash。空 entries 携带 artifact、子串伪装或跨 entry 父目录一律在任何恢复 mutation 前 fail closed。
- live rollback、live commit 前验证和 `InProgress` restart recovery 都必须同时证明目标 identity 与 pin identity 匹配；pin 缺失、被替换、不可读或指向其他 inode 均返回 conflict/manual recovery，并保留目标与冲突证据。
- durable `Committed` marker 是 point of no return。marker 写入后，pin 或 journal 的清理失败不能把已经提交的写入伪报为普通失败；必须保留 committed journal，供下一次 reconciliation 验证目标后继续幂等清理。
- 旧 `InProgress` Unix journal 没有 pin 时证据不足，必须 fail closed；旧 `Committed` journal 可以在验证已提交目标后清理，因为它可能正处于 marker 已持久化、pin 已删、journal 尚未删的合法崩溃窗口。
- Windows 继续使用从打开 handle 获得的 volume serial + file index 绑定；本次 hard-link pin 只补强会复用 inode 的 Unix ownership 缺口，不扩大 Windows 持久化格式。
- 若项目文件系统不支持 hard link、跨设备链接或拒绝创建 pin，返回明确的 `IMPORT_V2_IDENTITY_PIN_UNAVAILABLE`；不降级为 hash-only、sleep、路径字符串或允许覆盖。

### 13.3 先红后绿步骤

1. 保留两个真实文件系统集成测试：
   - rollback_preserves_same_content_external_replacement_by_identity；
   - restart_recovery_preserves_same_content_external_new_replacement。
2. 新增 pin 生命周期测试，证明安装后的目标和 pin 是同一对象，且 pin 路径进入 durable `recovery_artifacts`，只在 durable commit 之后清理。
3. 新增 live rollback/commit 负向测试：删除 pin 或用普通文件重绑 pin 后，事务必须返回 conflict，且外部目标字节不变。
4. 新增 restart recovery 负向测试：`InProgress` journal 的 pin 缺失或被重绑时不得按 hash + dev/inode 猜测 ownership。
5. 新增 durable marker 清理故障测试：
   - marker 后 pin cleanup 失败仍返回成功并保留 committed journal；
   - marker 后 journal delete 失败仍返回成功；
   - 后续 reconciliation 验证目标后移除残留 pin/journal。
6. 新增 cohort durability 测试，统计真实执行的 pin-parent sync 次数，确保同一父目录只同步一次；Windows 不执行 Unix pin sync。
7. 保留旧 journal 反序列化兼容测试，并分别验证旧 `InProgress` fail closed 与旧/残留 `Committed` 幂等清理语义。
8. 运行 transaction 全模块测试、project mutation binding contract，并在 Ubuntu/macOS runner 执行全部 `#[cfg(unix)]` pin 回归。

### 13.4 决策门

已批准采用 schema-preserving hard-link identity pin：复用 `recovery_artifacts`，不新增 V2 identity 字段。若实现过程中发现必须改变公开/长期持久化 schema、允许不支持 hard link 的文件系统自动降级，或需要改变 durable marker 的提交语义，立即暂停并重新请求用户决策，不夹带进 CI 修复。

### 13.5 禁止的修复

- 增加 sleep 等待 inode 改变。
- 只比较内容 hash。
- 删除 identity 断言。
- 在 identity 不可用时自动覆盖或删除文件。
- 用 path canonicalization 代替已打开对象身份。
- pin 创建失败时退回 hash-only、inode-only 或 best-effort commit。
- durable marker 已成功后，把纯清理失败向调用者报告成“事务未提交”。

## 14. 工作流 D：Lint in-memory project key

### 14.1 为什么必须与 transaction 分开

memory_project_key 复用 workflow_service::project_identity。Unix identity revision 目前也是 dev + ino。transaction 的短期文件所有权由候选 hard-link pin 证明，而项目根 identity 是跨正常子项增删长期稳定的命名空间；两者生命周期和失败语义不同，不能把 transaction 的 pin 方案或目录 ctime 直接塞进共享 project_identity，否则会旋转 workflow identity、取消任务或丢失持久化绑定。

### 14.2 调查步骤

1. 列出 project_identity 的所有调用者，区分：
   - workflow authority/persistence；
   - lint 内存报告；
   - 仅诊断用途。
2. 验证各平台稳定 creation/birth time：
   - Windows creation time；
   - macOS birthtime；
   - Linux statx btime 的实际 runner 可用性。
3. 验证正常创建、修改、删除 wiki 子文件不会改变候选 root identity。
4. 验证删除整个 root 并在相同路径重建会改变候选 root identity。
5. 若 Linux 文件系统不提供稳定 birthtime，评估 scoped lint memory generation registry，而不是污染共享 workflow identity。

### 14.3 测试要求

- 删除 std::thread::sleep(10ms) 依赖。
- 新测试必须证明：
  - 普通项目内容变动，memory key 不变；
  - root 被替换，memory key 改变；
  - canonical alias 指向同一 root 时 key 相同；
  - Windows 大小写/8.3 与 macOS /var alias 不产生假替换。

### 14.4 决策门

若唯一可行方案会改变 Workflow identity revision 的跨版本语义，先请求用户确认。优先采用只影响 lint 内存报告的 scoped 修复。

## 15. 工作流 E：Saved answer hash 与 Git checkpoint 顺序

### 15.1 证据

save_answer_to_wiki 当前在 allow_overwrite=true 时：

1. 取得 expected_hash；
2. 先 create_scoped_checkpoint；
3. 再通过 WriteMode::OverwriteIfHashMatches 让 FileStore 校验 hash。

因此 stale hash 本应立即返回 FILE_HASH_MISMATCH，却可能先被慢或失败的 Git checkpoint 遮蔽。

### 15.2 正确顺序

1. 解析并校验目标路径。
2. 读取当前 hash。
3. stale expected hash 立即返回 FILE_HASH_MISMATCH，并附现有 baseline details。
4. hash 匹配后创建 scoped checkpoint。
5. checkpoint 成功后重新解析安全写入根。
6. 最终写入仍使用 OverwriteIfHashMatches(expected)，防止 checkpoint 期间发生外部编辑。
7. 最终 CAS 失败必须返回 FILE_HASH_MISMATCH，且不能写入。

预检不是最终授权；最终 CAS 和路径重验证不能删除。

### 15.3 回归测试

1. stale hash + 不可用 Git：
   - 返回 FILE_HASH_MISMATCH；
   - 文件字节不变；
   - 不要求 checkpoint 成功。
2. matching hash + checkpoint failure：
   - 返回 GIT_CHECKPOINT_FAILED；
   - 文件字节不变。
3. matching hash + checkpoint success：
   - checkpoint hash 存在；
   - 新内容落盘。
4. checkpoint 后并发编辑：
   - 最终 CAS 返回 FILE_HASH_MISMATCH；
   - 不覆盖外部编辑。

### 15.4 禁止的修复

- 在 valid overwrite 时跳过 checkpoint。
- 把 checkpoint failure 降级为 warning。
- 只做预检、不做写入前 CAS。
- 因 macOS Git 慢而扩大 checkpoint timeout。

## 16. 工作流 F：Git policy scan、真实进程与 lane 隔离

### 16.1 当前证据不足点

GIT_COMMAND_TIMEOUT 的 details 只记录调用者 args。run_git_process 在真正执行该 args 之前还会：

1. 获取 per-project lane；
2. 执行 reject_local_git_filters；
3. 运行 hardened Git command。

因此日志中的 status 或 rev-parse 不能证明真正超时的一定是最终命令。必须先得到 stage 证据。

### 16.2 第一轮：只增强可诊断性

在不泄露环境变量、Git 配置值或路径内容的前提下，为测试/错误 details 增加：

- stage：lane_wait、policy_scan、git_command、stdout_join 或 process_reap；
- elapsed_ms；
- remaining_budget_ms；
- command kind，仅记录现有安全 args；
- cancellation 是否触发；
- child 是否完成回收。

若生产错误 details 的兼容风险较高，可先使用 cfg(test) hook/trace collector，在测试失败信息中输出相同数据。

### 16.3 拆分测试责任

blocked_project_git_lane_does_not_block_another_project 同时测试 lane map 和真实 Git 可启动性，导致项目 B 的进程调度慢被误报为 lane 串线。拆成：

1. 确定性 lane 单元测试：
   - 持有 root A lane；
   - root B lane 必须立即可获取；
   - 同 root A 的第二获取保持阻塞；
   - 不启动 Git。
2. 真实 Git 集成测试：
   - 两个不同 repository 分别运行受控轻量命令；
   - 不人为持有另一 lane；
   - 验证 hardened runner 正常完成。

拆分后仍必须保留 task cancellation interrupt lane wait 的现有测试。

### 16.4 hardened Git 测试

app_owned_git_disables_hooks_fsmonitor_and_textconv 必须继续使用真实 Git，因为它验证恶意 hook/fsmonitor/textconv 不执行。允许优化测试 fixture，但不能替换为只检查参数字符串。

调查顺序：

1. 测量 fixture setup、policy scan、status/commit、rev-parse、diff 的分段耗时。
2. 确认 run_git_in 是否继承 hosted runner 的 global signing/hooks/config；测试 setup 应使用最小、非交互、无签名环境。
3. 确认 bounded process 在 timeout 后终止并 join stdout/stderr reader。
4. 审计同测试进程中其他长寿命子进程是否在 Drop/cancel 后残留。
5. 检查 policy scan 是否每个 Git 命令重复执行且是否存在无界成本；任何缓存方案都必须能感知 .git/config 与 config.worktree 变化，并继续 fail closed。
6. 只有分段证据证明默认 timeout 低于正常操作的合理上界时，才讨论局部预算；不得全局盲目扩大。

### 16.5 负向回归

- post-commit hook 不创建 marker。
- core.fsmonitor helper 不创建 marker。
- diff.external/textconv 不创建 marker。
- local filter、process filter、include、includeIf、config.worktree 仍被拒绝。
- markerless root 不 spawn。
- invalid .git marker 仍 fail closed。
- timeout/cancel 后无残留 child。
- root A lane 不阻塞 root B。

### 16.6 退出条件

- 聚焦真实 Git 测试连续 10 次通过。
- 每次耗时有记录，不能只看最终通过。
- 远端 macOS 与 Ubuntu 不再出现 GIT_COMMAND_TIMEOUT。
- 安全负向测试全部保留。

## 17. 工作流 G：AppState / ProjectRegistry health 与 trust

### 17.1 当前可能链路

require_external_ai_access 在 with_workflow_access 内调用 project_assessment_service.inspect_current，并要求 access.trust=Trusted 且 health=Healthy。

with_current_project_write_access 会：

1. 进入 update mutation barrier；
2. 按 asserted root 获取 project trust transition lane；
3. 从 ProjectRegistry 重新 resolve；
4. with_resolved_layout；
5. resolve_workflow_access_locked；
6. validate_project_write_access；
7. 持锁执行 operation。

远端错误只能证明其中的 health/trust 不满足，尚不能证明是 runner flake或生产锁错误。

### 17.2 先增加聚焦事实断言

在失败测试每个关键阶段检查并输出脱敏状态：

- requested root；
- canonical root；
- registry authority root；
- trust enum；
- filesystem access enum；
- persistence enum；
- project health；
- layout kind；
- required roots 是否存在；
- authority revision 是否意外旋转。

不输出项目内容。

### 17.3 定向假设与证伪

假设 A：macOS /var 与 /private/var alias 导致 registry、lane 和 assessment 使用不同 key。

- 证据要求：同一 temp root 的 requested/canonical/authority root 不同。
- 回归：alias 与 canonical path 必须映射同一 lane 和 authority。
- 若根值完全一致，则排除该假设，不修改 normalization。

假设 B：项目健康检查受 Git 子进程超时污染。

- 证据要求：assessment details 明确记录 Git probe failure/timeout。
- 验证 markerless strict-native fixture 是否仍启动 Git。
- 若 markerless fixture零 Git attempt，则排除。

假设 C：测试 fixture 缺少当前 strict-native 必需布局。

- 对照 ProjectLayout 当前要求逐项断言 purpose/schema/raw/sources/wiki/index/.app/tasks/exports/skills。
- 若缺项，修测试 fixture；不放宽生产健康判定。

假设 D：并行测试共享全局 config、registry lane 或固定临时路径。

- 检查 state_with_temp_config 是否 UUID 隔离。
- 检查所有 static map key 是否 canonical root，而不是 project_id 或非 canonical alias。
- 用 barrier/channel 重现，不使用 sleep。

### 17.4 回归矩阵

- strict native 注册后 external access 成功。
- compatible vault grant 前失败、grant 后成功。
- unhealthy native 即使历史上 trusted 也禁止 external/write。
- root A write critical section 不阻塞 root B。
- revoke 阻塞直到 root A external lease drop。
- root B task 不被 root A revoke 取消。
- macOS alias、Windows 8.3/大小写和 Linux symlink escape 保持现有安全语义。

### 17.5 禁止的修复

- health 不明时视为 Healthy。
- trust record 找不到时沿用旧 context。
- 将所有 project transition 变成单一全局锁。
- 删除 revoke wait 或晚到持久化阻断。
- 增加 15 秒 channel timeout 以掩盖前置 access failure。

## 18. 工作流 H：CI workflow

### 18.1 基线

7bc7b3af 的 Rust 步骤为：

~~~yaml
- name: Run Rust service tests
  run: cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
~~~

b464941a 改为全局 test-threads=2 后仍有 18 个跨平台失败，因此本重做分支不直接继承该改动。

### 18.2 修改条件

只有出现以下证据之一才修改 ci.yml：

- 测试明确需要系统服务，而 CI 漏装的是产品声明的真实运行依赖；或
- Rust 官方/平台工具链需要显式环境配置；或
- 受控资源测试有已定义、可解释的局部资源调度入口。

不因为“较慢”就全局限制线程。若最终确需并发上限，必须同时证明：

- 它符合应用测试的资源模型；
- 不隐藏共享状态；
- 默认并发重复运行也不再产生错误结果；
- ci-contracts 测试同步更新。

## 19. 建议的实现与提交顺序

每个提交保持单一解释。测试可以先在工作区形成红灯，但推送提交应包含对应修复并通过聚焦测试。

建议顺序：

1. test: isolate CI tests from OS keyring
   - connector session test construction
   - source lifecycle memory SecretService
   - 不改生产 Default

2. test: use deterministic Windows nonzero process fixture
   - 只处理非零退出夹具
   - 保留完整进程树测试

3. fix: pin Unix transaction candidate identity across commit/recovery
   - hard-link pin lifecycle、missing/rebound pin、committed cleanup fault 和 legacy journal 测试
   - transaction 内 schema-preserving、fail-closed 最小生产修复

4. fix: keep lint memory reports scoped to a root instance
   - 只有独立设计证据完成后提交
   - 不与 transaction identity 混提

5. fix: validate saved-answer hash before Git checkpoint
   - stale hash short-circuit
   - final CAS 与 checkpoint 负向测试

6. test/fix: separate Git lane invariants from process availability
   - stage diagnostics
   - lane unit test与真实 Git 集成测试
   - 根据证据决定最小生产修复

7. test/fix: stabilize current project health revalidation
   - 先补事实断言
   - 只修已证实路径/fixture/共享状态问题

8. chore: refresh focused CI evidence
   - progress.txt
   - 仅必要 gotchas
   - 相关 Graphify

如果调查证明若干失败有同一生产根因，可以合并修复，但提交说明必须列出共享不变量和所有回归测试，不能仅以“CI stabilization”概括。

## 20. 聚焦验证矩阵

### 20.1 每个工作流最低门槛

| 工作流 | 聚焦测试 | 邻接测试 |
| --- | --- | --- |
| Secret Service | 三个原失败测试、secret memory delete | connector_session 全模块、source_lifecycle tests |
| Windows process | 完整 streaming_process 函数重复 20 次 | agent_service process/output/cancel tests |
| transaction identity | 两个原失败测试、pin lifecycle/missing/rebound/cleanup fault、legacy journal | transaction 全模块、project_mutation_binding_contracts、Ubuntu/macOS Unix pin 回归 |
| lint project key | replace/ordinary mutation/alias | lint reports 全模块、workflow identity tests |
| saved answer | stale/matching/race/checkpoint failure | chat saved_answers、file_store CAS、Git checkpoint |
| Git | hardened config、lane、cancel、markerless | git_service 全模块、checkpoint consumers |
| AppState | 五个远端失败测试 | project registry、workflow access、trust revoke tests |

### 20.2 中间检查

测试夹具和局部代码每组完成后：

~~~powershell
npm run check:quick
~~~

若改动触及 filesystem mutation、Git、secret、IPC、concurrency、background task 或 authority，check:quick 只用于中间反馈，不能作为交付结论。

### 20.3 最终本地门槛

从隔离 worktree 根目录运行：

~~~powershell
npm run check
~~~

若失败：

1. 判断是否由本次 scoped change 引起；
2. 修复；
3. 从头重新运行 npm run check；
4. 不从失败步骤续跑后声称完整通过。

最终记录：

- 前端测试数；
- Rust library 与 integration 测试数；
- ignored 数及原因；
- lint/build/console/GUI compile；
- elapsed；
- commit SHA；
- 是否使用隔离 CARGO_TARGET_DIR。

## 21. Graphify 更新

代码修改完成后：

~~~powershell
graphify update .
git status --short
git diff -- graphify-out
~~~

审查原则：

- 只保留由本次代码/测试变化产生的 AST 和关系更新。
- .vocab.txt、last_query_stamp、memory query 等纯查询噪声不作为 CI 修复证据。
- 不因 graphify-out 已脏而跳过 update。
- 不把主 worktree 的既有 Graphify 文件复制到隔离 worktree。

## 22. 两轮独立 review

### 22.1 Review A：共享上下文

审查目标：

- 修复是否符合产品信任、写权限、Git checkpoint 和文件身份设计；
- 是否与 SPEC 第 16 节、Runbook、progress/gotchas 一致；
- 是否扩大到无关产品代码；
- 测试是否真正覆盖远端错误；
- 是否保持 Windows/macOS/Linux 对等。

重点文件：

- src-tauri/src/app_state.rs
- src-tauri/src/services/git_service.rs
- src-tauri/src/services/agent_service.rs
- src-tauri/src/services/secret_service.rs
- src-tauri/src/services/import_v2/
- src-tauri/src/services/lint_service/
- src-tauri/src/services/chat_service/saved_answers.rs
- src-tauri/src/utils/safe_project_dir.rs
- .github/workflows/ci.yml

### 22.2 Review B：fresh context

不给既有根因结论，只提供：

- 用户安全边界；
- 原始 diff；
- 新增测试；
- 三平台失败日志摘要。

要求寻找：

- 被测试掩盖的生产回归；
- journal/serialization 兼容问题；
- symlink/junction/hard-link 绕过；
- Git filter/include 逃逸；
- child process 泄漏；
- normal project mutation 导致 identity 错误旋转；
- 测试仍依赖 scheduler、wall clock 或真实 keyring。

### 22.3 Review 关闭规则

- 主代理逐项验证 reviewer 发现，不机械照单全收。
- 修复所有有效 P1/P2。
- 若 reviewer 对 identity schema 或 authority 语义有分歧，暂停并提交决策材料给用户。
- review 修复后从头运行 npm run check。

## 23. 远端 CI 操作与等待

### 23.1 推送

~~~powershell
git status --short
git log --oneline --decorate -n 10
git push -u origin codex/rework-cross-platform-ci
~~~

禁止 --force 和 --force-with-lease。

### 23.2 绑定同一 SHA

~~~powershell
$sha = git rev-parse HEAD
gh run list --repo StoneLL1/llm-wiki-desktop --branch codex/rework-cross-platform-ci --commit $sha
~~~

等待该 SHA 对应的完整 CI run。不能把同分支不同 SHA 的三个 Job 拼接为“全绿”。

### 23.3 失败处理

任一 Job 失败：

1. 记录 run URL、SHA、Job ID、runner image。
2. 下载完整失败日志。
3. 列出精确失败测试、error code、耗时和 stage。
4. 更新本文证据或 progress 中的 Pending 事实。
5. 做聚焦复现与最小修复。
6. 新提交、新 SHA、重新等待三个平台。

不使用“rerun failed jobs”把一次偶然绿替代根因修复；若为验证稳定性主动 rerun，必须保留原运行和 rerun 两份证据。

### 23.4 全绿证据格式

~~~text
Commit:
Run URL:
Windows job ID / conclusion / elapsed:
macOS job ID / conclusion / elapsed:
Ubuntu job ID / conclusion / elapsed:
All jobs refer to same SHA: yes/no
Reruns required: count and reason
~~~

## 24. progress.txt 与 gotchas.txt

### 24.1 progress

重要里程碑在 progress.txt 顶部新增一行并保留全部历史：

- 详尽计划落地；
- 每个生产根因修复完成；
- 本地完整 npm run check 通过；
- 两轮 review 关闭；
- 三平台同 SHA 全绿；
- 合入 master；
- Draft/安装验收里程碑。

不能把 Pending 的远端、签名或安装证据写成完成。

### 24.2 gotchas

只有出现新的、可复发、非显而易见陷阱才新增。以下已存在，不重复记录：

- markerless project 不应 spawn Git；
- PowerShell cold startup 不适合作为 bounded process fixture；
- macOS /var 与 /private/var descriptor path；
- busy-wait race 测试；
- Git config.worktree 与 include/filter 审计；
- Windows 8.3 alias。

## 25. 合入 master 的门槛

以下全部满足才允许进入合入步骤：

- 工作分支基于 7bc7b3af 的可解释提交；
- 所有聚焦测试通过；
- npm run check 从头通过；
- 两轮 review 关闭有效 P1/P2；
- Graphify 已更新且无无关产物混入；
- 同一 SHA 的 Windows、macOS、Ubuntu Job 全部 success；
- progress.txt 已记录真实 URL、SHA、Job 结论；
- stash 和主 worktree 未受影响；
- 没有 force push。

合入方式遵循仓库 branch protection。若需要 PR、required review 或管理员批准，停止并向用户列出 GitHub 中待完成的动作，不绕过保护。

合入后必须验证 origin/master 的 SHA 确实包含已验证提交；若 merge commit 改变 SHA，必须等待 master merge SHA 自己的三平台 CI 全绿，不能沿用分支 SHA 结论。

## 26. 阶段 B：首次发布准备的进入条件

只有以下条件全部为真才启动：

1. 修复已合入 master。
2. master 合入 SHA 的 Windows、macOS、Ubuntu CI 全绿。
3. 仓库、tag、版本、release notes、known limitations 已重新验证。
4. 用户没有要求暂停发布准备。

进入后按顺序运行：

~~~powershell
npm run check:release-config
npm run check:release-config:local
npm run test:final-four-redlines
npm run check
~~~

然后只核对 GitHub Environments 和 secret 名称是否配置，不读取值：

- capability-release
- desktop-release
- required reviewers
- 最小权限
- key owner
- backup custodian
- offline recovery evidence

缺少任何 owner、custodian、reviewer、证书或离线恢复证据时停止，并让用户在 GitHub/安全存储中配置。不得要求把私钥粘贴到聊天或仓库。

Draft 候选必须验证 Windows x64、macOS arm64、macOS x64、Linux x64、20 项 capability catalog、updater signatures、Authenticode、Developer ID/notarization/staple、latest.json 四平台、checksums、SBOM、provenance 和 attestation。

随后进行旧签名版本到候选版本的真实安装、升级、启动、卸载和用户项目字节不变验证。

即使 Draft 候选全部通过，也只能生成 Go/No-Go 报告并单独询问用户是否批准 stable 发布。

## 27. 停止并请求用户决策的条件

出现以下任一情况，不自行扩大范围：

- transaction identity 需要不兼容 journal schema。
- project identity 修复会改变正常项目 mutation 的 workflow revision。
- 需要修改公开 DTO、持久化格式或 release contract。
- 需要降低路径、trust、write、Git、process 或 credential fail-closed 语义。
- 需要 GitHub 重新认证、管理员权限、environment reviewer 或 branch protection 操作。
- 需要生产证书、私钥、密码、owner/custodian 决策。
- 需要创建 stable tag 或发布 stable Release。
- 主 worktree 或隔离 worktree 出现无法归属的并发修改。

请求决策时必须给出：

- 已观察事实；
- 至少两个可选方案；
- 安全和兼容代价；
- 推荐方案；
- 不决策时可以继续做的安全工作。

## 28. 阶段 A Definition of Done

阶段 A 只有在以下检查全部勾选后完成：

- [ ] 所有 18 个远端失败均有精确根因分类。
- [ ] 测试构造问题已使用 deterministic fixture 修复。
- [ ] 生产问题有先红后绿回归。
- [ ] transaction replacement 对 same-content 外部对象 fail closed。
- [ ] lint/project identity 不因正常内容写入错误旋转。
- [ ] saved answer stale hash 不先依赖 Git，但合法覆盖仍有 checkpoint 和最终 CAS。
- [ ] Git hooks/fsmonitor/textconv/filter/include 防护未弱化。
- [ ] Git timeout/cancel 后子进程完整回收。
- [ ] AppState trust/health/write/revoke 不变量保持。
- [ ] 未使用全局 sleep、盲目 timeout、全局 serial 或 swallowed error。
- [ ] 所有聚焦测试通过。
- [ ] npm run check 从头通过。
- [ ] Graphify 更新完成且 diff 已筛选。
- [ ] 两轮独立 review 关闭。
- [ ] 新分支已普通 push，无 force。
- [ ] 同一 SHA 的 Windows Job success。
- [ ] 同一 SHA 的 macOS Job success。
- [ ] 同一 SHA 的 Ubuntu Job success。
- [ ] progress.txt 已记录 run URL、SHA、三个 Job。
- [ ] 受保护流程合入 master。
- [ ] master 合入 SHA 的三平台 CI 全绿。

在最后一项之前，不进入发布 Draft 执行。
