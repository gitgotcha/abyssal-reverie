import { useEffect, useState } from "react";
import type {
  Category, NullablePatch, Project, RelationshipPatch, Tag, Task, TaskPriority,
  TaskProgress, UpdateTaskInput,
} from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { PRIORITY_LABELS, formatProgressLine } from "./TasksPanel";
import { DatePicker } from "../../components/DatePicker";
import { SearchablePicker } from "../shared/SearchablePicker";

type TaskDraft = {
  title: string;
  projectId: string;
  tagId: string;
  pomodoro: number;
  budgetMinutes: string;
  priority: TaskPriority | "";
  deadline: string;
  notes: string;
};

const draftKey = (taskId: string) => `abyssal-reverie.task-draft.${taskId}`;

function draftStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    const storage = window.localStorage;
    storage.getItem("__abyssal_reverie_storage_probe__");
    return storage;
  } catch {
    try {
      const storage = window.sessionStorage;
      storage.getItem("__abyssal_reverie_storage_probe__");
      return storage;
    } catch { return null; }
  }
}

function loadTaskDraft(taskId: string): TaskDraft | null {
  try {
    const raw = draftStorage()?.getItem(draftKey(taskId));
    if (!raw) return null;
    const draft = JSON.parse(raw) as Partial<TaskDraft>;
    if (typeof draft.title !== "string" || typeof draft.pomodoro !== "number") return null;
    return {
      title: draft.title,
      projectId: typeof draft.projectId === "string" ? draft.projectId : "",
      tagId: typeof draft.tagId === "string" ? draft.tagId : "",
      pomodoro: Math.max(1, Math.min(99, draft.pomodoro)),
      budgetMinutes: typeof draft.budgetMinutes === "string" ? draft.budgetMinutes : "1",
      priority: draft.priority === "high" || draft.priority === "med" || draft.priority === "low" ? draft.priority : "",
      deadline: typeof draft.deadline === "string" ? draft.deadline : "",
      notes: typeof draft.notes === "string" ? draft.notes : "",
    };
  } catch {
    return null;
  }
}

function clearTaskDraft(taskId: string): void {
  try { draftStorage()?.removeItem(draftKey(taskId)); } catch { /* storage may be disabled */ }
}

/** v1.2 F2: task detail — edit metadata, save with visible state, complete or
 *  archive. Failure keeps the dialog open with the error inline. */
