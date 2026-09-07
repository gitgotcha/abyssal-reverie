# 执行计划：管理体验与全局磨砂

> **对应方案**：`docs/superpowers/plans/2026-09-07-management-glass-development.md`（下称「方案」）
> **本文件状态**：✅ v2.1 —— Codex 第二轮复核通过：**允许开展批次 0**；本轮五项修订随批次 0 入库；迁移依赖清单（1.3）已补齐，批次 1 前置在文档层面满足
> **边界保留**：不跑全仓 fmt ｜ 不覆盖 v1.3.0 ｜ 真实库不做故障注入 ｜ 多屏缺设备记 BLOCKED
> **作者**：WorkBuddy（Buddy）｜ **日期**：2026-09-07（v2.1）
> **职责边界**：本文件把方案第五～七节转化为可核对的任务序列与门禁。不替代方案，与方案冲突时以方案为准并停下报告。

## 修订记录

**v2.1（2026-09-07，按 Codex 第二轮复核修订；结论：允许开展批次 0，本版修订随批次 0 入库，迁移清单补齐后方可进入批次 1）——修订前全部实测核实：**

1. **依赖表保护清单补齐关键字段（任务 1.3）**：子表外键 `sessions.task_id`（db.rs:53，SET NULL）、`budget_history.task_id`（db.rs:643，NOT NULL + **CASCADE**——父行丢失会被级联静默删除）、`focus_segments.task_id`（db.rs:655，SET NULL）；`sessions.tag_id` 不能替代 `sessions.task_id` 的保护；`timer_state.selected_task_id`（db.rs:39，**裸 TEXT 无 FK**）与 `timer_state.tag_id`（db.rs:236）绑定显式保留；索引补 `idx_tasks_sort_order`（db.rs:32/225）、`idx_tasks_project`（db.rs:704）。**新增数据守恒对账**：`foreign_key_check` 通过 ≠ 历史数据未被级联删除或置空——迁移前后逐表对账记录数、task 关联分布、总投入毫秒数。
2. **迁移先例更正（任务 1.2）**：`run_v3_migration`（db.rs:100，文档串 db.rs:85 明写 v2→v3）对应 **v2→v3 重建**，非 v3→v4；v3→v4 实为增量 ALTER（db.rs:696–702 ADD COLUMN）。v2 版「v3→v4 即 rebuild，db.rs:89」行号与版本双错，已更正；重建方向不变。
3. **跨天逾期刷新机制明确（任务 4.5）**：不能依赖「渲染时重算」（页面静止时整夜可能不重渲染）——本地午夜定时触发刷新，休眠恢复、窗口重新激活时补检查；验收新增「页面不操作跨过午夜」「休眠跨天后恢复」两场景。
4. **§2.4 前置命令改 PowerShell 可粘贴形式**：Bash 反斜杠续行与多源 `cp` 不能直接执行；补目标目录 / 分支存在性查重、复制后两份文档 `Get-FileHash` 核对一致、主工作树原件保留。
5. **§7 停止条件排除计划内情况**：两份未跟踪方案文档、TDD 预期红灯、已允许的视觉验收 BLOCKED 均不触发停工，避免批次 0 被自身正常步骤卡住。
6. **转述更正**：「重命名后检索索引失效」实际落点为批次 3 任务 3.3（计划文件本身无误，错误出自 v2 修订移交报告的转述）。

**v2（2026-09-07，按 Codex 审核意见修订；六项必改 + 细节补充全部落实）：**

1. **「写死 25」由"未复现"改为已定位**：`TasksPanel.tsx:87` `projects.length >= 0 ? 25 : 25`（247 行用于换算提示）。v1 漏查原因：grep 模式只搜乘法表达式（`25*60`/`1500`），未覆盖裸字面量三元。红灯①改为「设置 1 分钟，提示与实际创建预算一致」，无需用户确认。同文件 85 行 `effectiveTagId = formTagId || fallbackId` 即方案禁止的"自动填『其他』标签"现行实现，列为批次 1 红灯证据。
2. **AGENTS.md 由"整体过时"改为局部失实**：Tailwind v4 / oxfmt 实际存在且已接入构建与样式；失实的只是 Figma Make 环境描述与"服务器已运行"假设。处置 = 局部纠错 + 补 Tauri 约定，保留有效约定，独立小提交。
3. **原型已定位**：`C:/Users/27846/Documents/Codex/2026-09-05/superpowers-plugin-superpowers-openai-curated-remote/outputs/abyssal-prototype`（仓库外，来源为绝对路径）。复用不重建；迁入开发工作树后补日历等交互；视觉验收保持「未完成」。
4. **批次 1 迁移方案具体化**：v4 的 `tasks.tag_id` / `priority` 为 **NOT NULL**（db.rs:146/148），ALTER 无法解除既有列约束 → **重建 tasks 表**（事务内 + 依赖表保护 + 逐条复制旧值）；**区分 v3→v5 与 v4→v5** 两条路径，v4 用户预览不得再展示旧项目映射或重算旧预算（`preview_v4_migration` db.rs:840 需按版本分派）；补预览 / 取消 / 确认 / 失败回滚 / 重复打开测试矩阵。
5. **红灯提交与"每批全绿"的矛盾消除**：红灯只记证据、不落持久提交；红→绿在修复批内部完成；**提交全绿**；任务引用所属批次 SHA，不强求一任务一提交。
6. **§2.4 分支恢复命令撤回**：detached HEAD 不构成沙箱回滚的证据（那是 WorkBuddy 沙箱内的观察，未在本机验证）。改为先核对引用与提交可达性，再从确认基线建开发分支 + 隔离工作树；不强制恢复其他历史分支；两份未跟踪方案文档必须带入新工作树。

