# Graph 原生标题栏拖动焦点验收记录（Windows，Batch 3）

## 结论

**通过。** 同一 Git 提交构建的 MSI、构建 EXE 与已安装 EXE 完成哈希绑定后，Windows 真实安装包上的原生标题栏拖动与真实 Alt-Tab 返回全部满足合同阈值：6 个测量轮次 get_graph=0、规范化前台 false=0、resize=0、>100ms 应用停顿=0，Graph 稳态位置滞后 P95 为 1.414px（阈值 12px），真实后台返回后 observed Graph 恰好刷新 1 次。

## 证据

| 项 | 值 |
| --- | --- |
| 源提交 | '55dc5a62265e2f00884790045a13ddff9f1edc8c'（worktree 干净，tree '75f8e01ddccb0b6d2287a659a374f6f667f83660'） |
| 原始 JSON | docs/qa/evidence/graph-titlebar-drag/55dc5a62265e2f00884790045a13ddff9f1edc8c/2026-08-31-windows.json（status=passed） |
| 构建命令 | npm run tauri -- build --bundles msi（结尾 updater 签名私钥缺失报错不影响已产出的 MSI bundle） |
| MSI | LLM Wiki Desktop_0.1.0_x64_en-US.msi，18,927,616 字节，SHA-256 '5d8073f4ef153521fa13bbce2dcc42fd54d6dfaf414a36ed6c7544b906892677' |
| 构建 EXE | 49,274,880 字节，SHA-256 '84d6c6dbee660ad89238524bf8543c66abbbf79b5c17beec5f0838825bc22ccb' |
| 已安装 EXE | %LOCALAPPDATA% 下 LLM Wiki Desktop 目录，SHA-256 与构建 EXE 完全一致（per-user 安装：ALLUSERS=2 MSIINSTALLPERUSER=1） |
| WebView2 | 151.0.4129.107 |
| 操作系统 | Windows 11 家庭版，build 26200 |
| 显示器 | AMD Radeon 890M：2880x1800@120Hz；NVIDIA RTX 5070 Laptop：2560x1440@319Hz |
| 测量窗口 | DPI 120（125% 缩放），固定尺寸，仅位置拖动 |
| Fixture | native-git-3-pages（3 wiki 页 / 240 支持文件 / Git tree '6f9dc550…'），fixtureHash 'd1156523bf9303b9d9b82f5380e1c576521ee5bc4c6be1fcc6fe10d3f574f08e' |

## 方法

- 刺激：Win32 SendInput 在原生非客户区标题栏 mouse-down，随后 112 步移动（每步 2px/1px，约 16ms 节拍，约 60Hz），最后 mouse-up；finally 保证 mouse-up。
- 观测：每步外部读取 GetWindowRect(HWND) 记录期望/实际位置、误差、前台 HWND/PID；MoveWindow 只在测量结束后恢复窗口原位，不参与测量。
- 轮次：Dashboard 与 Graph 各 1 轮不计入测量的 warm-up 拖动（落实计划“已预热稳定”的要求，消除每路由首次拖动的移动循环启动成本）加 3 个测量轮；Graph 预热后 get_graph 恰好 1 次。
- IPC：CDP 函数调用断点观测 get_graph；浏览器侧记录 DOM、raw Tauri、规范化 app://foreground-changed、resize、rAF gap 与 Long Task。
- Alt-Tab：真实 SendInput Alt-Tab 切换到第三方进程（away PID 45896 不等于目标 48228）后再次 Alt-Tab 返回目标 HWND。

## 结果

| 路由 | 轮次 | 有效样本 | 未移动 | 未移动比例 | 滞后 P95 | 滞后最大 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Dashboard | 1 | 112 | 2 | 1.79% | 1.414px | 4.472px |
| Dashboard | 2 | 112 | 1 | 0.89% | 1.414px | 2.236px |
| Dashboard | 3 | 112 | 1 | 0.89% | 1.414px | 2.236px |
| Graph | 1 | 112 | 2 | 1.79% | 1.414px | 4.472px |
| Graph | 2 | 112 | 2 | 1.79% | 1.414px | 4.472px |
| Graph | 3 | 112 | 1 | 0.89% | 1.414px | 2.236px |

阈值对照：未移动比例上限 10%（实际最高 1.79%）；Graph P95 上限 12px（实际 1.414px）；最大值上限 24px（实际最高 4.472px）；Graph 与 Dashboard 的 P95 差上限 8px（实际 0）；未移动比例差上限 5 个百分点（实际 0）；固定尺寸拖动 resize 为 0；拖动阶段 get_graph 增量为 0；标题栏阶段规范化 foreground=false 为 0；真实返回后 observed Graph get_graph 恰好 1；>100ms 应用停顿为 0。

焦点证据：每个测量轮 raw Tauri 序列均为 true 到 false 再到 true（WebView 标题栏噪声仍存在），规范化序列均为 true 到 true；Alt-Tab 规范化序列为 false 到 true。

合同测试：node --test scripts/graph-titlebar-drag-contract.node-test.mjs 13/13 通过（含坏样本 77/112、P95 152 拒绝，以及新增的“away 未离开目标进程必须失败”用例）。

## 装置迭代说明

首次真实采集（提交 4f201753）暴露三类测量装置缺陷，均已在后续提交修复并以 55dc5a62 重测通过：

1. 原生 helper 缺少合同要求的 stimulus 字段（3d0f31ee）。
2. Alt-Tab away 窗口绑定 helper 控制窗不可靠：现代记事本会把窗口交给宿主进程、后台 helper 无权 SetForegroundWindow、固定坐标点击可被更高 z 序窗口截走（a5821018、55dc5a62）。合同改为强校验 before 与 returned 精确回到目标进程和 HWND、away 为真实不同进程且不同 HWND；控制窗仅作为可切换窗口兜底。
3. 每路由首次拖动的移动循环启动成本与相位重置前排队的 resize 事件污染第 1 轮（3d0f31ee）：按计划加入每路由 1 轮不计入测量的 warm-up，resize 仅统计首末原生样本时间窗内的事件。

中间失败采集不作为验收证据入库；本记录叙述其结论，最终判定只依据 55dc5a62 的通过运行。

## 范围与遗留

- macOS/Linux 语义 smoke（真实退前台一次刷新、窗口框架交互不重复刷新）未在本 Windows 主机执行，不得以 Windows SendInput 脚本伪装，留待对应平台。
- get_graph 异步迁移门未触发（本次证据未达到触发条件），延期原因在 Batch 4 交付报告中汇总。
- 本次为 Batch 3 验收资产与脚本改动，未修改任何 Graph 或后端生产文件。
