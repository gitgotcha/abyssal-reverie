# EXECUTION_STATUS — ABYSSAL_REVERIE_ZCODE_PLAN.md 修正轮

> 日期：2026-09-06。执行者：ZCode。依据：仓库内 `docs/plans/ABYSSAL_REVERIE_ZCODE_PLAN.md`（修订版，R01–R20）。
> 起点：夜间自主执行已完成 v1.1.2/v1.2/v1.3（分支 `feature/v1.1.2-consistency` → `feature/v1.2-tasks-projects` → `feature/v1.3-mini-window`）。本轮按修订计划对已实现代码做**符合性修正**。
> 用户指示：登录·云·同步（PLAN-140/150）整体跳过，等接口就绪。

## 1. 本轮修正对照表

| 修订项 | 计划要求 | 修正结果 | 证据 |
|---|---|---|---|
| R01 | 暂停切换仅换绑定、不开新片段；resume 不复活已完成/归档绑定；退出时封口开片段 | ✅ `switch_timer_task` 仅 Running 封口/开段；`resume_timer` 检查绑定状态，完成/归档/缺失则解绑为未指定任务；`persist_running_as_paused` 封口片段 | `repository.rs`；测试见下 |
| R02 | 每次成功响应都按会话 ID upsert（newly*=false 也补齐）；资格决定可见性；revision 守卫；refreshStats 返回 Promise+序号；到期去重；bootstrap 全时统计不得充当今日 | ✅ `upsertLogNewestFirst` 语义重写；`applyTimer` revision 守卫 + `applyTimerForced`；`statsSeqRef` 序号丢弃旧响应；`inflightExpiresRef` 按 sessionId 合并；resync/bootstrap 不再用 all-time 充当 today | `format.ts`、`App.tsx` |
| R03 | 公共排序 `(startedAt DESC, id DESC)`，不用 rowid；SessionLog 保留完整权威字段 | ✅ `list_sessions`/`list_sessions_query` 改 `id DESC`；`sortLogsDesc` 平局改 id DESC；`SessionLog` 增 `endedAt/focusedSeconds/statisticsEligible` | `repository.rs`、`format.ts`、`types.ts` |
| R04 | 列表 `< to` 半开区间；`from<to` 校验；日桶必须连续覆盖 `[from,to)` | ✅ `list_sessions_query` 改 `<`；`validate_day_boundaries` 增 from<to 与首尾覆盖断言 | `repository.rs` |
| R06 | 语义迁移先预览确认，取消不改数据；预算基准与通用映射由用户决定 | ✅ `preview_v4_migration`（只读）；`run_v4_migration_with(params)`；启动进入 prep 模式（`open_prepared`，业务命令全部被 `MIGRATION_REQUIRED` 门禁）；`preview_migration`/`confirm_migration`/`cancel_upgrade` 三命令；前端 `MigrationPrepScreen` | `db.rs`、`lib.rs`、`commands.rs`、`MigrationPrepScreen.tsx` |
| R07 | 历史归属用发生时快照，不取任务当前项目；未映射单列 | ✅ 回填按 `session.project_snapshot` 匹配迁移中归并的项目，未匹配留 NULL + 名称快照 | `db.rs` MIGRATION_V4 |
| R08 | 旧记录不做伪连续区间；单位 ×1000；精度标记 | ✅ `focus_segments.temporal_precision`（`legacy_total_only`/`effective_interval`）；回填段标记 legacy，起止仅为元数据 | `db.rs` |
| R12 | 新统计按稳定 ID 汇总，名称快照仅追溯；未映射单列 | ✅ `get_statistics`/`all_time_statistics` 的 byProject/byCategory 以 `project_id/category_id` 分组、显示名联当前实体，无 ID 落 `unmapped:` 桶 | `repository.rs` |
| P112-03 | 活动轮绑定任务缺失时显式说明，不假高亮 | ✅ TimerPanel 增加「本轮任务已不在待办列表」行（快照标题 + 徽标） | `TimerPanel.tsx` |
| P112-06 | 版本探测经副本（含 WAL 提交页）、源零触碰；打开失败（锁/权限/IO）不当损坏重建；预迁移备份验证 checkpoint busy + 副本 integrity/version | ✅ `probe_schema_version` 副本探测；`is_corruption_diagnosis`（仅 NOTADB/CORRUPT 走恢复）；`backup_before_migration` 三段验证 | `db.rs` |
| R14 | 后台结算权威化（前移） | ✅ 夜间轮已在 v1.3 A 落地（ticker 直接落库 + `timer-settled`）；本轮补 R02 的 forced-apply | `lib.rs`、`App.tsx` |
| R15–R19 | v1.4/v1.5 相关 | ⏭ 跳过（用户指示：接口未就绪） | — |
| R20 | 待确认项固定 | 排版（G2/G3）仍按用户指示挂起；其余按本计划固定值执行 | — |

