# WorkBuddy 交接文档 — Abyssal Reverie v1.3.0 剩余工作

> 交接日期：2026-09-06。交接人：ZCode。仓库：`D:\Project\abyssal-reverie\Abyssal Reverie`。
> 目的：把剩余验收/发版工作交给 WorkBuddy 执行一轮开发。本文自包含，可独立阅读。

## 1. 当前仓库状态（务必先核对）

| 项 | 值 |
|---|---|
| 当前分支 | `feature/v1.3-mini-window`（**未合并 main、未打 tag、未发版**） |
| 分支栈 | `main`(74c5937) ← `feature/v1.1.2-consistency` ← `feature/v1.2-tasks-projects` ← `feature/v1.3-mini-window` |
| 版本号 | `package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` = **1.3.0** |
| 最新提交 | `91cc608`（docs: 恢复点）之后的发布构建与演练未提交（产物在 `release/v1.3.0/`，release/ 是否入库遵循仓库现状——v1.1.1 时曾入库） |
| 门禁状态 | `cargo test` **113 passed + 1 ignored（真实库演练，见 §3.2）**；`vitest` **45 passed**；`tsc` 0 错误；NSIS + 便携 EXE 构建成功 |
| 真实数据库 | `%APPDATA%\com.abyssalreverie.focus\abyssal-reverie.sqlite` 仍为 **schema v3（未迁移）**；三件套完整备份在 `release/v1.3.0/rehearsal/live-backup/` |
| 运行进程 | 已全部退出（单实例槽位干净），可直接启动 EXE |

**开工前核对**（在仓库根目录）：

```powershell
git status --short          # 应干净
git log -3 --oneline        # 应看到 91cc608 为起点之后的提交
cargo test --manifest-path src-tauri/Cargo.toml   # 113 passed, 1 ignored
pnpm verify                 # vitest 45 + tsc 0 + vite build
```

## 2. 已完成内容（不要重做）

v1.1.2（一致性修复）→ v1.2（类别/项目/任务 + 片段账本 + 预算快照 + complete_task_now）→ v1.3（Rust 后台结算 + 悬浮小窗 + 完成并保存/撤销 + 小鱼伙伴）全部实现；随后按修订计划完成 R01–R08、R12、P112-03、P112-06、R06（迁移预览协议）的符合性修正。

**明确跳过（用户指示）**：登录、云端备份、多设备同步（PLAN-140/150）——接口未就绪，整体不做。植树/生命树（原第 5 项）范围外。排版 G2/G3（字体/字号层级）用户亲自处理，不要动。

## 3. 剩余需要完成的业务（按优先级）

### 3.1 Windows 实机 GUI 验收（发版门禁，核心剩余工作）

用 `release/v1.3.0/Abyssal-Reverie_1.3.0_portable.exe`（SHA-256 见 `release/v1.3.0/SHA256SUMS.txt`）执行。已验证：启动正常、真实库会进入迁移预览屏（ZCode 已实际启动确认窗口渲染正确）。

| # | 场景 | 预期 |
|---|---|---|
| A1 | 真实库迁移（GUI 路径） | 启动 → 迁移预览屏显示任务/会话/项目数（真实库：0 任务、20 会话、4 条合格）；预算基准可改（真实设置当前为 1 分钟/番茄，**建议在预览屏改为 25 再确认**）；「取消并退出」= 进程退出、库仍 v3；「确认升级」= 迁移成功进入主界面 |
| A2 | 迁移后对账（GUI） | 统计页/活动栏数据与迁移前一致（专注航线 4 条合格记录、总 246s 专注）；任务/项目面板正常 |
| A3 | 创建反馈 | 中文输入法候选回车不提交；创建失败（可断 DB 模拟）保留草稿；连点只建一个 |
| A4 | 运行中切换任务 | A 运行 10 分钟选 B → 主钟不重置，A/B 时间分别记账，界面高亮与后台一致 |
| A5 | 完成任务并保存 | 40 分钟轮第 15 分钟在小窗/任务详情完成 → 任务完成、15 分钟保存、主钟暂停剩 25；8 秒内可撤销；不足 30 秒提示"待本轮达标后计入" |
| A6 | 小窗矩阵 | 托盘「显示/隐藏小窗」；主窗最小化小窗可见；关闭小窗计时不断；两窗并发暂停只执行一次；折叠/置顶/纯数字模式 |
| A7 | 休眠/锁屏 | 休眠恢复后计时状态合理（当前实现=退出冻结+恢复暂停）；**需实测并如实记录**（R11 要求） |
| A8 | 多屏/DPI | 拔外接屏后小窗回可见区；100/150/200% 缩放不裁切（至少 100% 与系统实际值） |
| A9 | 断网启动 | 无网络环境 EXE 独立启动（资源已嵌入，`custom-protocol` 已确认在 Cargo.toml） |
| A10 | 主窗无响应结算 | 运行中让主窗忙（如断点/挂起）→ 到期后 Rust 照样落库一次，重开状态正确 |
| A11 | 长跑 | 24 小时运行内存/CPU 无异常增长（可缩短为 1–2 小时基线记录，注明时长） |