export function TaskDetailDialog({ task, progress, categories, projects, tags, onClose, onSave, onApplyRelationship, onToggleDone, onArchive, onComplete }: {
  task: Task;
  progress?: TaskProgress;
  categories: Category[];
  projects: Project[];
  tags: Tag[];
  onClose: () => void;
  onSave: (patch: Omit<UpdateTaskInput, "id">) => Promise<unknown>;
  onApplyRelationship: (patch: RelationshipPatch) => Promise<unknown>;
  onToggleDone: () => void;
  onArchive: () => void;
  onComplete: () => Promise<unknown>;
}) {
  const [restoredDraft] = useState(() => loadTaskDraft(task.id));
  const [title, setTitle] = useState(restoredDraft?.title ?? task.title);
  const [projectId, setProjectId] = useState(restoredDraft?.projectId ?? task.projectId ?? "");
  const [tagId, setTagId] = useState(restoredDraft?.tagId ?? task.tagId ?? "");
  const [pomodoro, setPomodoro] = useState(restoredDraft?.pomodoro ?? task.pomodoroTarget);
  const [budgetMinutes, setBudgetMinutes] = useState(
    restoredDraft?.budgetMinutes ?? String(Math.max(1, Math.round(task.targetSeconds / 60))),
  );
  const budgetSeconds = Math.max(1, Number(budgetMinutes) || 1) * 60;
  const [priority, setPriority] = useState<TaskPriority | "">(restoredDraft?.priority ?? task.priority ?? "");
  const [deadline, setDeadline] = useState(restoredDraft?.deadline ?? task.deadline ?? "");
  const [notes, setNotes] = useState(restoredDraft?.notes ?? task.notes);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"clean" | "dirty" | "saving" | "saved" | "error">("clean");

  const dirty =
    title !== task.title || projectId !== (task.projectId ?? "") ||
    pomodoro !== task.pomodoroTarget ||
    budgetSeconds !== task.targetSeconds || priority !== (task.priority ?? "") ||
    tagId !== (task.tagId ?? "") ||
    deadline !== (task.deadline ?? "") || notes !== task.notes;

  useEffect(() => {
    if (!dirty) {
      clearTaskDraft(task.id);
      return;
    }
    try {
      const draft: TaskDraft = { title, projectId, tagId, pomodoro, budgetMinutes, priority, deadline, notes };
      draftStorage()?.setItem(draftKey(task.id), JSON.stringify(draft));
    } catch { /* storage may be disabled */ }
  }, [task.id, dirty, title, projectId, tagId, pomodoro, budgetMinutes, priority, deadline, notes]);

  const requestClose = () => {
    if (saving) return;
    if (dirty && !window.confirm("还有未保存的修改，确定要放弃吗？")) return;
    clearTaskDraft(task.id);
    onClose();
  };

  const projectOptions = categories.filter(category => category.status === "active").flatMap(category =>
    projects.filter(project => project.categoryId === category.id && (project.status !== "archived" || project.id === task.projectId))
      .map(project => ({ value: project.id, label: project.status === "archived" ? `已归档 · ${project.name}` : project.name, group: category.name, disabled: project.status === "archived" })),
  );
  const tagOptions = tags.map(tag => ({ value: tag.id, label: tag.name }));

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setSaveState("saving");
    setError(null);
    try {
      const relationshipChanged =
        projectId !== (task.projectId ?? "") ||
        tagId !== (task.tagId ?? "") ||
        priority !== (task.priority ?? "") ||
        deadline !== (task.deadline ?? "") ||
        notes !== task.notes;

      if (relationshipChanged) {
        const project: NullablePatch<string> = projectId === (task.projectId ?? "")
          ? { action: "keep" }
          : projectId ? { action: "set", value: projectId } : { action: "clear" };
        const tag: NullablePatch<string> = tagId === (task.tagId ?? "")
          ? { action: "keep" }
          : tagId ? { action: "set", value: tagId } : { action: "clear" };
        const nextPriority: NullablePatch<TaskPriority> = priority === (task.priority ?? "")
          ? { action: "keep" }
          : priority ? { action: "set", value: priority } : { action: "clear" };
        const nextDeadline: NullablePatch<string> = deadline === (task.deadline ?? "")
          ? { action: "keep" }
          : deadline ? { action: "set", value: deadline } : { action: "clear" };
        const nextNotes: NullablePatch<string> = notes === task.notes
          ? { action: "keep" }
          : notes ? { action: "set", value: notes } : { action: "clear" };
        await onApplyRelationship({
          taskId: task.id,
          expectedRevision: task.relationshipRevision,
          project,
          tag,
          priority: nextPriority,
          deadline: nextDeadline,
          notes: nextNotes,
        });
      }

      if (
        title !== task.title ||
        pomodoro !== task.pomodoroTarget ||
        budgetSeconds !== task.targetSeconds
      ) {
        await onSave({
          title,
          pomodoroTarget: pomodoro,
          ...(budgetSeconds !== task.targetSeconds
            ? { targetSeconds: budgetSeconds }
            : {}),
        });
      }
      clearTaskDraft(task.id);
      setSaveState("saved");
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSaveState("error");
    } finally {
      setSaving(false);
    }
  };

  const field = {
    fontFamily: "var(--font-sans)", fontSize: 12, color: C.textPrimary,
    background: C.cardDim, border: `1px solid ${C.hairline}`,
    borderRadius: 7, padding: "6px 9px", cursor: "text", width: "100%",
  } as const;

  return (
    <div role="dialog" aria-modal="true" aria-label="任务详情" onClick={requestClose}
      style={{
        position: "fixed", inset: 0, zIndex: 85, background: "rgba(2,3,5,0.45)",
        display: "flex", alignItems: "center", justifyContent: "center",
      }}>
        <div role="document" onClick={e => e.stopPropagation()}
        style={{
          width: "min(420px, 92vw)", maxHeight: "88vh", overflowY: "auto",
          padding: "18px 20px", borderRadius: 14,
          background: "rgba(8, 13, 18, 0.88)",
          backdropFilter: "blur(22px) saturate(1.05)", WebkitBackdropFilter: "blur(22px) saturate(1.05)",
          border: "1px solid rgba(215,228,230,0.14)",
          boxShadow: "0 18px 48px rgba(2,3,5,0.5)",
          display: "flex", flexDirection: "column", gap: 12,
        }}>
        <div style={{ display: "flex", alignItems: "center" }}>
          <div style={{ fontSize: 13, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>任务详情</div>
          <span style={{ marginLeft: 8, fontSize: 10, color: saveState === "error" ? "rgba(231,164,145,0.95)" : saveState === "saved" ? C.silver : C.textMuted, fontFamily: "var(--font-sans)" }}>
            {saveState === "saving" ? "保存中…" : saveState === "saved" ? "已保存" : saveState === "error" ? "保存失败，可重试" : restoredDraft ? "已恢复未保存草稿" : dirty ? "未保存" : ""}
          </span>
          <button type="button" onClick={requestClose} aria-label="关闭任务详情" className="btn-delete"
            style={{ marginLeft: "auto", width: 24, height: 24, borderRadius: 6, background: "transparent", border: "1px solid transparent", color: C.textMuted, cursor: "pointer" }}>×</button>
        </div>

        {progress && task.targetSeconds > 0 && (
          <div style={{ padding: "8px 10px", ...CARD, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)" }}>
            {formatProgressLine(task, progress)}
            {task.budgetSource !== "creation" && (
              <span style={{ marginLeft: 6 }}>
                （{task.budgetSource === "migration" ? "按迁移时基准推算" : "手动调整过的预算"}）
              </span>
            )}
          </div>
        )}

        <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)" }}>
          标题
          <input value={title} onChange={e => setTitle(e.target.value)} style={field} aria-label="任务标题" />
        </label>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", flex: 1, minWidth: 120 }}>
            项目
            <SearchablePicker value={projectId} onChange={setProjectId} options={projectOptions}
              ariaLabel="所属项目" emptyLabel="独立任务" placeholder="搜索项目…" />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", flex: 1, minWidth: 100 }}>
            标签
            <SearchablePicker value={tagId} onChange={setTagId} options={tagOptions}
              ariaLabel="任务标签" emptyLabel="未设置" placeholder="搜索标签…" />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", flex: 1, minWidth: 90 }}>
            优先级
            <select value={priority} onChange={e => setPriority(e.target.value as TaskPriority)} aria-label="优先级"
              style={{ ...field, cursor: "pointer" }}>
              <option value="">未设置</option>
              <option value="high">高</option>
              <option value="med">中</option>
              <option value="low">低</option>
            </select>
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", width: 88 }}>
            预计番茄
            <input type="number" min={1} max={99} value={pomodoro}
              onChange={e => setPomodoro(Number(e.target.value) || 1)}
              aria-label="预计番茄数" style={{ ...field, fontFamily: "var(--font-mono)" }} />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", width: 88 }}>
            预算分钟
            <input type="number" min={1} max={24 * 60} value={budgetMinutes}
              onChange={e => setBudgetMinutes(e.target.value)}
              aria-label="预算分钟" style={{ ...field, fontFamily: "var(--font-mono)" }} />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", width: 140 }}>
            截止日期
            <DatePicker value={deadline} onChange={setDeadline} />
          </label>
        </div>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)" }}>
          备注
          <textarea value={notes} onChange={e => setNotes(e.target.value)} rows={3} aria-label="备注"
            style={{ ...field, resize: "vertical" }} />
        </label>

        {error && (
          <div role="alert" style={{ fontSize: 11, color: "#ffb3b3", fontFamily: "var(--font-sans)" }}>{error}</div>
        )}

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button onClick={onArchive}
            style={{
              padding: "6px 12px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
              cursor: "pointer", color: "rgba(195,212,218,0.85)",
              background: "rgba(27,37,44,0.40)", border: "1px solid rgba(215,228,230,0.12)",
            }}>归档</button>
          {!task.done && (
            <button onClick={() => void onComplete()}
              style={{
                padding: "6px 12px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
                cursor: "pointer", color: C.moonlight,
                background: "rgba(27,37,44,0.40)", border: `1px solid ${C.hairlineStr}`,
              }}>完成此任务</button>
          )}
          <span style={{ flex: 1 }} />
          <button onClick={onToggleDone}
            style={{
              padding: "6px 12px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
              cursor: "pointer", color: "rgba(195,212,218,0.85)",
              background: "rgba(27,37,44,0.40)", border: "1px solid rgba(215,228,230,0.12)",
            }}>{task.done ? "重新打开" : "标记完成"}</button>
          <button onClick={() => void save()} disabled={saving || !dirty}
            style={{
              padding: "6px 14px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
              cursor: dirty ? "pointer" : "default", color: "#0B1116",
              background: dirty ? "rgba(186,200,204,0.92)" : "rgba(186,200,204,0.35)",
              border: "1px solid rgba(215,228,230,0.30)", fontWeight: 500, opacity: saving ? 0.6 : 1,
            }}>{saving ? "保存中…" : "保存"}</button>
        </div>
      </div>
    </div>
  );
}