**补充进任务表的原方案细节**：关闭应用提醒未保存草稿（窗口关闭 + 托盘退出）、跨天刷新逾期标记（今天不逾期、完成不警告）、详情快捷修改失败回退显示并保留可重试值、从项目/标签进入任务返回原位置、重命名后搜索索引失效、显式清空规则覆盖全部可选字段（优先级/日期等，不只项目标签）——分别落点见批次 1/3/4 表内标注。

---

## 0. 使用约定

- 状态标记：⬜ 未开始 ｜ 🔄 进行中 ｜ ✅ 完成（有证据）｜ ⛔ 阻塞（原因必须写明）
- **每批完成定义（DoD）**：该批全部任务 ✅ + 门禁矩阵全绿 + 独立提交（含 SHA）+ 批报告
  （变更文件清单、测试数量实际值、遗留问题）。
- **TDD 红绿循环**：每个行为任务先写失败用例（🔴 红灯），确认失败原因正确，再实现（🟢 绿灯）。
  禁止先实现后补测试，禁止为过测试放宽断言。
- **红灯证据的留存方式**：红灯的失败输出记录在批报告 / 执行日志中；红→绿在**修复批内部**完成，
  **不把失败测试作为持久提交**——每个提交点必须全绿（审核意见第 5 条）。
- **每任务上报**：变更文件、测试数量（实际通过数，不是估算）、所属批次 commit SHA（不强求一任务一提交）。
- 测试数量预期（各批所列「预计新增」）仅为规划参考，**实际以批报告实测为准，不得用预期冒充实测**。

---

## 1. 全局红线（继承方案 §一 + 既有裁定，全程有效）

| # | 红线 | 来源 |
|---|---|---|
| R1 | 原主题背景、配色、专注页与小窗结构保留；磨砂是统一表层，不是重做品牌 | 方案 §一 |
| R2 | 旧数据不擅自清空：迁移逐条复制旧值，禁止批量清空历史「其他」标签或「中」优先级 | 方案 §一/批次1 |
| R3 | 不覆盖 `release/v1.3.0/` 冻结产物；新候选放独立目录、独立哈希 | 方案 §一 |
| R4 | 不操作真实库做故障注入；迁移/恢复一律先在**副本**验证（R06 预览确认协议） | 方案 §一 + HANDOFF §5 |
| R5 | 计时、预算、30 秒资格、幂等结算、revision 守卫只在 Rust；前端不得复制第二套 | 方案 §2.2 + HANDOFF §5 |
| R6 | `user_version`（schema）与备份 `formatVersion` 各自独立管理，不混用 | HANDOFF §5 |
| R7 | 不执行全仓 `cargo fmt`（2026-09-06 用户裁定：推迟为独立维护提交） | 裁定豁免 3 |
| R8 | 高风险动作（push / merge / tag / 发版 / 真实库写入）必须显式授权后执行 | 用户常设规则 |
| R9 | 偏离计划（脏 worktree、冲突、回归、复现失败）立即 halt-and-report，不 reset / 不猜 | 用户常设规则 |
| R10 | 不加入登录、云同步、植树、提醒通知、重复任务、多标签、项目分组新层级 | 方案 §一 |

---

## 2. 事实基线（2026-09-07 11:15 实测核实，非沿用旧报告）

### 2.1 仓库状态

| 项 | 实测值 |
|---|---|
| HEAD | **detached @ `170dcb4`**（分支引用缺失，见 2.4 前置动作） |
| 工作区 | 干净（本次执行计划文件除外，未跟踪） |
| 提交链 | `95cf1c6 → 3331f51 → 9512439 → 170dcb4`（v1.3.0 栈 + 3 份文档） |
| schema | `LATEST_SCHEMA_VERSION = 4`（db.rs:10）→ 本周期升级 **v5** |
| 门禁参考 | 2026-09-06 23:26 实测：cargo **113 passed / 0 failed / 2 ignored**；vitest **45**（10 文件）；tsc **0**；vite build ✓ —— **批次 0 必须重跑并记录新数字** |
| 真实库 | v4，integrity ok，tasks=0 / sessions=20 / segments=4（勿动；回滚点在 `release/v1.3.0/acceptance/20260906-2328/pre-acceptance-v4/`） |

### 2.2 方案 §四 文件边界现状核对