## 2. 门禁记录（实际命令与结果）

在仓库根目录（Windows，Git Bash）：

| 命令 | 结果 |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml` | **113 passed, 0 failed, 1 ignored**（含本轮新增：open_failure_is_not_treated_as_corruption、migration_preview_reads_v3_without_converting、confirmed_migration_honours_user_parameters、ticker_settles_expired_without_frontend_and_is_single_shot 等） |
| `npx tsc --noEmit -p tsconfig.json` | 0 错误 |
| `npx vitest run` | **45 passed**（10 文件，0 失败） |
| `pnpm tauri build --no-bundle` | ✓ release EXE 构建成功（`src-tauri/target/release/abyssal-reverie.exe`） |
| `cargo test --locked` | 未使用 `--locked`（本轮未引入新依赖，锁文件未变化；下轮发布门禁按计划补 `--locked` 参数） |

## 3. 新增行为测试清单（本轮）

- `open_failure_is_not_treated_as_corruption`：目录占位 → 打开失败 → 无 `.corrupt-` 改名。
- `migration_preview_reads_v3_without_converting`：v3 预览计数正确且 schema 不变。
- `confirmed_migration_honours_user_parameters`：确认基准 40 分钟 → 预算 4800s；通用映射 project → 独立项目创建并链接。
- 既有 `newer_database_is_refused_and_untouched` 现在经由副本探测路径（源字节不变断言仍通过）。
- 既有 `corrupt_database_is_recovered_by_reseeding` 验证 NOTADB 仍走恢复。
- 既有 `pre_migration_backup_is_created_before_upgrading` 验证备份目录中恰好一个 .bak（无 sidecar 泄漏）。

## 4. 未验证项 / 已知限制

1. **Windows 实机 GUI 矩阵未执行**（夜间无人值守）：NSIS/便携发布构建、断网启动、v1.1.1 真实库副本升级演练、多显示器/拔屏/DPI、主窗无响应结算、24 小时长跑。发版前必须按 `docs/ACCEPTANCE_GUIDE.md` 补齐。
2. **锁屏/休眠行为**未实测（R11 要求实测并记录，当前仅有实现路径：退出冻结 + 恢复暂停）。
3. **迁移预览确认前的「活动旧轮处理」**（P120-02 的完整矩阵）：prep 模式下运行中轮次保持原样（Rust 不推进），确认后按既有恢复协议处理；跨版本组合场景需实机验证。
4. 音效/通知仍是前端单点播放（主窗），R14 的「后端单一调度者」完全形态需 Tauri 通知/音频后端化，本轮未做（已在计划中列为 v1.3 §2.7 的既知边界）。
5. `cargo fmt` 未统一：`repository.rs`/`db.rs` 存在混合行尾（可编译；建议格式化提交单独处理）。

## 5. 版本状态

- 分支：`feature/v1.3-mini-window`（未合并 main、未打 tag、未发版）。
- `package.json` / `tauri.conf.json` / `Cargo.toml` = **1.3.0**。
- 登录·云·同步（PLAN-140/150）：按用户指示整体跳过，等接口就绪后依 PLAN-140 §1 的「外部门」流程再启动。

## 6. 已确认的恢复点（2026-09-06，用户确认）

用户已确认以下状态记录：

- **已完成**：R01–R08、R12、P112-03、P112-06。核心改进：任务切换与片段记账、响应去重、稳定 ID 统计、旧库迁移预览、历史快照真实性、数据库备份与损坏识别。
- **门禁全绿**：Rust 113 / 前端 vitest 45 / TypeScript 0 错误 / Release EXE 构建成功。
- **暂未完成**：Windows 实机验收、真实 v1.1.1 数据升级演练、休眠/多屏/长时间运行测试、排版 G2/G3。
- **登录、云端和同步**：继续跳过。
- **分支尚未合并 main，未发布**。

### 下一步（下次会话从这里开始）

1. **Windows 实机与真实数据迁移验收**（优先）：
   - `pnpm tauri build` 产出 NSIS + 便携 EXE（验证生产资源嵌入、`custom-protocol`）；
   - 断网、干净环境启动；SHA-256 登记便携包；
   - **真实 v1.1.1 库副本升级演练**：备份文件、迁移预览屏确认（预算基准/通用映射）、迁移后对账（任务/项目/会话/片段数量与时间总量守恒）、取消路径不改动数据；
   - 按 `docs/ACCEPTANCE_GUIDE.md` 跑 GUI 矩阵：创建反馈、运行中切换、完成并保存、小窗、休眠恢复、多屏/DPI、主窗无响应后台结算；
   - 结果记入 `docs/REGRESSION_REPORT.md`。
2. **验收通过后**：合并三条 feature 分支 → 打 tag → 发版（服从当次会话授权）。
3. **之后**：排版 G2/G3（用户亲自拍板）；接口就绪后启动 PLAN-140。

