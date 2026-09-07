# 批次 0 基线事实与执行报告

> **时点**：2026-09-07 12:03–12:15（第一部分）｜ 15:58–16:01（Codex 复核纠正处置：红灯拆分 / 证据归档 / 复跑全绿）
> **执行环境**：主仓库完成只读基线与红灯取证；乔在本机 PowerShell 创建隔离工作树；Codex 在权限恢复后于隔离工作树完成原型、文档、复验与提交
> **基线提交**：`170dcb4`；开发分支 `feature/management-glass`；工作树 `D:/Project/abyssal-reverie/worktrees/management-glass`
> **状态**：✅ 批次 0 完成。正式业务修复仍按批次 1/2 的 TDD 任务实施；本批只冻结基线、留存红灯证据并完成交互原型
> **未验收边界**：浏览器视觉与真实键盘检查仍为 BLOCKED；DOM 检查不替代正式应用、Rust 或 EXE 验收

---

## 1. git 状态实测（§2.4 步骤①，全部只读）

| 检查 | 实测结果 | 判定 |
|---|---|---|
| `git status -sb` | `## HEAD (no branch)`；未跟踪仅两份方案文档 | ✅ 与 §2.1 一致，无意外脏区 |
| `git branch -vv` | 仅 `feature-v1.1-local-prerequisites`（98dca6e）与 `main`（74c5937，`[origin/main: gone]`） | ✅ 与预期一致 |
| `git cat-file -t 170dcb4` | `commit` | ✅ 基线可达 |
| `git log --oneline -4 170dcb4` | `170dcb4` fmt 推迟裁定 → `9512439` 用户裁定记录 → `3331f51` 验收计划 → `95cf1c6` 门禁复跑报告 | ✅ 提交链完整 |
| `git worktree list` | 仅主树 `D:/Project/abyssal-reverie/Abyssal Reverie 170dcb4 (detached HEAD)` | ✅ `worktrees/management-glass` 未被占用 |

**结论**：步骤②（创建 worktree + 分支）的所有前置核对通过，可安全执行。

## 2. 门禁实测（任务 0.3；命令按 §5 矩阵原样执行）

| 门禁 | 命令 | 实测结果（2026-09-07 12:08–12:09） | §5 基线 | 判定 |
|---|---|---|---|---|
| Rust 全量 | `cargo test --manifest-path src-tauri/Cargo.toml --locked` | **113 passed / 0 failed / 2 ignored**（测试 1.66s，总 21s 含编译） | 113 / 0 / 2 | ✅ 无漂移 |
| 前端类型 | `pnpm verify` 第一步 `tsc --noEmit -p tsconfig.json` | **0 错误**（链式通过） | 0 | ✅ |
| 前端测试 | 同上 `vitest run` | **45 passed（10 文件）**，0 失败 | 45 / 10 | ✅ |
| 前端构建 | 同上 `vite build` | ✓ 16.14s，40 modules，dist 产物正常（index-Bo666SoQ.js 290.42 kB） | build ✓ | ✅ |

> **标注（Codex 复核纠正③）**：上表 45 passed 为**加入红灯测试文件之前**的实测。红灯文件短暂放入
> `src/` 发现范围期间该数字不适用；证据捕获后已将文件移出 vitest 发现范围（归档于
> `docs/superpowers/plans/evidence/2026-09-07-batch0/`），并于 15:59 **复跑 `pnpm verify` 确认恢复全绿**：
> tsc 0 错误 + **45 passed（10 文件）** + build ✓ 19.46s（产物同为 index-Bo666SoQ.js 290.42 kB，与冻结基线一致）。
> 迁移演练门禁（drill）属批次 1 起，批次 0 不跑；v4→v5 drill 待批次 1 建立后加入。

## 3. 冻结产物哈希对照（任务 0.4，红线 R3）

| 产物 | 实测 SHA-256（2026-09-07 12:10） | SHA256SUMS.txt 记录 | 判定 |
|---|---|---|---|
| `Abyssal-Reverie_1.3.0_portable.exe`（21,244,416 B） | `dd816b5b721b4be9e60d791895178336ae20becb25792f84c7b1585127cc66bd` | 一致 | ✅ 未变化 |
| `Abyssal-Reverie_1.3.0_windows-x64_setup.exe`（273,748,219 B） | `2a20c15be2c8a6f72fd11f9d0c6a100c8d08cc87e9a2c5851a7192030658723a` | 一致 | ✅ 未变化 |

两文件 mtime 均为 2026-09-06 12:09，本周期内未被触碰。「不得变化」对照基线确立。

## 4. schema 与迁移拓扑（db.rs 实测行号）