| 文件 | 现状 | 批次动作 |
|---|---|---|
| `src/features/tasks/TasksPanel.tsx` | 已有 | 批次 2/3 改造迁移 |
| `src/features/tasks/TaskDetailDialog.tsx` | 已有 | 批次 3 改造迁移 |
| `src/features/tasks/ProjectManager.tsx` | 已有 | 批次 3 改造迁移 |
| `src/features/tags/TagManager.tsx` | 已有 | 批次 3 改造迁移 |
| `src/features/shared/palette.ts` | 已有 | 批次 5 扩展 token |
| `src/test/FakeAppGateway.ts` | 已有 | 批次 1 契约同步 |
| `src/domain/models.ts` | 已有 | 批次 1 类型扩展 |
| `src/features/management/*`（4 个新文件） | **缺失** | 批次 3 新建 |
| `src/features/shared/SearchablePicker.tsx / DatePicker.tsx / Dialog.tsx` | **缺失** | 批次 3/4 新建 |
| `src/domain/search.ts / localDate.ts` | **缺失** | 批次 3/4 新建 |
| `outputs/abyssal-prototype/` | **目录不存在**（含 `outputs/` 本身） | ⚠️ 见 2.3-③ |
| `pinyin-pro` 依赖 | **未安装** | 批次 3 安装并锁定 |

### 2.3 v1 事实核查的更正（按 Codex 审核，v2 已全部重核）

| # | v1 结论 | v2 更正（已逐条实测核实） |
|---|---|---|
| ① | ~~AGENTS.md 整体过时~~ | **局部失实**：Tailwind v4 / oxfmt 实际存在且接入构建与样式；失实的仅是 Figma Make 环境描述、"服务器已运行"等假设。**处置**：局部纠错（删失实环境描述、补 Tauri 约定），保留有效约定，独立小提交（批次 0 任务 0.8） |
| ② | ~~「写死 25」未复现~~ | **已定位**：`TasksPanel.tsx:87` `const minutePerPomodoro = useMemo(() => projects.length >= 0 ? 25 : 25, [projects])` —— 恒为 25；247 行 `formPomodoro * minutePerPomodoro` 用于新建面板换算提示。同文件 85 行 `effectiveTagId = formTagId || fallbackId` 即"自动填『其他』标签"的现行实现。**v1 漏查原因**：grep 模式只搜 `25*60`/`1500` 乘法表达式，未覆盖裸字面量三元——教训：找写死默认值必须同时搜变量定义与裸字面量 |
| ③ | ~~原型目录不存在~~ | **原型存在但不在仓库**：`C:/Users/27846/Documents/Codex/2026-09-05/superpowers-plugin-superpowers-openai-curated-remote/outputs/abyssal-prototype`。**复用，不重建**；批次 0 迁入开发工作树后补日历、失败提示、撤销冲突演示；浏览器视觉验收保持「未完成」标记，不伪造截图 |

### 2.4 执行前置动作（批次 0 内执行；PowerShell 可粘贴形式；撤回 v1 的批量 branch -f 命令）

> 撤回理由（审核意见第 6 条）：detached HEAD @ `170dcb4` 本身不构成"沙箱回滚"的证据——那是 WorkBuddy
> 运行环境内的观察，未在本机验证。分支缺失可能另有原因，也可能根本不影响本周期。

```powershell
# ① 先核对，再动手（全部只读）
git branch -vv                     # 确认现有引用（实测仅 main 与 feature-v1.1-local-prerequisites）
git cat-file -t 170dcb4            # 确认基线提交可达
git log --oneline -3 170dcb4       # 确认链完整：170dcb4 → 9512439 → 3331f51

# ② 创建前查重：目标目录与分支都必须不存在；任一已存在即停下报告，不覆盖
Test-Path "D:/Project/abyssal-reverie/worktrees/management-glass"   # 期望 False
git branch --list feature/management-glass                          # 期望无输出
git worktree list                                                   # 确认该路径未被占用

# ③ 从确认的基线创建开发分支 + 隔离工作树，创建后立即核验
git worktree add -b feature/management-glass "D:/Project/abyssal-reverie/worktrees/management-glass" 170dcb4
git worktree list                  # 核验新工作树出现

# ④ 两份未跟踪方案文档带入新工作树（untracked 不随 worktree add 复制；主工作树原件保留，不删不改）
$plans = "docs/superpowers/plans"
$dest  = "D:/Project/abyssal-reverie/worktrees/management-glass/docs/superpowers/plans"
Copy-Item "$plans/2026-09-07-management-glass-development.md"   $dest
Copy-Item "$plans/2026-09-07-management-glass-execution-plan.md" $dest

# ⑤ 复制后核对内容一致：同名两份文件的 Hash 值必须相同，不一致即停下报告
Get-FileHash "$plans/2026-09-07-management-glass-development.md",   "$dest/2026-09-07-management-glass-development.md"
Get-FileHash "$plans/2026-09-07-management-glass-execution-plan.md", "$dest/2026-09-07-management-glass-execution-plan.md"

# ⑥ 不顺带恢复 feature/v1.1.2-consistency、feature/v1.2-tasks-projects 等历史分支；
#    如后续确需，单独核对可达性后按需创建，不写在本计划前置里。
```