注意：A1 演练后若真实库已迁移到 v4，**保留迁移后状态**即可（这正是升级目标）；出问题时用 `rehearsal/live-backup/` 三件套恢复（关闭应用后整体复制回去）。

### 3.2 真实库演练命令（测试级，已通过，可复跑）

```powershell
cd src-tauri
cargo test drill_real_v3 -- --ignored --nocapture   # v1.3.0 演练：取消路径 + 确认路径 + 对账
cargo test drill_real -- --ignored --nocapture      # 旧 v1.0.0→v3 演练（历史保留）
```

最近一次结果：`preview: tasks=0 sessions=20 projects_to_create=[] general_tasks=0 suggested_basis=1min`；确认路径 `legacy_segments=4, session_seconds=246s == segment_ms/1000, fk_violations=0`。

### 3.3 验收通过后：合并与发版

1. `git checkout main && git merge --no-ff feature/v1.1.2-consistency`（三分支为栈式，也可直接合并栈顶 `feature/v1.3-mini-window`——**栈顶包含全部提交，直接合并它即可**）；
2. 更新 `README.md`（当前版本、产物名 1.3.0、SHA-256）；`docs/CHANGELOG.md` 已有 v1.3.0 条目（标注"尚未发布"，发版时去掉）；`docs/REGRESSION_REPORT.md` 补验收结果；
3. 打 tag `v1.3.0`，GitHub Release 附便携包 + 安装器（沿用 v1.1.1 模式：portable zip + setup，README 有打包命令）；
4. 便携包建议压缩为 zip 并登记 SHA-256（v1.1.1 模式）。

### 3.4 发版后（可选，用户醒后确认）

- 排版 G2/G3（用户亲自拍板，勿动）。
- PLAN-140/150 等接口就绪后按其 §1「外部门」流程启动。

### 3.5 已知瑕疵（低优先级，可顺手修）

- `repository.rs`/`db.rs` 混合行尾，建议单独一次 `cargo fmt` 提交（勿混入功能提交）。
- 音效/通知由主窗前端播放（单实例下无双播问题；R14 完全后端化列为后续）。
- 设置页的番茄节奏预设（G4）未实现——可选项，不阻塞。

## 4. 参考文件清单

### 必读（顺序即依赖）

| 文件 | 作用 |
|---|---|
| `docs/plans/ABYSSAL_REVERIE_ZCODE_PLAN.md` | **主计划**（修订版）：PLAN-INDEX 全局约束 + R01–R20 修订表 + 五个版本计划；业务语义以此为准 |
| `docs/plans/EXECUTION_STATUS.md` | 执行状态 + 已确认恢复点（§6）+ 门禁记录 + 未验证项 |
| `docs/ACCEPTANCE_GUIDE.md` | v1.1 验收指南（GUI 矩阵基础，叠加 §3.1 新场景） |
| `docs/CHANGELOG.md` | v1.1.2/v1.2.0/v1.3.0 三个版本的用户可见变更（v1.3.0 标注未发布） |
| `docs/REGRESSION_REPORT.md` | 历次回归记录（发版前补 v1.3.0 实机结果） |

