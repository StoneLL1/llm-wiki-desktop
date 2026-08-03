# First-run / Project-open Workbench 实现审计

## Latest adjudication — 2026-08-03, round 15

**Release decision: No-go.** The first-run/project-open product flow is substantially implemented and the descendant-link read policy is now covered by concrete Rust code. This round also closed the P1 write-path bypasses found during review. It does **not** establish descriptor/handle-anchored, cross-process atomic no-follow mutation; that remains a release blocker under the authority design's §8.4/§12 link policy.

### Scope and evidence

| Design obligation | Current implementation evidence | Audit result |
| --- | --- | --- |
| No-project workbench, create-to-Import, typed open/assessment and restricted/trusted/read-only routing | App shell, project assessment, project service and frontend integration already have regression coverage recorded below. | Implemented; end-to-end acceptance matrix is still incomplete. |
| Contained descendant links may be read once; escapes, loops and sensitive targets stay hidden | `ProjectLayout::list_markdown_files`, quick readability and cancellable inventory canonicalize contained targets, de-duplicate physical paths and reject external, root-loop and sensitive targets. | Implemented for read/index paths. |
| A read-visible linked page must never become a writable page | `ProjectContext::resolve_wiki_write_path` constrains mutations to the configured writable wiki root, while the generic write resolver rejects linked parents. Search page save/create/rename/delete, Chat saved answers, Lint fixes and Compile output/delete/rollback now preflight this semantic root. | P1 closed in code; full gate passed. |
| App-state cache/log side effects obey the same boundary | Graph cache reads/writes, Search deletion/cache invalidation, Chat cache/log/session deletion and Lint cache/log handling use strict project/wiki write resolution or safely do nothing when the side-effect path is unsafe. | P1 closed in code; full gate passed. |
| Original sources remain immutable | Compile deletion/output and its backup/rollback paths now use the writable wiki root for `wiki/*`, so a `wiki/linked` junction cannot target `raw/sources`. | P1 closed in code; full gate passed. |

### Review findings and disposition

Two independent review passes found no P0. The following P1 findings were corrected in the working tree:

1. A confirmed Workflow deletion could traverse an internal junction such as `wiki/linked -> raw/sources` and remove the original source. Compile preflight, apply, deletion and recovery paths now resolve `wiki/*` through `resolve_wiki_write_path`.
2. Graph cache, page-delete cache invalidation, Chat saved-answer cache/log updates, and Lint cache/log updates used direct `.app` or `wiki` paths. They now resolve strict write targets first.
3. A physical canonical path returned by read-only Markdown discovery could otherwise be supplied to a page mutation. Search, Chat and Lint now require the configured writable wiki root, not merely a project-contained path.
4. Chat session deletion and Workflow Generate Content's compensating artifact cleanup still deleted a read-resolved path. Both now require strict project write resolution before deletion.

The following P2 items have been rechecked; only the explicit external-link UI and the product acceptance matrix remain open:

1. `ProjectService::scan_project` and cancellable inventory now derive their roots from the resolved `ProjectLayout` roles instead of hard-coded `wiki`, `raw/sources` and `.app/tasks` paths. The role-aware implementation preserves native source/task semantics, compatible root/page/source semantics, cancellation/progress and no-follow inventory walking. A compatible root-Markdown + `pages/` + `sources/` fixture proves summary and inventory counts agree with the resolved layout.
2. The bounded assessment reader rejects a sensitive target reached through a linked ancestor. The Windows regression `raw -> .app` is now included in the full suite and passed. Broader macOS/Linux link-matrix coverage still belongs to the release acceptance work.
3. The project tree still has no explicit presentation for an external descendant link that is visible as a warning but excluded from indexing. This remains a product UX gap, not permission to read it.

### Verification status for this round

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` — passed.
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` — passed.
- Final `npm run check` — passed from the beginning in 307.8 seconds after the role-aware inventory and linked-chat-state regressions: Import/Source evidence, GUI Rust check, Vitest **117 files / 856 tests**, capability tools, lint, production build, console scan, Rust **917 tests / 0 failures**, integration tests and doc tests all passed.
- The Windows linked-chat-state regression creates a junction without requiring Developer Mode; it proves that `.app/chats -> raw/sources` is rejected before session deletion and the original source bytes remain unchanged.

### Non-negotiable release blockers

1. Path-based canonicalization and revalidation leave a TOCTOU window if an unrelated process swaps a directory/file after validation. Use directory handles/file descriptors with no-follow semantics for every atomic mutation before release.
2. Complete the §14 acceptance matrix, including Windows junction, macOS/Linux symlink, CJK/Unicode and collision cases; unit coverage of adjacent helpers is insufficient.
3. Surface excluded external descendant links as warnings without reading target content, and complete the authoritative acceptance matrix with recorded platform evidence.