> worktree add 会创建 `.git/worktrees/*` 与新分支引用——创建后**必须立即核验**（步骤 ③末行）。
> 已知环境差异：WorkBuddy 沙箱会静默回滚 `.git` 下新文件创建（两次实证，见 v1 §2.4，保留备查），
> 因此此步骤在真实终端执行；若在沙箱内执行，每步都要延时后复核。
> 步骤 ⑤ 哈希不一致同样 halt-and-report，不带着缺失或不一致的文档进后续任务。

---

## 3. 裁定结果（2026-09-07 Codex 审核给出）

| # | 问题 | 裁定 | 落点 |
|---|---|---|---|
| Q1 | v1.3.0 与本周期的关系 | **候选封存，隔离开发新周期**。措辞注意：封存 ≠ 决定不发，是否发版留待后续单独裁定；新候选仍需完成其适用验收 | 批次 6 任务 6.6 |
| Q2 | 新版本号 | **暂定 v1.4.0，打包阶段最终确定**；格式化维护提交不预占版本号 | 批次 6 任务 6.6 |
| Q3 | worktree 位置与分支名 | `worktrees/management-glass` + `feature/management-glass` **可行，创建前先查重** | §2.4 步骤 ② |
| Q4 | 原型来源 | **复用现有原型**（§2.3-③ 绝对路径），不重建 | 批次 0 任务 0.7 |
| Q5 | AGENTS.md 处置 | **局部纠错、保留有效约定**，独立小提交 | 批次 0 任务 0.8 |

---

## 4. 批次执行序列（0 → 6，顺序不可调换；每批独立提交）

### 批次 0：冻结基线与复现 ⬜

**目标**：可追溯基线 + 两份方案文档入库 + 红灯证据记录 + 原型迁入与行为清单。

| 任务 | 内容 | TDD | 涉及文件 |
|---|---|---|---|
| 0.1 | 读取 AGENTS.md（按 2.3-① 局部采信）、EXECUTION_STATUS、HANDOFF、db.rs 迁移/备份实现、repository.rs `parse_backup_text` / `validate_import` / `import_data`，产出《基线事实清单》 | — | 只读 |
| 0.2 | 执行 §2.4 前置动作：核对引用与提交可达性 → 建 worktree + 开发分支 → **把两份未跟踪方案文档复制进新工作树**，核验并记录 | — | git |
| 0.3 | 重跑门禁，记录实际数字（不沿用 2026-09-06 旧数） | — | — |
| 0.4 | 记录 v1.3.0 两个产物 SHA-256 作为「不得变化」对照基线（`dd816b5b…` / `2a20c15b…`） | — | — |
| 0.5 | 🔴 红灯用例①（证据记录，不落持久提交）：设置 1 分钟时，新建面板提示与实际创建预算都等于 60s（当前 `TasksPanel.tsx:87` 恒 25 → 提示显 25 分钟；红灯输出存批报告） | 🔴证据 | TasksPanel 创建面板 + 网关 |
| 0.6 | 🔴 红灯用例②（证据记录）：标题-only 创建后 project_id / tag_id / priority 均为 NULL（当前 85 行 `effectiveTagId = formTagId \|\| fallbackId` 自动填「其他」，红灯输出存批报告） | 🔴证据 | 契约层（FakeAppGateway / models.ts） |
| 0.7 | 原型**迁入**（复用 §2.3-③ 现有原型，不重建）：拷贝入仓 → 补日历、保存失败提示、撤销冲突演示 → 浏览器确认；若安全策略仍阻塞则**保留未验收标记**，不伪造截图 | — | outputs/abyssal-prototype/ |
| 0.8 | AGENTS.md **局部纠错**（裁定 Q5）：删失实的 Figma Make 环境描述与"服务器已运行"假设，补 Tauri/pnpm 工程约定；**保留** Tailwind v4、oxfmt 等有效内容；独立小提交 | — | AGENTS.md |

**交付**：基线事实清单、2 个红灯证据（附失败输出，存批报告不提交失败测试）、迁入的原型 + 行为清单、AGENTS.md 纠错提交。
**提交点**：`docs: commit management-glass plan & execution plan` → `docs(agents): correct stale scaffold assumptions` → `chore(baseline): record baseline & gate numbers`（全部全绿可提交）。

---

### 批次 1：可选字段与数据升级（schema v4 → v5）⬜

**目标**：标题-only 创建落库全 NULL；旧数据零丢失；备份兼容；关联修订号就位。