- `LATEST_SCHEMA_VERSION = 4`（db.rs:10）→ 本周期升 **v5**。
- **v2→v3 = `run_v3_migration`（db.rs:100，docstring db.rs:85 明写版本）**：重建 tasks + sessions（rename-first db.rs:134–137，旧表先改名防 FK 悬挂，逐行核验后才删旧表）；timer_state 增量 ALTER 加 tag_id / tag_name_snapshot（db.rs:236–237）。
- **v3→v4 = 增量**：新建 profiles / categories / projects / budget_history / focus_segments + tasks `ALTER ADD COLUMN` ×6（db.rs:696–702）、sessions 加 profile_id（703）。**不是重建先例**。
- v4 tasks NOT NULL 约束：`priority`（db.rs:146）、`tag_id REFERENCES tags(id) ON DELETE RESTRICT`（db.rs:148）→ v5 可空化必须重建表。
- **子表对 tasks 的 FK**：`sessions.task_id`（db.rs:53，SET NULL；索引 idx_sessions_task_id db.rs:64）、`budget_history.task_id`（db.rs:643，**NOT NULL + CASCADE**）、`focus_segments.task_id`（db.rs:655，SET NULL）。
- **timer_state**：`selected_task_id`（db.rs:39，**裸 TEXT 无 FK**，fk 检查不覆盖）；`tag_id`（db.rs:236，REFERENCES tags SET NULL）。
- **tasks 相关索引**：`idx_tasks_sort_order`（db.rs:32；v3 重建于 225）、`idx_tasks_tag_sort`（db.rs:226）、`idx_tasks_project`（db.rs:704）。
- 预算 `COALESCE(..., 25)` 两处**语义不同**（Codex 复核纠正④；db.rs:840–870 已实测核实）：
  - **db.rs:744 = 迁移基准**：`run_v4_migration` 内 `basis_minutes`（settings 无值时 fallback 25），用于 `UPDATE tasks SET target_seconds = pomodoro_target × basis × 60`，budget_source='migration'；
  - **db.rs:858 = 迁移预览的建议基准**：`preview_v4_migration`（db.rs:840 起）的 `suggested_focus_minutes`，是给用户看的**迁移建议值**，**不是默认 seed**；
  - 真正的默认 seed 是 `seed_defaults`（db.rs:872 起），直接用 `AppSettings::default()`，与以上两处无关。
  - 三处均**不随批次 2 任务 2.1 盲改**：2.1 只改新建提示 / 新建默认链路；迁移基准与预览建议值属迁移语义，需在批次 1 迁移设计中单独裁定。
- `preview_v4_migration`（db.rs:840）；R06 预览确认三命令：`preview_migration` / `confirm_migration` / `cancel_upgrade`（EXECUTION_STATUS.md §1 R06 行）。
- drill 先例：`drill_real_v3` 先 `fs::copy` 数据库三件套到临时目录再操作（源头零触碰）——批次 1 任务 1.10 的 v4→v5 drill 仿此。

## 5. 备份 / 导入函数清单（repository.rs，0.1 采集）

| 函数 | 行号 | 签名要点 |
|---|---|---|
| `export_data` | repository.rs:2793 | `-> Result<ExportBundle, CommandError>` |
| `parse_backup_text` | repository.rs:2896 | 文本 → `ExportBundle`；有版本头解析与 v1 回填测试覆盖 |
| `validate_import` | repository.rs:2938 | `bundle -> Result<(), _>`；`rejects_unknown_backup_version` / `import_rejects_a_non_abyssal_backup` 覆盖 |
| `import_data` | repository.rs:2983 | `(&mut Connection, &ExportBundle) -> Result<ImportSummary, _>`；失败保数据测试在位 |

批次 1 任务 1.7（备份兼容：空字段表达 + 旧版本解析保留）在此四处落刀。

## 6. AGENTS.md 现状盘点（0.8 纠错范围；本轮不改文件）

- **失实（删/改）**：标题 `figma-make-app`（L1）、"running inside Figma Make"（L3）、"Vite dev server is **already running** on $PORT (default 8443)"（L5–7）——本工程是 Tauri 2 桌面应用，开发走 `pnpm tauri dev`，无预运行服务器。
- **有效（保留）**：Tailwind CSS v4 + `@tailwindcss/vite`（L27/33）、oxfmt（L29）、Vite 8 / TS 5.7（L28）、结构条目——**所指 6 个文件（.mise.toml / src/main.tsx / src/index.css / src/App.tsx / vite.config.ts / package.json）实测全部存在**。
- **缺失（补）**：Tauri/pnpm 工程约定（`pnpm tauri dev`、`pnpm verify` 门禁、`src-tauri/` Rust 侧结构、cargo 测试命令）。
- 处置 = 批次 0 任务 0.8 局部改写，独立小提交。