> 审计日期：2026-08-03  
> 审计类型：产品设计到代码的实现一致性审计  
> 审计状态：初始审计完成；已附加 2026-08-03 实施复核  
> 权威设计：[`../superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)  
> 配套执行计划：[`../superpowers/plans/2026-08-03-first-run-project-open-workbench-remediation.md`](../superpowers/plans/2026-08-03-first-run-project-open-workbench-remediation.md)

---

## Implementation review update — 2026-08-03 round 14

The implementation now honors the read side of the authority document's descendant-link rule in the real Markdown-index enumeration path. Canonical containment is verified before a linked directory or file is traversed, then only the canonical physical path is used for continued discovery and caller output. This provides file/directory de-duplication, stable path selection, and loop protection. An external target, a link back to the project root, and app/runtime-sensitive targets remain excluded. This change does not relax any write API.

The regression fixture exercises an internal junction, an external junction, a junction into .app, and a loop junction. It is mandatory on Windows and uses mklink /J, so missing symlink privilege cannot turn the test into a false pass. Quick assessment and the cancellable Markdown inventory now apply the same limited internal-link policy, while source/task inventory stays no-follow. The strict write-path resolver prevents an edit, rename, or delete from using a logical linked alias. Layout regression (10/10), new assessment/inventory junction regressions, formatting, and npm run check:quick passed. The audit remains **No-go**: external descendant-link presentation is absent, and the compatible-guidance/graph-repair write routes still lack a cross-process, handle-anchored atomic no-follow implementation.

## 2026-08-03 实施复核（第十三轮）

本轮补齐了此前不能运行的前端验证，并修正了由新后台盘点命令和 no-project workbench 迁移暴露出的测试契约。`start_project_inventory` 继续保持 fire-and-forget，但改为以 `Promise.resolve` 包装 IPC 返回值；这使受限宿主或最小测试 shim 未实现该后台命令时不会因访问 `undefined.catch` 中断项目打开。Store 测试改为按 command 精确 Mock，而不再依赖已不成立的调用顺序；自动盘点被纳入项目打开、重新定位与启动重开后的预期。App 测试还会在每例开始时关闭 Settings、清除 workspace focus，避免一个用例遗留的模态遮挡随后 keyboard/focus 行为。界面文案已从过时的 “Back to launch” 改为 “Back to workspace / 返回工作区”，与“无项目工作台不是独立启动页”的权威设计保持一致。

实际 Windows runner 验证已可运行：三组受影响前端测试为 **58/58**，完整 Vitest 为 **117 文件、856/856 用例**；当前文件上的 `npm run check:quick` 通过（ESLint、`cargo check --no-default-features`、`tsc -b`、Vite production build、console scan）。因此，历史记录中 sandbox `spawn EPERM` / Tailwind native-module 问题只应解释为受限 sandbox 的运行条件，**不再是前端实现或测试缺失的结论**。

完整 `npm run check` 已从头通过（full mode，4 分 56 秒）：Import/Source evidence、GUI Rust `cargo check`、Vitest、capability tools、lint、Vite build、console scan 以及 no-default-features Rust 全量测试均通过；后者运行 **906 个测试，0 失败**。本轮第一次 full-gate 尝试曾在 `capability_release` 链接时收到一次 `link.exe` unexpected error，随后单独完整复跑成功，再次从头执行的 full gate 也成功。因此它应记录为一次瞬态链接故障，而不是未关闭的工具链或业务阻塞；最终质量证据以这次全绿 full gate 为准。

**当前发布判断仍为 No-go。** 已实现的 native recent relocation、普通资料夹保护、无项目 AppShell、create-to-Import、typed assessment/authority、后台盘点和 graph-cache 受限修复均有代码与单元/定向回归证据，且全量质量门已通过；但 descriptor/handle 级的跨进程 atomic no-follow 尚未实现，除 graph cache 外的损坏 `.app/*.json` 仍故意保持 read-only Recovery，权威规范的 17 场景尚未完成端到端验收。后续实施与发布报告必须保留这些产品与安全边界。

## 2026-08-03 实施复核（第十二轮）

第十一轮留下的 native recent-project 重新定位现已实现，但范围严格限制为**带 app-owned durable identity 的原生知识库**。新建原生库在 `.app/project.json` 写入 UUID；随后 ordinary native open 会优先复用该 UUID。对缺失 recent 条目，用户显式选择候选目录后仍先完成只读 assessment；后端只接受 `native_current` 且可读取的目录，安全读取候选的 `.app/project.json`，并要求 UUID 与旧 recent 的 `projectId` 完全一致。compatible 和 legacy Markdown 目录没有可移动、可证明的身份，继续只能通过显式打开新位置并按需移除旧记录，不能自动合并。

身份校验成功后，`relocate_recent_project` 在同进程 recent mutex 与 config-dir OS-backed mutation file lock 内再次安全读取 candidate UUID，并要求旧 root 的 `symlink_metadata` 明确为 `NotFound`；权限或其他 I/O 错误一律拒绝。随后才以 `projectId + 规范化旧 rootPath` 查找唯一全局 recent 条目，拒绝不存在条目和被其他条目占用的目标路径，再以原子写替换为新条目。remember/remove/relocate 共用这两层锁，因此 cooperating 的多个 LLM Wiki 进程不会从同一旧快照互相覆盖；重新定位不会写入、扫描、删除或初始化任何既有项目目录。registry 保持写锁期间先执行该 global commit，只有 commit 成功才将精确 old root 重绑定为已验证的新 native root 并更新 authority revision；任何 UUID、旧根、recent 或落盘失败均保持 registry 旧值，避免 session authority 与持久 recent 分裂。

Windows 上 `canonicalize()` 生成的 `\\?\` 设备路径前缀会使普通 recent 路径与注册 root 看似不同。本轮在 registry 与 recent 更新中统一移除该前缀，并保留 UNC 与 Windows 大小写归一化；macOS/Linux 不再错误折叠大小写，定向回归已覆盖该路径。项目切换器仅为 missing 条目显示“查找已移动的知识库”图标按钮，React key 同时含 `projectId + rootPath`，避免历史重复 UUID 时的列表冲突；失败时按后端 code 区分原路径仍可用、recent 已变化、跨进程锁占用和身份无法验证，保留当前项目并取消临时 assessment。

验证：无 GUI Rust 定向回归通过（available-old-root 拒绝、final UUID mismatch 拒绝且 recent 保持、exact entry replace / target conflict、registry commit-failure 不重绑、Windows prefix / 平台大小写语义、recent concurrent update、OS-backed file lock 跨进程互斥），`cargo check`、`cargo fmt`、`tsc -b`、lint 均通过；两位独立 review 的 P0/P1 已关闭。locale 已按 JSON 解析验证。该轮关于 sandbox 无法运行 Vite/Vitest 的历史环境观察已由第十三轮的实际 Windows runner 验证取代；以第十三轮的 Vitest、quick gate 与完整 gate 记录为准。

## 2026-08-03 实施复核（第十一轮）

最近知识库管理已收敛为三条可证明安全的操作：安全移除、重新评估，以及清除歧义 Markdown 的已记忆意图。`remove_recent_project` 以 `projectId + 规范化 rootPath` 精确匹配全局 `recent-projects.json` 条目；它不会枚举、修改或删除项目目录，丢失目录的条目同样可以移除。`remember_recent_project` 与移除共享同进程写入锁，避免两个 command 的 read-modify-write 互相覆盖；原子替换仍只保证文件完整性，不能单独解决丢失更新。

“重新评估”只对现有 recent root 执行只读 assessment。可直接打开的结果才切换项目；需要歧义/材料/不可读决策时，状态层会释放当前项目但保留完成的 assessment，交给 no-project workspace 显示下一步；评估、清除记忆或打开失败则继续保留当前项目。项目切换器不再宣称 `role=menu`，改为普通可聚焦按钮，避免不完整的 roving-menu 语义。

本轮撤回了此前“选择一个新目录后成功打开即可移除旧 recent”的重新定位候选。当前 `projectId` 是会话 ID，而 `canonicalIdentityKey` 本身含路径；二者都不能证明移动后的目录仍是同一知识库。缺失条目只能安全移除；用户仍可通过“打开已有知识库”显式打开新位置。身份验证的重新定位仍是未关闭项：需要先在创建时持久化可移动的 native identity，并由后端在同一原子操作中比较候选身份后更新 recent entry；compatible vault 在没有可验证稳定身份时必须继续要求用户手动保留/移除记录。

验证：无 GUI Rust 定向回归覆盖精确移除、项目目录不触碰以及并发 remember/remove 两项更新均保留；默认 `cargo check`、`cargo fmt --check`、`tsc -b` 与 lint 通过。`npm run check:quick` 的 lint 和 no-GUI Rust core lane 通过，但 frontend build 仍同时遇到 Tailwind Windows 原生模块不可加载与 Vite/Rolldown Windows realpath 子进程 `spawn EPERM`，故 quick gate 整体不计通过。前端 Vitest 的不可运行性也已独立复现：即使移除 Tailwind 插件，config loader 仍报相同 `spawn EPERM`，因此该限制不是应用 CSS 或本项功能导致的失败。

## 2026-08-03 实施复核（第十轮，已由第十一轮修正）

本轮最初实现了安全移除，并曾提出“新目录成功打开后移除旧 recent entry”的重新定位候选。第十一轮独立复审确认该候选不能证明两个路径是同一知识库，已撤回；不得将本节视为重新定位已完成的证据。安全移除的精确匹配和原子全局配置更新仍然有效，且项目目录不会被枚举、修改或删除；即使项目已丢失，用户仍可在 project switcher 中清理对应的最近项。

验证：无 GUI Rust 定向回归确认只移除精确匹配项、保留其他条目、两个实际目录均未被触碰；默认 `cargo check` 与 `tsc -b` 通过。前端 Vitest 的不可运行性也得到进一步隔离：删除 Tailwind 插件后，Vite/Rolldown config loader 仍因 sandbox 拒绝 Windows realpath 子进程而报 `spawn EPERM`。因此该限制不是应用 CSS 或本项功能导致的失败。

跨进程 atomic no-follow 的工程前置条件也已明确：现有依赖没有跨平台 directory-handle / `open_dir_nofollow` 实现；评估过的 capability filesystem API 可提供该语义，但当前 Cargo registry 访问在已授权环境中仍超时，且本机未缓存该库。继续使用现有 path-based API 不能关闭此发布阻塞，故保持明确的 No-go，而不是以检查后重验冒充原子保护。

## 2026-08-03 实施复核（第九轮）

歧义 Markdown 文件夹的已记忆选择现已具备完整的撤销路径，并严格位于“最近知识库”管理界面：项目切换器为每个可用最近项目提供“重新选择如何使用此文件夹”的操作。该操作先执行只读 assessment；只有当前仍是歧义 Markdown 时，才调用新的 `clear_ambiguous_project_intent`，清除按 canonical identity 与 identity revision 精确绑定的全局 `project-open-decisions.json` 条目。目标文件夹、`.app` 和 `.git` 均不会被创建、修改或删除；下一次打开会重新显示两个显式意图选项。

同一清除命令在 assessment 卡片中也有可见反馈，前端 Zustand store 只以返回的 typed assessment 更新本地状态。后端拒绝非歧义 assessment 的清除请求，以避免把“重新选择”误用为一般项目写入能力。

验证：针对性 Rust 回归通过，覆盖记忆、清除、fresh service 重新 assessment，以及 selected folder 从未出现 `.app` / `.git`；默认 `cargo check`、`cargo fmt --check` 与 TypeScript `tsc -b` 通过；两份 locale 使用 Node `JSON.parse` 验证为有效 JSON。`npm run check:quick` 的 lint 与无 GUI Rust core lane 通过，但 Vite build 在当前 sandbox 加载 Tailwind Windows 原生模块时被阻断，并伴随 `spawn EPERM`；完整前端 Vitest/E2E 未计为通过。

## 2026-08-03 实施复核（第八轮）

审计中的 P1 外部 AI / 项目写入 authority 缺口已在后端集中收口：`AppState` 新增基于当前 layout、identity、health、trust、filesystem 与 app-state persistence 的 `require_external_ai_access` 和 `require_project_write_access`。Chat、Source AI、deep Lint、Agent/BYOK HTML export 在创建任务前与异步执行开头均复验；会写入 `.app` 或 wiki 的 Chat session、保存回答、convenience edit 同样必须通过项目写入守卫。

因此，单纯的 `ProjectRegistry` canonical 路径登记不再可用于发送内容、调用 Agent/BYOK 或创建项目状态；Restricted、Recovery、Repairable、Unreadable、只读以及尚未具备安全 app-state 根目录的 compatible vault 均会以明确错误失败关闭。定向 26 项 AppState authority 回归通过，覆盖未信任拒绝、Recovery 在原生 trust 仍被撤销、无持久 state 根拒绝写入、健康原生项目通过，以及空原生项目保留一般 workflow state 但不能执行或写入。默认 Rust 编译仍需作为 IPC command 接线证据；GUI harness 运行态限制保持不变。

## 2026-08-03 实施复核（第七轮）

高风险 app-owned 写入现在新增了一层跨进程互斥：共享本应用配置目录的 LLM Wiki 进程会竞争同一个 OS 文件锁；锁覆盖 compatible vault 的 `.app/compat` guidance 写入与 Recovery 的 graph-cache repair。取得锁之后会再次验证项目根路径，原有的逐段 no-link/reparse 校验、hash/Git 快照复验和同进程锁仍保留。

验证：独立 Rust 子进程在父进程持锁时明确收到 `PROJECT_MUTATION_LOCKED`；独立 store 实例并发时峰值临界区为 1；compatible-guidance 定向回归 6/6 仍通过。该改动关闭了**协作的应用实例之间**的跨进程写入竞争，但不关闭 descriptor/handle-level atomic no-follow：任意不遵循应用锁的外部进程仍可在路径 API 的间隙替换目录或文件，故它继续是发布阻塞。

## 2026-08-03 实施复核（第六轮）

本轮实现了 Recovery 的第一条完整、受限修复闭环，范围严格限于可完全再生的 `.app/graph-cache.json`：

- assessment 不再只检查图谱缓存是否为任意 JSON，而是验证 `GraphData` schema；旧占位 `{nodes, edges}` 与语法损坏缓存都会进入 Recovery。新建知识库改写完整空 `GraphData`，不会把全新项目误判为损坏。
- `prepare_assessed_project_repair` 只读地绑定 assessment identity/revision、Git HEAD 与 dirty-path 预览、缓存 SHA-256、精确备份路径、受保护路径和“外链继续阻断”策略；PendingAction 在 10 分钟后过期。
- 确认后的 apply 重新核验 identity、缓存 hash、项目 root、Git preview 与可写性，先建立 Git checkpoint，再以 create-new 备份原损坏字节，最后原子替换为可读空缓存。目标在预览后变化会返回 `PROJECT_REPAIR_TARGET_CHANGED`，不写 backup 或缓存。
- Recovery 横幅新增“查看修复”入口，复用 authority dialog 与确认对话框，显示目标、备份、checkpoint、保护路径和外链边界；没有安全候选时明确保留只读 Recovery，而不伪造修复操作。

验证：默认 `cargo check`、Rust 格式检查、TypeScript build 通过；无 GUI Rust 定向图谱缓存回归 7/7 通过，覆盖 schema 检测、旧缓存检测、备份保留、原子再生、预览后漂移拒绝、现有图缓存失效路径，以及真实 Git checkpoint 后的 repair 顺序。GUI-feature 测试二进制编译后在当前 Windows sandbox 因 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` 不能启动，故 command-level 运行证据与前端 Vitest/E2E 仍不能计为通过。

这关闭了“图谱缓存安全修复”的实际实现缺口，但 **不关闭整个 repair/recovery Batch**：其他 `.app/*.json` 仍只读恢复（避免猜测用户配置/书签/冲突状态），跨进程原子 no-follow 及完整 UI/E2E 验收也仍是发布阻塞。

## 2026-08-03 实施复核（第五轮）

本轮收紧 compatible vault 的 app-owned guidance 写入路径：

- `.app/compat` 的创建改用逐段 no-link/reparse 检查的目录助手；助手仅返回本进程实际创建的路径，失败回滚不会删除并发创建或已被替换的目录。
- 同一应用进程的兼容启用操作以全局写入锁串行化；`purpose.md` / `schema.md` 在读取、临时文件创建、写入、rename 提交及安全回滚前后都重新验证根、父目录和目标文件。
- 出现路径漂移时，回滚只会删除仍可证明位于 canonical root 内的临时文件或空目录；无法再次证明安全的残留会保留，避免通过已重定向路径误删外部内容。

新增 compatibility guidance 的同进程并发测试；4/4 定向 Rust 测试通过，默认 `cargo check`、`cargo fmt --check` 通过。这里的结论必须严格限定为：**已缓解并验证应用内并发与路径漂移，不是跨进程原子 no-follow 保证**。外部进程仍可在路径检查与操作系统路径式 open/rename 之间替换目录，因此跨进程互斥/descriptor-relative 写入仍是发布阻塞项。

## 2026-08-03 实施复核（第四轮）

本轮关闭了“打开后同步递归盘点”的阻塞路径：

- `open_assessed_project` 与歧义 Markdown 的确认打开现在只返回 `inventoryState: scanning` 的轻量项目摘要；工作台接收并写入当前项目后，才显式调用 `start_project_inventory`。
- 后端以 project-scoped、memory-only 的可取消 Task 执行只读盘点；不会为 Restricted / read-only compatible vault 创建 `.app`、Task 文件或 Git 状态，也不会跟随子级 symlink / Windows reparse point。
- 盘点完成后以 `project://refreshed` 回写 `ready` 统计；取消时回写 `partial` 统计，状态栏明确标示“盘点中 / 部分结果 / 无法盘点”，不会把未知数据伪装成 0。
- 任务激活逻辑现在也会返回只读项目的内存任务，因此用户可以在 Task UI 中观察和取消盘点。

本轮验证：`node_modules\\.bin\\tsc.cmd -b`、`cargo check --manifest-path src-tauri/Cargo.toml`、`cargo fmt --manifest-path src-tauri/Cargo.toml --check` 通过；新增后台盘点 Rust 测试 3/3 通过（含 Windows junction 不跟随回归）。完整 Rust 库测试启动了 886 个用例、输出未见失败，但受执行环境 60 秒上限中止，**不计作完整全绿证据**。Vite/Vitest 仍受 sandbox 中 Tailwind 原生模块加载限制，UI E2E 未验证。

因此 Batch H 的“后台、可取消、部分就绪盘点”已完成；发布结论仍为 **No-go**，剩余关键阻塞是 repair plan/apply、兼容写入的 TOCTOU/no-follow 互斥保护，以及完整 UI/E2E 验收。

## 2026-08-03 实施复核（第三轮）

本轮继续关闭审查中能以局部、可验证改动收敛的安全问题：

- `layout` 发现现在与 quick assessment 共用同一 deadline / cancellation budget；在进入 native discovery 及每个顶层、嵌套 Markdown 探测前都会停止，而不是在已开始递归后忽略取消。
- 新建到“预先存在但为空”的目标也统一走同 parent staging。安装时会短暂、可验证地迁移原空目录；若目标在初检后被其他进程写入，会恢复原目录并返回 `PROJECT_DIR_CHANGED_DURING_CREATION`，不会覆盖该并发内容。
- quick assessment 新增可移植路径名冲突检测：case-only、NFC/NFD 与 Windows 尾点/空格别名会产生 `PROJECT_PATH_NAME_COLLISION`，使项目进入 `Repairable`，从而阻止 compatible trust、写入及外部 AI 执行，但保持 Markdown 可读。
- 首次打开“新建知识库”时，后端只会准备应用专用的 `Documents/LLM Wiki` 父目录；普通资料目录不会被触及。若该位置不存在会创建该空容器，若它是文件或链接则拒绝并让用户改选位置；之后仍优先使用用户最后手动选择的 parent。
- 根 symlink/junction 现在先 canonicalize 到真实目录再注册/操作；后代 symlink/reparse point 仍被逐段拒绝，防止索引或写入逃逸项目边界。

这并不等价于 Batch H 已完成：该扫描仍有条目上限、没有后台 deep inventory；根 symlink 语义、compatible enablement 的 no-follow/跨进程互斥及 repair plan/apply 仍未闭环。发布判定继续是 **No-go**。

## 2026-08-03 实施复核（第二轮）

本轮根据两份独立代码审查继续修复了可在当前范围内闭环的 P1 问题：

- 新建失败现在保留在模态框内，并且后端在“项目已经创建、但注册/评估/最近项目记录失败”时返回 `PROJECT_CREATED_OPEN_FAILED`，携带项目根目录、失败步骤和重新打开路径；不再把已落盘项目误报为普通创建失败。
- 无项目状态下的设置按钮不再是无效控件。它只开放不依赖项目的语言与主题偏好，通过新的 Tauri 命令保存到全局设置；Provider、密钥和项目设置仍明确要求先打开项目。
- Windows 的只读快速判定不再错误地把所有非 POSIX 目录判为只读；真正写入仍由后端写路径重验。
- `Recovery` 现在在后端 workflow access resolver 中实际降级为不可执行，且 session capability 也不会给予外部 AI 或项目写入。`Unreadable`/尚无 Source 则继续由各 workflow 的前置条件处理，避免将空的新原生库误判为损坏库。
- 新建对话框会记住用户上次手动选择的父目录；`Documents/LLM Wiki` 的首次默认目录仍未实现，保留为 P2。

第二份 fresh-context 审查确认无 P0 回归，并指出当时仍不能关闭发布门禁的部分：layout discovery 的取消/预算覆盖、Unicode/case collision 检测、同步递归盘点、兼容模式写入 TOCTOU、repair plan/apply，以及 remembered ambiguous decision 的清除入口。前两项已在第三轮补齐 quick-assessment 范围；其余结论已回写到执行计划，整体判定仍是 **No-go**。

第二轮的当时验证为 `cargo check --manifest-path src-tauri/Cargo.toml`、`node_modules\\.bin\\tsc.cmd -b`、两个新增聚焦 Rust 测试，以及完整 Rust 库测试（875 passed、0 failed、2 ignored）。第三轮的最后验证为默认 Rust `cargo check`、TypeScript build，以及完整 Rust 库测试 **882 passed、0 failed、2 ignored**；完整前端 Vitest/Vite 仍因沙箱阻止 Tailwind 原生模块加载而无法启动，故未将 UI E2E 标为已验证。

## 1. 审计结论

### 2026-08-03 实施复核

本审计原先识别出的 **ordinary materials 原地初始化 P0 已关闭**；首屏已迁入持久 `AppShell`，新建成功后进入 Import，typed assessment/authority、ambiguous 选择记忆、普通资料 Import handoff，以及最近项目 assessment-first 重开均已落地。

本轮实现证据：

- `src/app/App.tsx` 始终渲染 `AppShell`，无项目时由 `WorkspaceController → WorkspaceRouter → NoProjectWorkspace` 呈现完整工作台和两个入口。
- 旧 `InitializeFolder`、预览命令、文件归档/移动逻辑和第三入口已移除；`open_project` 对普通目录 fail closed。
- `ProjectSessionAuthority` 成为 create/open/explicit authority refresh 的后端快照，并在右侧展示 type/trust/filesystem/health/Git；Recovery 有非颜色依赖 banner。
- 新建项目在同 parent 私有 staging 目录完成结构、Git 与初始 commit 后再 rename 安装；失败会给出 recovery metadata，现有空目标目录不会被删除。
- ambiguous Markdown 意图按 canonical identity + revision 保存于全局设置，绝不写入被打开目录；普通资料目录只会作为创建后 Import discovery 的候选。
- startup 与通知项目切换均改为 assessment-first，不再从 UI 调用 legacy binary `open_project`。

复核验证：项目服务 Rust 测试 **37/37**、默认 Rust `cargo check`、TypeScript build、i18n JSON parse、`git diff --check` 均通过；Impeccable changed-surface detector 返回 `[]`。前端 Vitest/Vite 在当前沙箱仍无法启动（Tailwind 原生模块被拦截，提升请求由审批通道拒绝），因此 UI 端到端场景不能标记为已验证。

仍未达到完整发布定义：repair plan/apply、background deep inventory、根 symlink 允许策略、完整 global no-project Settings、remembered-decision 的 recent 管理清除入口，以及全部 17 场景 UI/E2E 证据仍待完成。因此结论从“完全未完成”更新为 **核心首启路径已修复，但整体仍 No-go**。

**当前产品层面答案是：核心首启/打开路径已实现，但整体设计尚未完成。**

当前实现已经具备一套质量较高的后端项目评估、项目身份、信任存储、兼容布局和 Workflows 授权底座；用户可经历首次启动、新建知识库、打开已有知识库、歧义判断、普通资料文件夹转入 Import 与 assessment-first 重启。Recovery repair、深度扫描、完整模块就绪状态与跨平台路径策略仍未完成目标迁移。

更准确的状态是：

- **后端授权与 assessment 主干：已实现，具备继续扩展的基础。**
- **首次启动与打开项目的核心前端产品流程：已实现，仍欠完整异常/设置闭环。**
- **旧启动页和旧普通文件夹初始化路径：已从正式路径移除。**
- **按权威规范的 17 个验收场景：核心路径已提升，但 repair/deep scan/link/全量 E2E 仍使整体不能签收。**

因此当前版本不应被标记为 “First-run / Project-open Workbench 已完成”，也不宜在移除普通文件夹原地初始化能力之前发布该流程。

---

## 2. 审计问题与判定口径

本审计回答四个问题：

1. `2026-07-30-first-run-project-open-workbench-design.md` 中确认的产品设计是否已经映射到可达代码路径？
2. 已实现的代码是否保留了设计要求的安全边界、状态模型和交互语义？
3. 现有测试是否证明新设计完成，还是只证明旧行为稳定？
4. 在可以对外宣称完成之前，还需要哪些跨前端、IPC、Rust 服务、持久化和测试工作？

判定状态：

| 状态 | 含义 |
|---|---|
| ✅ 已实现 | 用户可从正式入口完成该场景，前后端状态与安全边界符合规范，并有相称测试 |
| 🟡 部分实现 | 基础类型或局部 UI 已存在，但缺少关键步骤、持久状态、异常路径或验收覆盖 |
| ❌ 未实现 | 正式用户流程缺失、被阻断或仍执行被规范废弃的行为 |
| ⚠️ 冲突 | 当前可达行为直接违反权威设计或仓库硬规则 |

“存在组件、命令或 DTO”不自动等于产品场景已完成。只有可达流程、授权、状态、恢复、文案和测试共同闭环才判定为已实现。

---

## 3. 权威顺序与审计范围

### 3.1 权威顺序

发生冲突时，本审计采用以下顺序：

1. 根目录 `AGENTS.md` / `CLAUDE.md` 的安全与架构硬规则
2. `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
3. `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`
4. `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`
5. `SPEC/PRD.md`、`SPEC/SPEC.md`、`SPEC/APP_flow.md`、`SPEC/TECH_STACK.md`、`SPEC/BACKEND_STRUCTURE.md`
6. 当前代码、测试与历史计划

历史 `UI-Frontend-design/import*.html` 仅作为视觉密度参考，不能恢复普通目录原地初始化、导入后自动编译或首屏 Agent/BYOK 设置。

### 3.2 已检查范围

前端主要入口：

- `src/app/App.tsx`
- `src/components/app/AppShell.tsx`
- `src/components/app/WorkspaceRouter.tsx`
- `src/components/app/RightContextPanel.tsx`
- `src/components/app/BottomStatusBar.tsx`
- `src/features/project/ProjectStartView.tsx`
- `src/features/project/ProjectAssessmentPanel.tsx`
- `src/features/project/ProjectAuthorityDialog.tsx`
- `src/stores/projectStore.ts`
- `src/stores/navigationStore.ts`
- `src/types/project.ts`
- `src/styles.css`
- 相关 i18n 与测试

Rust / IPC 主要入口：

- `src-tauri/src/commands/project_commands.rs`
- `src-tauri/src/commands/chat_commands.rs`
- `src-tauri/src/services/project_service.rs`
- `src-tauri/src/services/project_service/assessment.rs`
- `src-tauri/src/services/project_service/trust_store.rs`
- `src-tauri/src/models/project.rs`
- `src-tauri/src/models/layout.rs`
- `src-tauri/src/utils/path_safety.rs`
- `src-tauri/src/app_state.rs`

### 3.3 仓库基线

| 项目 | 审计值 |
|---|---|
| 分支 | `master` |
| HEAD | `90b9b0a feat(workflows): complete batch 8 cutover and closure` |
| 审计开始时工作树改动 | 61 个路径 |
| 本审计对应用代码的修改 | 0 |
| `UI-Frontend-design/` 修改 | 0 |
| 用户知识库内容修改 | 0 |

工作树中已有大量用户改动；本审计没有把这些改动归因于本任务，也没有清理、覆盖或暂存它们。

---

## 4. 执行摘要

### 4.1 产品验收概览

| 结果 | 数量 | 比例说明 |
|---|---:|---|
| 已实现 | 3 | 后端健康 Native/Restricted 打开与 Git 脏状态守卫较完整 |
| 部分实现 | 6 | 局部能力存在，但流程、UI 状态或异常闭环不完整 |
| 未实现/冲突 | 8 | 首屏、创建交接、材料路由、修复恢复、链接边界和重启等核心场景 |
| 合计 | 17 | 对应权威规范 §14 的全部验收场景 |

### 4.2 技术审计健康分

该分数评价当前首启/打开项目实现的技术质量，不代表产品完成百分比。

| # | 维度 | 分数 | 关键发现 |
|---|---|---:|---|
| 1 | Accessibility | 3/4 | 对话框焦点、ARIA、focus-visible 和 reduced-motion 已有基础；新流程缺失导致依赖状态与恢复状态无法被正确表达 |
| 2 | Performance | 2/4 | quick assessment 可取消，但打开后的项目扫描仍为同步递归流程，未形成后台可取消 deep scan |
| 3 | Responsive / Window resilience | 2/4 | 旧启动页有多档断点，但不是目标 Shell；完整工作台在无项目状态下不存在，无法验证窗口缩放一致性 |
| 4 | Theming | 3/4 | 项目 token、暗色模式和字体体系基本统一；遗留 launch CSS 仍维护第二套页面结构 |
| 5 | Implementation Integrity | 1/4 | 代码内部可运行，但与当前唯一产品权威存在系统性冲突，并保留高风险旧路径 |
| **总分** |  | **11/20** | **Acceptable：底座可复用，但需要显著产品迁移** |

### 4.3 严重度统计

| 严重度 | 数量 | 说明 |
|---|---:|---|
| P0 Blocking | 1 | 普通材料目录仍可被原地初始化并移动原文件 |
| P1 Major | 9 | 首屏、创建交接、决策路由、状态授权、修复恢复、深度扫描、重启、路径边界与外部 AI 授权 |
| P2 Minor | 4 | 新建表单完整性、兼容启用细节、旧测试契约、局部 UI 工程细节 |
| P3 Polish | 0 | 当前阶段不增加低价值视觉噪音 |

---

## 5. 初始审计逐条验收矩阵（历史基线）

| # | 规范场景 | 状态 | 当前证据 | 缺口 / 判定 |
|---:|---|---:|---|---|
| 1 | Fresh install：完整 Shell + 两张操作卡 | ❌ | `src/app/App.tsx:28` 在无项目时返回独立 `ProjectStartView` | Shell、主导航、设置、右栏和 28px 状态栏均未保留；旧页还有 Hero、搜索、最近项目、三入口和 Agent/BYOK |
| 2 | 新建项目默认值 | 🟡 | `ProjectStartView.tsx:536-670` 有名称、父目录、模板和完整路径 | 默认父目录为空；未记忆上次目录；模板无说明；非法字符通过静默清洗处理 |
| 3 | 新建成功后打开 Import 且不自动开系统选择器 | ❌ | `ProjectStartView.tsx:465-469` 只调用 `createProject`；导航默认 Dashboard | 没有 `setActiveView("import")`、一次性成功条和来源交接 |
| 4 | 第一次 Import 后选中可读 Source，不自动编译 | 🟡 | Import 已有独立 Source-first 实现 | 首启流程没有进入 Import，本次审计未重新验证完整 Import 验收矩阵；不得据此宣称端到端通过 |
| 5 | 健康 Native 直接打开 Dashboard | ✅ | `open_assessed_project` 允许健康 Native；启动默认视图为 Dashboard | 主路径具备类型评估与项目注册 |
| 6 | 首次打开健康 Obsidian：Restricted Compatible 且零写入 | ✅ | assessment + layout + trust store 组合；相关 Rust 测试验证未创建 `.app` / `.git` | 后续 UI 状态表达不足，但首次打开安全边界成立 |
| 7 | 信任 Obsidian 前确认 `.app/compat`、模板和 Git | 🟡 | `ProjectAuthorityDialog.tsx` 展示路径与 Git 选择；后端仅写 `.app/compat/*` | 模板选择固定为当前模板，右栏缺少完整状态回显；兼容能力范围仍需验证 |
| 8 | 歧义 Markdown：用户二选一 | ❌ | assessment 可返回 ambiguous；`ProjectAssessmentPanel.tsx:68-77` 只有返回和可选打开 | 无“作为 Markdown 知识库打开 / 用这些资料新建”；无按 canonical identity 记忆意图 |
| 9 | PDF/Office 普通资料目录：原目录不变，创建后 Import | ⚠️ | `ProjectStartView.tsx:325` 仍调用 `open_folder_as_project`；`project_service.rs:688-732` 会移动文件 | 与权威设计和硬规则直接冲突 |
| 10 | corrupt graph cache：可确认再生 | ❌ | assessment 可识别部分健康异常 | 无 typed repair plan、路径清单、checkpoint/backup 状态和 repair command |
| 11 | corrupt `.app` + 可读 Markdown：Recovery Dashboard | ❌ | `assessment.rs:407-430` 可给出 `ProjectHealth::Recovery` | 前端没有 Recovery 工作台；当前路径可能把它当普通项目打开，无法证明写操作被系统性禁用 |
| 12 | 根 symlink/junction：canonicalize 后允许 | ❌ | `path_safety.rs:5-20` 明确拒绝根 link/reparse point | 当前实现比规范更保守，但不符合已确认验收结果 |
| 13 | 内部链接逃逸 root：显示但不跟随/索引 | 🟡 | layout/assessment 会排除链接并产生 warning | 对内部安全链接也基本一概拒绝；“显示外链但不跟随”的用户可见模型不完整 |
| 14 | Git dirty：不静默 commit/stash，高风险写受控 | ✅ | assessment 读取 Git 状态；Workflows 权限/检查点守卫已实现 | 仍需在 Repair/Import/非 Workflow 写操作的最终门禁中统一复用 |
| 15 | 只读目录：长期 Read-only Dashboard | 🟡 | assessment 有 `read_only` 和读取 capabilities | 状态打开后未保存在 `ProjectSummary`；右栏与模块路由不能持续表达只读能力 |
| 16 | AI route 缺失：去配置、返回、不自动运行 | 🟡 | Workflows 的 `WorkspaceController` 已实现返回语义 | 未证明 Chat、Source AI、Lint 等所有 AI 入口统一消费显式信任与 route prerequisite |
| 17 | 重启：最新有效项目打开到 Dashboard | ❌ | `projectStore.ts:596-620` 用 `.find(project => !project.missing)` | 最新目录丢失时会静默打开更旧项目；且仍走旧 `open_project` 而非 typed assessment |

---

## 6. 分层实现现状

### 6.1 已完成且应保留的后端底座

#### A. Typed assessment 生命周期

已实现：

- `start_project_open_assessment`
- `get_project_open_assessment`
- `cancel_project_open_assessment`
- `open_assessed_project`
- opaque operation ID 与短期 assessment ID 分离
- 取消后丢弃未完成快照
- 有界 Markdown 扫描和取消检查

主要证据：

- `src/stores/projectStore.ts:300-397`
- `src-tauri/src/commands/project_commands.rs:102-159`
- `src-tauri/src/services/project_service/assessment.rs:81-215`

#### B. 类型化分类与独立维度

TypeScript 和 Rust 均已建立：

- Native / Legacy / NashSu / Obsidian / Markdown / Ambiguous / Materials / Unknown
- trust state
- filesystem access
- health
- layout
- capability list
- Git 摘要

主要证据：

- `src/types/project.ts:5-121`
- `src-tauri/src/models/project.rs:48-169`
- `src-tauri/src/services/project_service/assessment.rs:247-430`

当前关键不足不是没有类型，而是这些类型在打开项目后没有成为持续 session authority。

#### C. 全局信任存储与项目身份

已实现：

- 信任保存在应用配置而非项目目录
- canonical folder identity 绑定
- 目录被替换或身份变化后不恢复旧信任
- grant / restore / revoke
- 锁与损坏配置测试
- CJK 路径测试

主要证据：`src-tauri/src/services/project_service/trust_store.rs`。

#### D. Compatible guidance 最小写入

后端启用兼容模式时仅创建：

```text
.app/
└── compat/
    ├── purpose.md
    └── schema.md
```

实现具备临时文件、拒绝覆盖和失败清理逻辑，并保留已有 Markdown / `.obsidian`。这是当前最接近规范完成态的跨层切片之一。

### 6.2 只完成了展示或局部接线的前端

`ProjectAssessmentPanel` 已能显示：

- format
- trust
- filesystem
- health
- Markdown roots
- capability count
- Git 状态
- warnings

但它没有根据 classification/health 生成产品决策。当前 `ProjectStartView.tsx:193-202` 对大多数格式自动打开，对 ambiguous/materials/unknown 只留下 assessment 面板；调用处又没有传 `onOpen`，导致这些类型没有完成路径。

`ProjectAuthorityDialog` 已具备信任、Git、兼容 guidance 和 revoke 操作，但它位于项目打开后的设置/前置条件路径，不能替代打开过程中的确认、修复和恢复路由。

### 6.3 仍由旧模型驱动的部分

- `App.tsx` 仍把无项目状态切成独立页面。
- `ProjectStartView` 仍负责最近项目、首屏 Agent/BYOK 探测、模板展示、三入口和自有状态栏。
- `openProject` / `open_project` 旧路径仍被正式 UI 与启动恢复调用。
- `ProjectSummary` 仍以旧布尔 health 和路径计数为核心，未保存 assessment authority。
- `WorkspaceRouter` 未消费 project capabilities / health / trust / filesystem。
- `RightContextPanel` 未展示四个独立状态维度。
- `scan_project` 仍是打开时同步扫描。

---

## 7. 详细发现

## P0 Blocking

### P0-01 普通材料目录仍可被原地初始化并移动原文件

**类别：** Implementation Integrity / Data Safety  
**规范：** §6.4、§15；AGENTS.md “ordinary materials folders must never be initialized or reorganized in place”  
**位置：**

- `src/features/project/ProjectStartView.tsx:325-329`
- `src/features/project/ProjectStartView.tsx:180-205`
- `src/stores/projectStore.ts:273-298`
- `src-tauri/src/commands/project_commands.rs:65-98`
- `src-tauri/src/services/project_service.rs:505-610`
- `src-tauri/src/services/project_service.rs:688-732`

**现状：** “打开文件夹作为项目”仍可调用旧 `open_project`。普通目录会得到 `InitializeFolder` confirmation，确认后初始化 Git、写项目结构并通过 `archive_loose_files` / `fs::rename` 移动原文件。

**影响：** 即使存在确认与 Git checkpoint，这仍违背“原始资料文件夹保持不动”的已确认产品承诺。用户可能把普通工作目录当作资料来源，却在确认后看到其结构被应用重排。

**建议：**

1. 先写一条失败回归测试，证明正式 UI 和启动路径不再调用 `open_folder_as_project`。
2. 在具备 Git checkpoint 的实施批次中移除前端入口和 store intent。
3. 普通资料 assessment 只能进入“标准新建对话框 → 创建新知识库 → 原目录预填为 Import folder candidate”。
4. 待所有调用者迁移后删除或永久隔离 `InitializeFolder` 后端命令、确认类型和移动测试。
5. 保留原目录 hash/mtime 不变测试，覆盖 CJK、Unicode、嵌套目录和同名目标。

**完成标准：** 仓库搜索无可达 `open_folder_as_project`；普通材料端到端测试证明源目录树、内容 hash 和 Git 状态不变。

## P1 Major

### P1-01 无项目状态仍是独立 Launch Page，而不是完整工作台

**类别：** Implementation Integrity / Information Architecture / Accessibility  
**位置：** `src/app/App.tsx:28`、`src/features/project/ProjectStartView.tsx:220-460`、`src/styles.css:1229-1323`  
**现状：** 无项目时完全卸载 `AppShell`，渲染带 Hero、搜索、过滤、最近项目卡、三快捷入口、Agent/BYOK 右栏和独立底栏的旧启动页。

**影响：** 用户首次启动看不到真实导航、设置、右侧上下文和状态栏；进入项目时整个界面结构跳变；不可用模块无法解释依赖；直接违反规范 §2.1、§4 和 §15。

**建议：** 让 `AppShell` 始终存在，将无项目中心内容建成 `NoProjectWorkspace`；Shell 各区域消费明确的 no-project view model，而不是在组件内部猜测空值。

### P1-02 新建流程没有事务边界，也没有 Import 交接

**类别：** Implementation Integrity / Recovery  
**位置：** `ProjectStartView.tsx:465-469`、`projectStore.ts:251-270`、`project_service.rs:291-360`、`project_service.rs:906-945`  
**现状：** 后端能创建结构并初始化 Git，但没有一个统一 transaction journal / rollback report；前端成功后只关闭对话框并依赖默认 Dashboard。

**影响：** 用户无法立即完成第一次 Source；后半程失败可能留下不完整目录；失败后也无法知道哪些路径已创建、哪些已回滚。

**建议：** 后端返回 typed creation outcome（created paths、rolled-back paths、retained paths、Git state、recovery steps）；前端成功后明确导航到 Import，并传递一次性 success handoff。

### P1-03 Ambiguous 与 Materials 缺少决策路由

**类别：** Information Architecture / State Modeling  
**位置：** `ProjectStartView.tsx:193-202`、`ProjectStartView.tsx:312-316`、`ProjectAssessmentPanel.tsx:68-77`  
**现状：** 后端能分类，但面板只有事实展示，没有分类特定动作；ambiguous/materials/unknown 用户只能返回。

**影响：** 两个核心打开场景直接无法完成；产品虽然“不猜”，但也没有提供可操作决策。

**建议：** 建立 typed assessment route union，为 native direct-open、restricted-open、ambiguous-choice、materials-create、repair-confirmation、recovery-open、unreadable-error 分别提供唯一 UI 与命令集合。

### P1-04 Assessment authority 在打开后丢失，模块无法系统性授权

**类别：** Architecture / Authorization / Readiness  
**位置：** `src/stores/projectStore.ts:375-397`、`src/types/project.ts`、`RightContextPanel.tsx:446-500`、`WorkspaceRouter.tsx:62-80`  
**现状：** 打开后 assessment 被清空；`ProjectSummary` 没有完整 format/trust/filesystem/health/capabilities；工作区路由直接渲染模块。

**影响：** UI 不能持续区分 Trusted Read-only、Untrusted Read-only、Recovery、Compatible；模块可能依靠“缺文件后报错”而不是显式 prerequisite；右栏显示的信息与后端授权事实脱节。

**建议：** 引入后端派生的 `ProjectSessionAuthority`，在每次打开、信任、修复、Git 或文件系统状态变化后刷新；模块只消费 typed readiness，不复制授权判断。

### P1-05 Repair plan 与 Recovery Dashboard 未实现

**类别：** Recovery / Data Safety  
**位置：** `src-tauri/src/models/project.rs:130-162`、`assessment.rs:407-430`、项目打开前端  
**现状：** `ProjectOpenAssessment` 没有 repair plan；后端只把损坏 `.app` 标记为 Recovery；前端没有 repair confirmation 或 recovery surface。

**影响：** 可读 Markdown 可能因为 app state 损坏而得到模糊结果；用户看不到将修改哪些路径、是否有备份/checkpoint，也无法安全选择“暂不修复”。

**建议：** 把 repair plan 建成不可变、短期、与 assessment identity 绑定的 typed contract；prepare 只读，apply 重验证、建 checkpoint、逐操作记录并返回 recovery outcome。

### P1-06 Deep scan 不是后台、可取消、部分可用的 operation

**类别：** Performance / Long-task UX  
**位置：** `project_commands.rs:149-155`、`project_service.rs:381-436`  
**现状：** `open_assessed_project` 后同步调用递归 `scan_project`；没有 deep scan operation ID、进度事件、取消、partial 标志或重试状态。

**影响：** 大型 vault 打开耗时不可观测；用户可能看到空 Search/Graph 而不知道仍在扫描；取消 quick scan 不能解决打开后的长扫描。

**建议：** quick scan 只产生安全路由和最小 roots；打开 Dashboard 后启动 project-scoped background inventory task，持久化 discovered counts、progress、partial/failure/cancelled 状态。

### P1-07 启动恢复会静默跳过最新丢失项目

**类别：** State / Error handling  
**位置：** `src/stores/projectStore.ts:596-620`  
**现状：** bootstrap 用 `.find(project => !project.missing)` 选择第一个未丢失项目。

**影响：** 最新目录失效时应用打开另一个旧项目，用户可能在错误知识库中继续操作；规范要求保留完整 Shell 并显示最新路径错误。

**建议：** 只尝试 recents[0]；失败后进入 no-project workbench，保留 typed startup error，不自动 fallback；用户明确从 project switcher 选择其他项目。

### P1-08 路径链接与碰撞语义不符合确认规范

**类别：** Filesystem Safety / Cross-platform  
**位置：** `src-tauri/src/utils/path_safety.rs:5-20`、`src-tauri/src/models/layout.rs:256-264`  
**现状：** 根 symlink/junction 直接拒绝；后代链接普遍排除；quick scan 未发现 case-only / Unicode-normalization collision contract。

**影响：** 合法的 canonicalized root 无法打开；内部安全链接也失去可读能力；Windows/macOS/Linux 上的碰撞风险没有以用户可见诊断呈现。

**建议：** canonical root identity 与 traversal policy 分离；允许根 link canonicalize；内部 link 在 canonical root 内带循环保护读取；外部 link 只显示不跟随；碰撞只报告、不自动改名。

### P1-09 非 Workflow 外部 AI 命令缺少统一显式 trust authority

**类别：** Authorization / Privacy  
**位置：** `src-tauri/src/app_state.rs:394-401`、`src-tauri/src/commands/chat_commands.rs:106-149` 及其他 AI 入口  
**现状：** Workflows 已消费 `resolve_workflow_access`；部分 Chat 等命令主要解析 `ProjectContext`，没有在命令边界显式消费统一 trust/filesystem/health/capability authority。

**影响：** 某些兼容布局可能因缺少路径而偶然失败，但“偶然失败”不等于明确禁止外部 AI 传输。Restricted mode 的隐私承诺需要可审计的统一授权判定。

**建议：** 提取 `resolve_project_operation_access(project, operation)`；所有 Agent、Skill、BYOK、外部传输、写入与自动修复入口在创建 task 或发起网络调用前 fail closed。

## P2 Minor

### P2-01 新建对话框缺少完整默认值与路径验证

**类别：** Forms / Error handling / i18n  
**位置：** `ProjectStartView.tsx:536-670`、`src/utils/projectPath.ts`、`project_service.rs:906-945`  
**缺口：** Documents/LLM Wiki 默认父目录、last parent 持久化、Windows reserved names、路径长度、明确的非法字符错误、模板一句话说明。

**建议：** 前端即时提示与后端 authoritative validation 使用同一错误 code union；不要只静默删除字符；失败后保留全部字段。

### P2-02 Compatible enable confirmation 缺少完整模板和状态回显

**类别：** UX / State  
**位置：** `src/features/project/ProjectAuthorityDialog.tsx`  
**缺口：** template 不是可选择字段；操作成功后右栏不能持续看到 type/trust/filesystem/health；“完整功能”实际能力范围可能被文案夸大。

**建议：** 展示精确 enabled capabilities，不使用超出后端能力的笼统承诺；操作成功后刷新 session authority。

### P2-03 测试仍把旧首屏行为当作正确契约

**类别：** Test Integrity  
**位置：** `src/app/App.test.tsx:103-127`  
**现状：** 测试明确断言三入口启动页，并断言 primary navigation 不存在。

**影响：** 新设计迁移会被旧测试阻挡；当前绿色测试只能证明旧实现稳定。

**建议：** 先把权威验收矩阵转为失败测试，再迁移实现；旧行为测试应删除或改写，不能简单放宽断言。

### P2-04 旧 Launch CSS 与新 Shell 并存，增加视觉和维护漂移

**类别：** Theming / Responsive / Implementation Integrity  
**位置：** `src/styles.css:1229-1323`、`src/styles.css:3189-3209`  
**现状：** 旧页面维护自有 56px 顶部、36px 底部、Hero、右侧抽屉和断点；目标 Shell 规定 48px topbar、28px status bar。

**影响：** 两套壳层会长期产生尺寸、状态、键盘焦点和中英文适配差异。

**建议：** 完成 Shell 迁移后删除仅服务 legacy launch 的 class；NoProjectWorkspace 只组合既有 token 和 Shell primitives。

---

## 8. 系统性问题

### 8.1 新旧打开模型并行存在

项目已经有 typed assessment，但旧 `open_project`、`open_folder_as_project`、`InitializeFolder` 和旧启动恢复仍活跃。新模型不能成为唯一真相，导致：

- UI 有时按 assessment 决策，有时按旧二元 open 结果决策；
- 测试同时保护互相冲突的行为；
- 项目打开后的 authority 无法确定来源；
- 高风险旧路径无法被安全证明为不可达。

### 8.2 “评估结果”没有升级为“项目会话权威”

分类、信任、文件系统、健康和 capabilities 只在打开前存在。进入 Shell 后又回到旧 `ProjectSummary`。这是右栏、模块就绪、Recovery、Restricted、AI trust 和 deep scan 无法闭环的共同根因。

### 8.3 后端安全能力领先于产品路由

assessment、trust store、compatible guidance 已经具备，但前端仍围绕 legacy launch 组织。这不是再增加一张对话框可以解决的问题；必须先建立状态机和唯一打开 orchestrator。

### 8.4 绿色测试覆盖的是局部稳定性，不是权威设计

当前定向测试全部通过，但至少一组测试明确固定已废弃的首屏。实施必须遵循“规范 → 失败验收测试 → 代码”，不能用现有绿色作为完成证据。

---

## 9. 正向发现

以下实现应被保留和复用：

1. **assessment operation 与 assessment ID 分离。** 取消、短期结果和命令边界比直接传路径安全。
2. **TS/Rust typed contract 基础较完整。** 格式、信任、文件系统、健康、layout、Git 和 capability 已有镜像类型。
3. **全局 trust store 使用 canonical identity。** 已覆盖目录替换、损坏 store、锁与 CJK 路径。
4. **Compatible guidance 写入范围正确。** 只写 `.app/compat/purpose.md` 和 `schema.md`，不覆盖根同名文件。
5. **现有 Git dirty 防护较强。** Workflows 已经具备 fail-closed access/checkpoint 思路，可作为其他高风险命令的统一模板。
6. **对话框与基础无障碍具备良好起点。** `useModalDialog` 提供 focus trap、Escape、初始焦点与恢复；全局 focus-visible、dark theme 和 reduced-motion 已存在。
7. **UI token 基础可复用。** `src/styles.css` 已有 spacing、颜色、字体、radius、motion token，无需新建第二套设计系统。

---

## 10. 验证记录

### 10.1 前端定向测试

命令：

```powershell
npm test -- src/app/App.test.tsx src/features/project/ProjectAssessmentPanel.test.tsx src/features/project/ProjectAuthorityDialog.test.tsx src/stores/projectStore.test.ts src/types/project.contract.test.ts
```

结果：

- 5 个测试文件通过
- 52 个测试通过
- 0 个失败

解释：这些测试证明 assessment、authority dialog、store 和现有 App 路径的局部行为稳定；`App.test.tsx` 同时固定了旧三入口启动页，因此不能作为新设计完成证据。

### 10.2 Rust 定向测试

命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features project_service --lib
```

结果：

- 37 个测试通过
- 0 个失败
- 841 个过滤

解释：其中包含旧 ordinary-folder `InitializeFolder` 行为的测试，说明该能力不是死代码，而是被主动维护的旧契约。

### 10.3 Mechanical detector

Impeccable detector 对目标相关 UI/CSS 进行一次扫描，返回 4 个候选：

- 两处 blockquote 边线：分别属于 Wiki prose / Chat prose，不属于首启页，判定为与本审计目标无关。
- `progress__fill` 的 width transition：为通用进度条性能候选，需在 deep scan 实现时改为 transform 或验证影响。
- graph mini grid background：属于真实图谱可视化，判定为误报。

detector 没有发现新的目标页面 token/视觉违规。当前主要问题是产品状态与架构完整性，而不是机械样式错误。

### 10.4 未运行的检查

本任务只新增审计与计划文档，没有修改应用代码或执行配置，因此按仓库规则未运行 `npm run check:quick` / `npm run check`。实施计划中的跨层批次必须运行完整 `npm run check`。

---

## 11. 发布判断

### 11.1 当前发布结论

**No-go：不能以 “First-run / Project-open Workbench 已实现” 发布。**

最低发布阻断项：

1. 普通材料目录原地初始化与移动必须从所有正式入口不可达并最终退休。
2. 无项目状态必须进入完整 Shell，且只有新建/打开两个主入口。
3. 新建必须进入 Import；ambiguous/materials 必须完成决策路由。
4. assessment authority 必须贯穿右栏、模块 prerequisite、Restricted 和 Recovery。
5. 修复、deep scan、startup failure 和路径边界必须按验收矩阵闭环。

### 11.2 可以独立保留的已完成能力

- typed assessment lifecycle
- compatible classification
- global trust persistence
- compatible guidance minimal writes
- Git dirty / workflow authorization foundation

### 11.3 完成定义

只有同时满足以下条件，才能关闭本审计：

- §14 的 17 个验收场景全部通过；不允许以“部分实现”关闭。
- 仓库正式 UI、启动恢复和 IPC 不再调用 legacy ordinary-folder initialization。
- `ProjectSessionAuthority` 或等价后端派生状态成为 UI 唯一权限事实。
- Recovery、read-only、restricted、partial scan 都有明确可访问 UI 与下一步。
- CJK、Unicode normalization、Windows reserved names、case collision、symlink/junction 和 read-only 测试通过。
- 两个代码 review 视角均无未关闭 P0/P1。
- 最终从头运行 `npm run check` 通过。
- `UI-Frontend-design/` 与示例 `wiki/` 零改动。

---

## 12. 推荐行动顺序

1. **P0：安全切断旧 ordinary-folder mutation。** 在任何视觉重构前确保原资料目录不可能被原地改造。
2. **P1：建立唯一 project-open 状态机与持久 session authority。** 先统一事实，再接 UI。
3. **P1：把 no-project 状态迁入现有 AppShell。** 完成两入口、设置可用、右栏和状态栏。
4. **P1：闭环 new → Import 与 ambiguous/materials 决策。** 形成首个完整价值路径。
5. **P1：实现 repair/recovery 与 background deep scan。** 让大型或损坏知识库仍可读、可恢复、可取消。
6. **P1：补齐 startup/path/AI authorization。** 完成跨平台与隐私边界。
7. **P2：清理 legacy tests/CSS/i18n，完成双 review 和完整门禁。**

具体批次、文件范围、测试、检查点和停止条件见配套执行计划。

---

## 13. 实施后剩余验收矩阵（2026-08-03）

| 规范场景 | 实施后判定 | 复核说明 |
|---|---|---|
| 完整 Shell + 两张操作卡 | ✅ | 正式 `AppShell` 路径中仅保留新建/打开两个入口。 |
| 新建默认目录与上次目录 | ✅ | 后端准备且验证 app-owned `Documents/LLM Wiki`；最后手动选择的 parent 保存在全局 UI 存储。 |
| 新建 → Import | ✅ | 创建、authority、一次性提示和 Import 路由已连接，不自动打开 picker。 |
| 首次 Import Source 选择 | 🟡 | 交接到既有 Import flow 已完成；完整 Import E2E 尚未在本轮运行。 |
| Native / Restricted Compatible 打开 | ✅ | assessment-first route 与 authority snapshot 已连接。 |
| ambiguous / materials 决策 | ✅ | explicit typed choice、identity 记忆、recent 管理中的清除/重新选择，以及 immutable Import handoff 已实现。 |
| Recovery | 🟡 | readable Dashboard、authority、banner 与 graph-cache 的 preview/confirm/checkpoint/backup/rehash repair 已实现；其余损坏 `.app` JSON 保持只读 Recovery，完整 UI/E2E 尚未运行。 |
| deep scan / partial results | ✅ | 打开后启动 memory-only 后台盘点；任务可见、可取消，并用 scanning/ready/partial 诚实表达结果。 |
| 根 symlink/junction | ✅ | 根链接 canonicalize 后进入真实目录；所有后代链接继续被拒绝，避免越界跟随或写入。 |
| Startup latest-only | 🟡 | 已改为 latest-only + assessment-first；需要可运行前端集成测试证明全部故障/decision 分支。 |

未列出的原验收项维持其原判定，直到对应跨层证据补齐。