| 任务 | 内容 | TDD | 关键约束 |
|---|---|---|---|
| 1.1 | Rust 用例：项目/标签/优先级可清空、标题-only 创建、旧值保留（含「历史『其他』『中』不被批量清空」；红灯证据含 TasksPanel.tsx:85 自动填 fallback 的现行行为） | 🔴 | 用 v4 库**副本**演练 |
| 1.2 | schema v5：tasks.`tag_id` / `priority` 允许 NULL。**迁移方式已定：重建 tasks 表**——v4 中两列均为 NOT NULL（db.rs:146 `priority TEXT NOT NULL DEFAULT 'med'`、db.rs:148 `tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE RESTRICT`），SQLite 无法用 ALTER 解除既有列约束；**重建先例是 v2→v3**（`run_v3_migration` db.rs:100，文档串 db.rs:85 明写 v2→v3；rename-first db.rs:134–137）——v3→v4 为增量 ALTER（db.rs:696–702），不构成重建先例 | 🔴 | 事务内；逐条复制旧值（R2）；`foreign_key_check` 清零 |
| 1.3 | **依赖表保护清单**（重建 tasks 前后逐一核对 FK 与数据）：① 子表外键 `sessions.task_id`（db.rs:53，SET NULL）、`budget_history.task_id`（db.rs:643，NOT NULL + **CASCADE**——父行丢失会被级联静默删除）、`focus_segments.task_id`（db.rs:655，SET NULL）；**`sessions.tag_id` 的保护不能替代 `sessions.task_id`**。② `timer_state.selected_task_id`（db.rs:39，裸 TEXT **无 FK**）与 `timer_state.tag_id`（db.rs:236）的绑定显式保留。③ 索引 `idx_tasks_sort_order`（db.rs:32/225）、`idx_tasks_tag_sort`（db.rs:226）、`idx_tasks_project`（db.rs:704）重建后全部重建/验证，不丢不重。④ **数据守恒对账**：`foreign_key_check` 通过 ≠ 历史数据未被级联删除或置空——迁移前后逐表对账**记录数、task 关联分布、总投入毫秒数**（含空表 = 0 场景），任一不等即失败回滚 | 🔴 | 子表 FK 须重新指向新表（手段由批次 1 用例定案；v2→v3 先例用 rename-first + 同批重建子表，db.rs:134–137）；重建幂等；失败整体回滚 |
| 1.4 | **区分两条迁移路径**：`v3→v5`（旧备份恢复场景，经 v4 逻辑继续）与 `v4→v5`（当前用户，仅元数据可空化 + 修订号）。**v4 用户预览不得再展示旧项目映射、不得重算旧预算**——`preview_v4_migration`（db.rs:840）按版本分派或拆出 v5 专用预览 | 🔴 | R06 协议两条路径都走预览确认 |
| 1.5 | 迁移测试矩阵补齐：**预览 / 取消 / 确认 / 失败回滚 / 重复打开** ×（v3 起点、v4 起点）——取消零改动、失败整体回滚、重复打开不二次迁移不重复写 | 🔴 | R4：全部用副本 |
| 1.6 | tasks 新增关联修订号列（仅关联变更时递增）；`RelationshipPatch { expectedRevision }` 用 serde tagged enum 落地，事务内校验实体存在、状态、revision | 🔴 | 过期 revision 返回可理解错误 |
| 1.7 | 备份兼容：验证现 formatVersion 能否表达空字段；不能则提升并**保留旧版本解析**；往返导入导出覆盖空值 + 真实历史数据 | 🔴 | R6 |
| 1.8 | 未来版本拒写测试（v6 库打开报 `database_too_new`） | 🔴 | 已有机制回归 |
| 1.9 | 前端同步：models.ts（`TaskMetadata` / `NullablePatch` / `RelationshipPatch`）、appGateway / tauriAppGateway / FakeAppGateway；**显式清空（`{action:'clear'}`）覆盖全部可选字段**：项目、标签、优先级、截止日期、备注——不只项目/标签 | 🔴 | 接口按方案 §四 类型 |
| 1.10 | 迁移副本演练测试（仿 `drill_real_v3` 模式建 v4→v5 drill，数据源用 gitignored 快照，不碰真实库） | 🔴 | R4 |

**验收（方案原文）**：标题-only 创建后三字段均 NULL；旧记录、总投入、快照不变；非法外键与过期修订返回可理解错误。
**预计新增**：Rust 约 +12～18、vitest 约 +4～6（以实测为准）。
**提交点**：`feat(db): schema v5 nullable metadata + relationship revision` / `feat(db): versioned migration preview (v3 path vs v4 path)` / `feat(gateway): nullable patch contract`（Rust 与前端可分提）。
**风险**：重建表漏保护依赖表 → 以 1.3 清单逐项核对 + 演练兜底；演练不过不进下一批。

---

### 批次 2：预算与业务操作一致性 ⬜

**目标**：预算由已保存设置权威派生；完成/撤销/归档/标签删除全走 Rust 既有原子规则。

| 任务 | 内容 | TDD |
|---|---|---|
| 2.1 | 🟢 红灯①转绿：新建提示由已保存 settings 派生（含「设置保存中提交则等待结果，失败则提示，不用未保存值」）；创建返回权威 `targetSeconds`（断言 `40×2×60=4800` 用实际网关返回值，不构造假响应） | 🟢 |
| 2.2 | 回归：改默认时长不改旧任务预算、不改进行中/暂停轮次 | 🔴 |
| 2.3 | 预算编辑：显式传 targetSeconds + `budget_history` 记账；已投入不变；显示旧/新预算与已投入 | 🔴 |
| 2.4 | 活动任务完成统一走 `complete_task_now`；普通完成不得绕开封口规则（暂停、封口、解绑） | 🔴 |
| 2.5 | 归档/恢复保留归档前完成状态；项目完成校验（无未完成任务；最后任务完成只改进度）；计数在成功响应后统一更新 | 🔴 |
| 2.6 | 标签删除：先显示影响数量；确认后清关联不迁移；删除保护保留；撤销用关联修订号 + 事务 + 一次性令牌（窗口过期后旧令牌无效）；**撤销冲突矩阵**按方案 §六（清 A→设 B→撤销删 A → 任务仍为 B；设 B 后又清空 → 也不恢复） | 🔴 |
| 2.7 | 「关于」版本号改从构建版本来源读取，与安装包一致 | 🔴 |