## 7. 交接文档摘要（0.1 采集）

- `docs/plans/EXECUTION_STATUS.md`：ZCode 修正轮全记录（R01–R20 对照表、门禁记录、未验证项——锁屏/休眠未实测等 5 条已知限制）。
- `docs/plans/HANDOFF_WORKBUDDY.md`：6 节结构；§5 红线已并入执行计划红线表（R5/R6 同源）；§3.2 真实库演练命令为批次 1 drill 参照。

## 8. 批次 0 完成状态

| 任务 | 状态 | 前置 |
|---|---|---|
| 0.2 步骤②③ worktree + 分支 + 文档带入 | ✅ `feature/management-glass` @ `170dcb4`；三份文档与 evidence 已核对带入 | 乔在本机 PowerShell 创建，Codex 随后核验 |
| 0.5 / 0.6 红灯①② | ✅ **拆分证据已捕获并归档（见 §9）**；测试文件已移出 vitest 发现范围（evidence/ 内 .txt 存档，未删除证据），主工作树复跑 verify 恢复 45 passed 全绿 | 批次 1（0.6 转绿）/ 批次 2（0.5 转绿）在开发工作树内以正式用例红→绿 |
| 0.7 原型迁入 | ✅ `outputs/abyssal-prototype/`；补自定义本地日期选择、一次保存失败/重试、撤销删除标签冲突保护 | 原有 15 项 + 管理回归 2 组 + 新增 3 项均通过；视觉仍 BLOCKED |
| 0.8 AGENTS.md 局部纠错 | ✅ 已移除 Figma Make 与预运行服务器假设，补 Tauri、Rust 权威层和安全边界 | 提交 `91fb6a6` |
| 三个提交点（方案文档 → AGENTS 纠错 → 基线记录） | ✅ 前两项 `50401cc` / `91fb6a6`；本报告、证据及原型为第三提交 | 每个提交前工作区门禁为绿 |

**步骤④扩展说明**：三份文档与两份红灯证据均已保留在开发工作树；主仓库原件没有删除。

---

## 9. 红灯证据（任务 0.5 / 0.6；12:26 首轮捕获 → 15:58 按 Codex 复核拆分重跑）

**载体与处置（Codex 复核纠正③）**：测试文件已移出 vitest 测试发现范围，证据归档于
`docs/superpowers/plans/evidence/2026-09-07-batch0/`（两份均保留，未删除证据）：

| 归档文件 | 内容 |
|---|---|
| `TasksPanel.batch0-red.test.tsx.txt` | 拆分后的 6 条独立用例源码（批次 1/2 红→绿时的断言设计参照） |
| `2026-09-07-batch0-red-vitest-output.txt` | 拆分重跑的完整 vitest 输出 |

**拆分原则（Codex 复核纠正①）**：同一 it 内前一个断言失败会遮住后一个——首轮 2 failed 时，
红灯①的预算断言与红灯②的 priority 断言**从未执行到**。已拆为 6 条独立用例，
重跑结果 **3 failed / 3 passed（6 条）**，与预期完全一致：

| 用例 | 断言 | 结果 | 归因 |
|---|---|---|---|
| 0.5-a sanity | saveSettings(1分钟) 返回 settings=1 / timer=60s | ✅ 绿 | 网关契约前提独立验证成立 |
| 0.5-b 提示 | 提示应为 `个番茄 = 1 分钟` | 🔴 红 | TasksPanel.tsx:87 恒 25，不读设置 |
| 0.5-c 预算 | 创建后 targetSeconds 应为 60 | ✅ 绿 | 该模拟网关预算层（FakeAppGateway.ts:490）本按设置派生 |
| 0.6-a projectId | payload.projectId 为 undefined | ✅ 绿 | 本就未传 |
| 0.6-b tagId | payload.tagId 应为 null | 🔴 红 | TasksPanel.tsx:85 自动填「其他」 |
| 0.6-c priority | payload.priority 应为 null | 🔴 红 | TasksPanel.tsx:69 默认 'med'（首轮被 0.6-b 遮蔽，本轮首次暴露） |

### 三条红灯原始输出（各自独立暴露）

红灯①-显示层（0.5-b，App 级）：

```
FAIL … > 0.5-b 设置 1 分钟时，创建面板换算提示应显示「个番茄 = 1 分钟」
AssertionError: expected '个番茄 = 25 分钟' to be '个番茄 = 1 分钟'
  ❯ TasksPanel.batch0-red.test.tsx:64:30
```

红灯②-标签（0.6-b，TasksPanel 级）：

