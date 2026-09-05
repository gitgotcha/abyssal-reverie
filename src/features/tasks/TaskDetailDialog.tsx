import { useState } from "react";
import type { Category, Project, Task, TaskPriority, TaskProgress, UpdateTaskInput } from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { PRIORITY_LABELS, formatProgressLine } from "./TasksPanel";

/** v1.2 F2: task detail — edit metadata, save with visible state, complete or
 *  archive. Failure keeps the dialog open with the error inline. */
export function TaskDetailDialog({ task, progress, categories, projects, onClose, onSave, onToggleDone, onArchive, onComplete }: {
  task: Task;
  progress?: TaskProgress;
  categories: Category[];
  projects: Project[];
  onClose: () => void;
  onSave: (patch: Omit<UpdateTaskInput, "id">) => Promise<unknown>;
  onToggleDone: () => void;
  onArchive: () => void;
  onComplete: () => Promise<unknown>;
}) {
  const [title, setTitle] = useState(task.title);
  const [projectId, setProjectId] = useState(task.projectId ?? "");
  const [pomodoro, setPomodoro] = useState(task.pomodoroTarget);
  const [priority, setPriority] = useState<TaskPriority>(task.priority);
  const [deadline, setDeadline] = useState(task.deadline ?? "");
  const [notes, setNotes] = useState(task.notes);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const dirty =
    title !== task.title || projectId !== (task.projectId ?? "") ||
    pomodoro !== task.pomodoroTarget || priority !== task.priority ||
    deadline !== (task.deadline ?? "") || notes !== task.notes;

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await onSave({
        title,
        projectId,
        pomodoroTarget: pomodoro,
        priority,
        deadline,
        notes,
      });
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
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
    <div role="dialog" aria-modal="true" aria-label="任务详情" onClick={onClose}
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
        <div style={{ fontSize: 13, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>任务详情</div>

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
            <select value={projectId} onChange={e => setProjectId(e.target.value)} aria-label="所属项目"
              style={{ ...field, cursor: "pointer" }}>
              <option value="">独立任务</option>
              {categories.filter(c => c.status === "active").map(cat => (
                <optgroup key={cat.id} label={cat.name}>
                  {projects.filter(p => p.categoryId === cat.id && p.status !== "archived").map(p => (
                    <option key={p.id} value={p.id}>{p.name}</option>
                  ))}
                </optgroup>
              ))}
            </select>
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", flex: 1, minWidth: 90 }}>
            优先级
            <select value={priority} onChange={e => setPriority(e.target.value as TaskPriority)} aria-label="优先级"
              style={{ ...field, cursor: "pointer" }}>
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
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", width: 140 }}>
            截止日期
            <input type="date" value={deadline} onChange={e => setDeadline(e.target.value)}
              aria-label="截止日期" style={{ ...field }} />
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