**验收（方案原文）**：运行/暂停/到期三状态切任务、完成、撤销均不重复计时；错误与重放一致。
**提交点**：`fix(budget): derive from saved settings` / `feat(tags): revision-guarded delete & undo` 等按任务拆分。

---

### 批次 3：管理布局、检索与草稿 ⬜

**目标**：管理内部侧栏 + 三视图 + 拼音检索 + 草稿保护。

| 任务 | 内容 | TDD |
|---|---|---|
| 3.1 | `ManagementPanel.tsx` 内部侧栏（任务/项目/标签）；主侧栏仅「管理」入口；宽屏并排 / 窄屏抽屉按**实际容器宽度**切换 | 🔴（视图状态） |
| 3.2 | `TasksView / ProjectsView / TagsView` 共享实体仓库；`managementState.ts` 独立记住各分类的搜索/筛选/滚动/选中；筛选排除选中项时收起详情；**从项目/标签点进任务详情，返回时恢复来源页原位置**（返回上下文入 managementState） | 🔴 |
| 3.3 | 安装 `pinyin-pro`（锁定版本）；`search.ts` 纯函数 + 拼音索引**缓存**（实体变更时失效，不每次按键重拼；**项目/标签改名后，其下任务的检索索引同步失效**）；测试覆盖：论文/lunwen/lw、大小写归一、全角半角、精确>前缀>包含、同分稳定排序 | 🔴 |
| 3.4 | `SearchablePicker.tsx`：未设置、主动新建、清除、已归档当前值展示；不隐式绑定默认实体 | 🔴 |
| 3.5 | 显式保存状态机（未保存/保存中/已保存/失败）；IME 候选 Enter 不提交；多行备注 Enter 换行；提交期禁重复；切页草稿保留与恢复提示；**关闭应用提醒未保存草稿——覆盖窗口关闭与托盘退出两条路径**；详情快捷修改失败时**回退显示已保存值、同时保留可重试的新值** | 🔴 |
| 3.6 | 空状态四件套：无结果（清搜索/筛选入口）、真实空集合（新建入口）、待办全完成提示、新建条目不符合当前筛选时明确提示并切换视图 | 🔴 |

**验收（方案原文）**：不离开任务即可选择/新建关联；跨页修改立即同步；重命名不产生重复实体。
**提交点**：按 3.1–3.6 分 3～4 个提交，视图与状态分离。

---

### 批次 4：日历控件 ⬜

| 任务 | 内容 | TDD |
|---|---|---|
| 4.1 | `localDate.ts`：`isValidLocalDate` / `addLocalDays` 纯函数，**不用 `new Date('YYYY-MM-DD')` 的 UTC 语义**；固定期望：`2028-02-29`✓、`2027-02-29`✗、`addLocalDays('2026-12-28',7)='2027-01-04'`（不依赖测试机「今天」） | 🔴 |
| 4.2 | `DatePicker.tsx`：周一首日；今天与已选高亮**不只靠颜色**；上/下月；年月选择（年份列表可滚动）；返回今天；今天/明天/一周后/清除快捷；手动输入严格校验（闰年、跨月），错误就地提示不清旧值 | 🔴 |
| 4.3 | 共享 `Dialog/Popover` 层：portal 防裁切、窗口边缘翻转限高、滚动锁定、焦点回归触发器；方向键移动、Enter 选择、Escape 关闭、Tab 正常；打开时定位已选日否则今天 | 🔴 |
| 4.4 | 接入新建、详情、编辑三处同一控件；清除后真正保存空值 | 🔴 |
| 4.5 | **逾期标记**：仅基于本地日期（YYYY-MM-DD 字符串比较，不走 UTC/时区）；**今天不算逾期**；**跨天刷新不能依赖「渲染时重算」**（页面静止时整夜可能不重渲染）——监听本地午夜定时触发刷新，**休眠恢复、窗口重新激活时补检查**当前本地日期；验收含「页面不操作跨过午夜」「休眠跨天后恢复」两场景；不得保留昨天的过期结论；**已完成任务不持续显示逾期警告** | 🔴 |

**验收（方案原文）**：鼠标键盘都能完成选日与清除；不跨时区变前一天；今天与已选有明确区别。
**提交点**：`feat(date): local-date pure funcs + picker` / `feat(date): integrate into task forms`。

---

### 批次 5：全系统玻璃与微交互 ⬜