### 背景（可选）

| 文件 | 作用 |
|---|---|
| `docs/plans/EXECUTION_LOG.md` | 夜间自主执行全记录（含决策 X-1~X-10） |
| `docs/plans/V1.1.2_CONSISTENCY_PLAN.md` 等 5 份 | 早期分版计划（已被 ZCODE_PLAN 修订取代，仅作参考） |
| `docs/KNOWN_ISSUES.md` | 已知问题（v1.1.1 前） |
| `THIRD_PARTY_NOTICES.md` | 第三方素材许可（Kenney Fish Pack CC0 等） |

### 关键源码入口（改动热点）

| 文件 | 职责 |
|---|---|
| `src-tauri/src/repository.rs` | 计时状态机 + 片段账本 + 统计（稳定 ID 汇总）+ complete_task_now/undo |
| `src-tauri/src/db.rs` | 迁移（v4 + R06 预览/参数化）+ 守卫（副本探测/损坏分类/验证备份）+ 真实库演练测试 |
| `src-tauri/src/lib.rs` | 启动序列（prep 模式分支）+ ticker 后台结算 + 命令注册 |
| `src-tauri/src/commands.rs` | 全部 Tauri 命令（含 preview/confirm/cancel_upgrade 门禁） |
| `src/App.tsx` | 主窗状态编排（R02 响应合并/revision 守卫/统计序号） |
| `src/components/MigrationPrepScreen.tsx` | 迁移预览屏 |
| `src/mini/MiniWindow.tsx` + `mini.html` | 悬浮小窗（独立入口） |
| `src/features/tasks/TasksPanel.tsx` + `TaskDetailDialog.tsx` + `ProjectManager.tsx` | 任务/项目 UI |
| `src/test/FakeAppGateway.ts` | 测试用内存网关（与 Rust 语义对齐） |

### 门禁命令（在仓库根目录）

```powershell
pnpm install --frozen-lockfile
pnpm verify                                                            # tsc + vitest + vite build
cargo test --manifest-path src-tauri/Cargo.toml --locked               # 113 passed, 1 ignored
pnpm tauri build                                                       # NSIS
cargo test --manifest-path src-tauri/Cargo.toml drill_real_v3 -- --ignored --nocapture  # 真实库演练
```

## 5. 红线（继承自主计划，违反即返工）

1. 不得提前实现登录/云/同步（PLAN-140/150）、植树、项目分组。
2. 不动排版 G2/G3（字体、字号层级）——用户亲自处理。
3. 计时/资格/结算规则只在 Rust；30 秒规则只存在于 Rust。
4. 迁移必须走「预览确认」协议（R06）；取消不改数据。
5. `user_version`、备份 `formatVersion` 独立；未来库拒绝写入；迁移失败不触发损坏重建。
6. 合并/打 tag/发 Release 前，把验收结果写入 REGRESSION_REPORT（不得用计划文字冒充实测）。

## 6. 追记（2026-09-06 晚，WorkBuddy）

- 16:48:02 真实库已完成 v3→v4 迁移，**经用户裁定确认为本人操作**（启动应用并在迁移预览屏点「确认升级」），
  对账零丢失（合格会话 4 条 / 246s，legacy 段 4 条 = 246000ms 精确 ×1000，integrity ok）。
  §1「真实数据库仍为 schema v3」自此刻起过时。
- 用户裁定：**真实库保留 v4**。§3.1 的 A1/A2（真实库迁移 GUI 路径 + 迁移后对账）视为已由用户实机完成；
  剩余实机矩阵为 A3–A11。
- 演练测试 `drill_real_v3` 的数据源由「真实库（须为 v3）」改为「rehearsal/live-backup/ 的 v3 快照」——
  真实库已是 v4，测试不得依赖可变真实状态；恒真断言 `budgets >= tasks.min(budgets)` 修正为
  `assert_eq!(budgets, tasks)`（迁移写入语义 = 每任务恰一行，db.rs `INSERT … SELECT … FROM tasks`）。