```
FAIL … > 0.6-b 仅标题创建时 payload.tagId 应为 null
AssertionError: expected 'system-other' to be null
  ❯ TasksPanel.batch0-red.test.tsx:153:35
```

红灯②-优先级（0.6-c，TasksPanel 级）：

```
FAIL … > 0.6-c 仅标题创建时 payload.priority 应为 null
AssertionError: expected 'med' to be null
  ❯ TasksPanel.batch0-red.test.tsx:160:38
```

### 结论边界（Codex 复核纠正②——修正首轮过宽表述）

- **FakeAppGateway 的定位**：它是**前端集成测试的模拟网关**（`src/test/FakeAppGateway.ts`），
  **不是 Rust 权威网关**。首轮"UI 不受影响"的结论**过宽**，现修正为：
  - 0.5-b 证明的是**显示层**不读已保存设置（恒 25）；
  - 0.5-c 证明的是**该模拟网关**创建任务时预算按设置派生；
  - **不能**由此证明真实数据库 / Rust 侧预算正确，也**不能**证明"后续状态重读不受影响"——
    App 后续重新读取状态时**仍可能取得旧计时值**（尤其叠加 FakeAppGateway.saveSettings
    不写回 `this.timer` 的基建缺陷）。真实链路验证属批次 1/2 的 Rust 契约测试覆盖范围。
- 红灯证据仅证明**缺陷存在与归位**，不构成对修复方案的验证。

### 红灯过程中新发现（转交批次 1 / 2 处置，不扩大本批范围）

1. **第二处写死填充点**：`FakeAppGateway.ts:495` `tagId: 'system-other'` —— 模拟网关**完全忽略**
   `input.tagId` 直接写死保底标签。批次 1 契约改造（NullablePatch）必须连同此处与真实 Rust 侧
   （repository.rs `new_tasks_default_to_fallback_tag` 现行为）一并对齐。
2. **FakeAppGateway.saveSettings 保真度缺陷**（FakeAppGateway.ts:548–557）：计算了刷新后的 idle
   计时器但**从未写回 `this.timer`**，仅放入返回值——App 即时用返回值渲染故当前界面正常，但网关
   内部状态（`currentTimer`、后续 `bootstrap().timer`）保持陈旧时长，**后续状态重读仍可能取得旧
   计时值**。与 Rust `save_settings_refreshes_idle_timer_durations` 行为不一致。属测试基建缺陷，
   随批次 1/2 契约工作修正，不阻塞批次 0。

---

## 10. 隔离工作树最终复验（2026-09-07 16:39–16:44）

| 检查 | 实测结果 | 判定 |
|---|---|---|
| `pnpm install --frozen-lockfile --offline` | 锁文件未变化；135 个包全部从本地缓存复用 | ✅ |
| `pnpm verify` | tsc 0 错误；Vitest **45 passed（10 文件）**；Vite **40 modules** 构建成功 | ✅ |
| Rust 全量 | **113 passed / 0 failed / 2 ignored**；首次工作树编译 3m01s，测试 1.39s | ✅ |
| 原型 `verify.cjs` | **15 checks passed** | ✅ DOM 级 |
| 原型 `verify-management.cjs` | 导航/可空字段/标签/归档/活动任务行为两组通过 | ✅ DOM 级 |
| 原型 `verify-batch0.cjs` | 日历、失败重试、撤销冲突 **3 passed / 0 failed** | ✅ DOM 级 |
| 新增脚本语法 | `node --check batch0-enhancements.js` 退出 0 | ✅ |
| v1.3.0 portable SHA-256 | `dd816b5b721b4be9e60d791895178336ae20becb25792f84c7b1585127cc66bd` | ✅ 未变化 |
| v1.3.0 setup SHA-256 | `2a20c15be2c8a6f72fd11f9d0c6a100c8d08cc87e9a2c5851a7192030658723a` | ✅ 未变化 |

Rust 构建仍报告 8 条既有 warning（未使用导入、测试变量无需 `mut`、未使用函数等）；本批没有修改 Rust，且按 R7 不混入格式化或无关清理。前端构建报告 Node `module.register()` 弃用警告，门禁仍成功；后续依赖维护单独处理。

### 批次结论

- 批次 0 的八项任务均已有证据，基线未发生业务漂移。
- 三条业务缺陷仍保持红灯状态并已归档：时长提示恒 25、默认标签、默认优先级；分别交由批次 1/2 转绿。
- HTML 原型已补齐本批要求的三种交互，但真实浏览器视觉、实际键盘和正式 Tauri 行为仍未验收，不得以 DOM 结果替代。
- 允许进入批次 1；进入前继续遵守 v4 数据副本、迁移预览确认、数据守恒与真实库零故障注入边界。