| 任务 | 内容 | TDD/验证 |
|---|---|---|
| 5.1 | token 体系：surface / popup / overlay / foreground / focus，基准 `rgba(8,13,18,0.24)` + `blur(18px)` + 原细边框；**透明度最终依原背景最亮帧校验**；不新增主题 | 🔴（token 存在性与引用检查） |
| 5.2 | 管理、设置、统计、专注、小窗表面统一引用 token；业务布局保持原结构（R1） | 视觉走查清单 |
| 5.3 | 下拉/日历/弹窗/Toast：文字、底色、边框、禁用态、层级规则统一；菜单/弹窗用更实同色底层，正文不依赖背景恰好暗；对比度 ≥ 4.5:1；焦点框不被裁剪 | 🔴 + 走查 |
| 5.4 | 移除 `transition: all` 与整页位移；分类切换仅内容 120–160ms 短淡入；按钮 120–150ms 色彩反馈；禁列表搜索每字动画 | 🔴（静态检查） |
| 5.5 | `prefers-reduced-motion` 与应用「降低动态效果」同时关非必要动画；不支持模糊时同色实底回退；避免嵌套 backdrop-filter | 🔴 |
| 5.6 | Windows 检查 100%/150%/200% 缩放（E1 实测主缩放 150%）、长标题、长标签、菜单边缘、窄容器；亮帧/暗帧可读性；截图入验收目录 | 实机走查 |

**验收（方案原文）**：不出现白底下拉、不可读辅助文字、焦点丢失、卡片套卡片、按钮被挤出。
**提交点**：`style(tokens): unified glass surfaces` / `style(motion): constrained transitions`。

---

### 批次 6：联合回归与交付 ⬜

| 任务 | 内容 | 备注 |
|---|---|---|
| 6.1 | 全门禁通过（见 §5 矩阵），记录实际数字 | — |
| 6.2 | 迁移矩阵：新库建库、**v4 库副本升级 v5**、备份往返（含空值）、未来版本拒写 | 副本！R4 |
| 6.3 | 故障注入：保存失败、迟到响应、重复点击、撤销后再次修改——覆盖冲突与真实数据保留 | 不碰真实库 |
| 6.4 | 1000 条任务演示数据：记录设备、输入响应、滚动、内存；目标输入反馈通常 <100ms、无持续内存增长；未达标先优化索引与渲染，再评估虚拟列表 | 实测数字入报告 |
| 6.5 | 正式 EXE 实机：键盘、IME、缩放、日历、计时、管理跳转；**多屏仍无设备 → A8-2 记 BLOCKED** | 沿用 2026-09-06 裁定 |
| 6.6 | 新候选打包至 `release/<Q2 版本号>/`，独立 SHA-256；受影响验收重跑；**v1.3.0 原候选结果保留原始归属**（R3） | 打包需授权？——打包本身不需授权，**发版**需（R8） |
| 6.7 | 更新 EXECUTION_STATUS / REGRESSION_REPORT / 已知问题；原型与正式应用结果分开记录，不把 HTML DOM 结果写成 Rust/EXE 验收 | — |
| 6.8 | 合并 / tag / 发布：**仅在乔显式授权后执行** | R8 |

**提交点**：`test(integration): joint regression` / `release(<ver>): candidate build & hashes`。

---

## 5. 门禁矩阵（每批 DoD 必跑；数字为 2026-09-06 基线，执行时以实测为准）

