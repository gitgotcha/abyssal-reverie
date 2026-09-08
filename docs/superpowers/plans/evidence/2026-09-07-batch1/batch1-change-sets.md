# 批次 1 分批变更清单（提交整理依据；工作树状态 @ 2026-09-08 00:0x）

> **基线**：`e29ddae4`（提交点 1：schema v5 nullable metadata + relationship revision，已入库）
> **门禁（工作树现状实测）**：Rust **160 passed / 0 failed / 3 ignored**；前端 tsc 0 + vitest
> **87 passed（19 文件）** + build ✓；`git diff --check` clean。
> **说明**：三批改动按 hunk 拆分需 `git add -p`（models.ts / db.rs 跨批共享）；若拆分成本高，
> 可合并为单一提交（建议名：`feat: batch 1 — migration flows, backup v3, nullable contract`），
> 由复审方裁定。evidence/ 与两个新测试文件必须随对应批次入库。

## 提交 A —— `feat(migration): version v3 and v4 upgrade flows`（任务 1.4 + 1.5 + 1.9 组件适配）

| 文件 | 内容 |
|---|---|
| `src-tauri/src/commands.rs` | confirm_migration 改走 `db::run_confirmed_migration`（先判版本再校验参数） |
| `src-tauri/src/db.rs` | `run_confirmed_migration`（v4 忽略参数 / v1–v3 校验预算与映射）+ 迁移协议测试组 5+4 个 |
| `src-tauri/src/models.rs` | `MigrationPreview.migration_kind`（legacySemantic / nullableMetadata） |
| `src/components/MigrationPrepScreen.tsx` | 预览界面按 migrationKind 分支（v4 隐藏预算/映射选项） |
| `src/App.tsx` | 循环优先级：null → 循环起点 low |
| `src/features/tasks/TaskDetailDialog.tsx` | 未设置优先级保持为空，dirty 比较与 clear/set 语义同口径 |
| `src/features/tasks/TasksPanel.tsx` | "未设置"标签 + PriorityPip 空心点 |
| `src/features/tasks/__tests__/TasksPanel.create.test.tsx` | fixture 补 relationshipRevision、tagId null |
| `src/domain/models.ts`（部分） | `MigrationPreview.migrationKind` |
| `src/test/FakeAppGateway.ts`（部分） | previewMigration 返回 migrationKind |
| `src/components/__tests__/MigrationPrepScreen.test.tsx`（新） | 3 个界面行为测试（v3 显示选项 / v4 隐藏并声明不改写 / 预览只读） |

## 提交 B —— `feat(backup): format v3 with project snapshots + v6 refusal`（任务 1.7 + 1.8 + 1.10）

| 文件 | 内容 |
|---|---|
| `src-tauri/src/repository.rs` | 备份格式 v3：ExportBundle 携带 projects 快照（BackupProject）；import 重建缺失项目行（归「其他」类）；validate 校验项目引用；EXPORT_SCHEMA_VERSION 2→3（v2 备份仍解析，relationshipRevision 缺省按 0） |
| `src-tauri/src/db.rs` | v6 拒写测试（字节级零改动验证）+ fixture drill（预览/取消/确认/对账/重开/复预览拒绝）+ real v4 drill（#[ignore]，快照待放） |
| `src-tauri/src/models.rs` | `BackupProject` + ExportBundle.projects（serde default） |

测试：v5 往返（null 元数据 + 修订号 + 预算/状态/截止/备注/项目关系全保留）、v2 旧备份按 0 读、v6 拒写零改动、fixture 全协议演练、v3 失败回滚（版本 3 零改动）。

## 提交 C —— `feat(gateway): nullable patch contract`（任务 1.9）

| 文件 | 内容 |
|---|---|
| `src/services/appGateway.ts` | 接口加 `applyTaskRelationship(patch)` |
| `src/services/tauriAppGateway.ts` | `invoke('apply_task_relationship', { patch })` |
| `src/test/FakeAppGateway.ts` | 镜像 Rust 语义的实现（修订号 CONFLICT / 实体校验 / 仅变化递增）+ createTask tagId 不再写死 system-other + startTimer 标签快照（无标签任务保持无标签 / 休息轮次 fallback） |
| `src/domain/models.ts`（部分） | RelationshipPatch.priority（优先级显式清空通道；deadline/notes 走既有 "" 语义） |
| `src/test/FakeAppGateway.contract.test.ts`（新） | 9 个契约行为测试 |

对应 Rust：`RelationshipPatch.priority`（NullablePatch\<TaskPriority\>）+ apply_relationship_patch 的 priority 分支（repository.rs，已含在提交点 1 之后的修改中——**若 A/B/C 拆分，priority 扩展归 C**）。

## 批次 1 验收状态

| 任务 | 状态 |
|---|---|
| 1.1–1.3、1.6 | ✅ 已入库（e29ddae4） |
| 1.4 / 1.5 | ✅ 代码+测试完成（本清单提交 A） |
| 1.7 / 1.8 / 1.10 | ✅ 代码+测试完成（本清单提交 B）；real v4 drill 已针对 `{db,-wal,-shm}` 副本通过，任务 1→1、会话 21→21、预算 1→1、片段 5→5，重开不重复迁移 |
| 1.9 | ✅ 代码+测试完成（本清单提交 C）；浏览器视觉 / 真实键盘 / Tauri EXE 验收保持 BLOCKED（与批次 0 边界一致） |

## 已知边界（记入批报告）

1. FakeAppGateway.ts 含 Codex 阶段性全文件重排（1087 行版）——复审要求的"压缩为最小修改"
   因该文件被实时覆盖无法单方面完成；当前版本功能/类型/测试全绿，**重排 diff 建议以
   `git diff -w --ignore-blank-lines` 复核**；如必须压缩，需在 Codex 完全停写后执行
   （重建脚本已验证：`git show e29ddae4:src/test/FakeAppGateway.ts` + 3 处修改）。
2. 导入的项目统一落「其他」分类（分类管理仍 live-only）——1.7 验收清单未含分类归属，已记边界。
3. 对话框保留“未设置”优先级为空值；显式清空通过 RelationshipPatch.priority 发送 clear。