| 门禁 | 命令 | 基线 |
|---|---|---|
| Rust 全量 | `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 113 passed / 0 failed / 2 ignored |
| 前端全量 | `pnpm verify`（= `tsc --noEmit && vitest run && vite build`） | tsc 0 + vitest 45 + build ✓ |
| 迁移演练 | `cargo test --manifest-path src-tauri/Cargo.toml drill_real_v3 -- --ignored --nocapture`（批次 1 后加 v4→v5 drill） | 246s ↔ 246000ms 守恒 |
| 产物对照 | v1.3.0 两产物 SHA-256 不变 | `dd816b5b…` / `2a20c15b…` |

> 已知坑：`cargo fmt` 在仓库根跑会报 `could not find Cargo.toml` 且退出码同为 1——判断格式化是否干净要看有无 `Diff in` 输出（本周期本就不跑 fmt，R7）。

---

## 6. 提交与上报规范

- 提交粒度：每批 1～4 个提交；Rust 与前端契约可分开；**每个提交点必须全绿**——红灯只作为批内过程证据留存（§0），不落持久提交。
- commit message：`<type>(<scope>): <summary>` + 正文说明动机与验证方式；不混无关改动（R7）。
- 每任务上报三件套：**变更文件、测试数量实际值、所属批次 commit SHA**（不强求一任务一提交）。
- 批报告追加至 `docs/plans/EXECUTION_STATUS.md`（或独立执行日志），含遗留问题与下一批前置。

## 7. 停止条件（触发即 halt-and-report，等乔裁定）

1. 门禁数字与 2.1 基线不符且原因不明；
2. 迁移副本演练任何一条不变量失败（数量、守恒、FK、取消路径、重复打开）;
3. 需要触碰方案 §四 之外的文件，或引入方案之外的新依赖；
4. 任何真实库写入需求（一律改走副本）；
5. v1.3.0 产物哈希发生变化；
6. 执行中发现与方案/本计划冲突的事实（如再出现 v1「写死 25」式漏查），先停下核对，不带着错误假设继续。

**豁免——以下为计划内情况，不触发停止条件（第二轮复核第 5 条）**：

- 两份未跟踪方案文档出现在工作区 / 工作树：批次 0 任务 0.2 的正常步骤，直至随批次 0 首个提交入库；
- TDD 预期红灯的失败输出：§0 规定的批内证据，红→绿后自然消失，不算「门禁失败」；
- 已允许的视觉验收 BLOCKED：A8-2 多屏缺设备、原型浏览器验收遇安全策略——如实记录即可，不停工。

## 8. 风险登记

| 风险 | 影响 | 缓解 |
|---|---|---|
| 迁移方式选择（ALTER vs 重建表）不当 | 数据丢失 | 副本演练先行；演练不过不进下一批 |
| backdrop-filter 多层嵌套性能 | 小窗/低配机掉帧 | 禁嵌套 + 实底回退 + 亮帧校验 |
| pinyin-pro 打包体积 | 安装包变大 | 记录前后体积；按需 lazy import |
| worktree 与主工作树混淆 | 改错地方 | 2.4 核验步骤 + 每批报告注明工作路径 |
| 分支引用再次丢失（WorkBuddy 沙箱已两次实证） | 提交悬空 | 2.4 由乔执行；每批开始前 `git branch -vv` 核验 |
| HTML 原型视觉验收安全策略阻塞 | 原型未验收 | 按方案：保留未验收标记，不伪造；DOM 结果不替代实机验收 |

## 9. 审核闭环状态

Codex 2026-09-07 审核提出的六项必改 + 六项任务表细节补充，已全部落入本 v2（对照见「修订记录」）：

| 审核意见 | 处置 | 落点 |
|---|---|---|
| 1 写死 25 是漏查 | ✅ 已定位并承认漏查原因；红灯①改写 | 修订记录 1、§2.3-②、批次 0 任务 0.5 |
| 2 AGENTS.md 非全部失效 | ✅ 改局部纠错 | §2.3-①、批次 0 任务 0.8、裁定 Q5 |
| 3 原型在仓库外 | ✅ 记录绝对来源，复用不重建 | §2.3-③、批次 0 任务 0.7、裁定 Q4 |
| 4 迁移需具体 + 区分 v3/v4 起点 | ✅ 重建表 + 依赖表保护清单 + 双路径预览分派 + 五态测试矩阵 | 批次 1 任务 1.2–1.5、1.10 |
| 5 红灯提交与全绿矛盾 | ✅ 红灯只留证据，提交全绿 | §0、§6 |
| 6 分支恢复命令不能原样执行 | ✅ 撤回 branch -f，改核对后创建；方案文档带入工作树 | §2.4、批次 0 任务 0.2 |
| 细节补充 ×6 | ✅ 分别落点 | 批次 1（1.9 显式清空全字段）、批次 3（3.2 返回原位置、3.3 改名失效、3.5 关闭提醒/快捷修改回退）、批次 4（4.5 逾期跨天） |

**第二轮复核（2026-09-07，Codex）**：结论「主要修订已落实，**可以进入批次 0 的基线核对**；但不能把整份计划标为审核全部通过」。五项修订 + 一处转述更正已落入 v2.1（修订前全部实测核实）：

| 复核意见 | 处置 | 落点 |
|---|---|---|
| 1 迁移依赖清单漏关键字段 | ✅ 补 `sessions.task_id` / `budget_history.task_id` / `focus_segments.task_id`、`timer_state.selected_task_id` 绑定保留、`idx_tasks_sort_order` / `idx_tasks_project`；新增**数据守恒对账**（FK 通过 ≠ 未被级联删除/置空） | 批次 1 任务 1.3（v2.1 修订 1） |
| 2 迁移先例版本写错 | ✅ 更正为 v2→v3（db.rs:100；v3→v4 实为增量 ALTER，db.rs:696–702） | 批次 1 任务 1.2（v2.1 修订 2） |
| 3 跨天刷新不能只写渲染重算 | ✅ 午夜定时触发 + 休眠恢复/窗口重新激活补检查 + 两场景验收 | 批次 4 任务 4.5（v2.1 修订 3） |
| 4 前置命令不适配实际 PowerShell | ✅ 改可粘贴形式 + 目录/分支查重 + 复制后 `Get-FileHash` 核对 + 原件保留 | §2.4（v2.1 修订 4） |
| 5 停止条件须排除计划内情况 | ✅ 新增豁免清单（未跟踪文档 / 预期红灯 / 已允许 BLOCKED） | §7（v2.1 修订 5） |
| 转述更正：改名索引失效在 3.3 | ✅ 计划文件本身无误（3.3 与首轮明细表均落批次 3），错误出自移交报告转述，已更正口径 | —（v2.1 修订 6） |

**执行状态**：批次 0 可开展；本轮修订随批次 0 首个提交入库；迁移依赖清单已补齐——批次 1 的前置在文档层面满足，实跑前置仍以批次 0 门禁重跑与红灯证据为准。
