import { useRef, useState } from "react";
import type { CreateTaskInput, Tag, TagDeletePreview, Task, TaskPriority } from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { HorizonDivider } from "../timer/GoalRing";
import { TagManager } from "../tags/TagManager";

function PriorityPip({ p }: { p: TaskPriority }) {
  const colors: Record<TaskPriority, string> = {
    high: "rgba(190,120,120,0.80)", med: "rgba(170,145,108,0.80)", low: "rgba(158,173,178,0.70)",
  };
  return <span style={{ width: 5, height: 5, borderRadius: "50%", background: colors[p], display: "inline-block", flexShrink: 0 }} />;
}

export function TasksPanel({ tasks, tags, onCreateTask, onToggleTask, onDeleteTask, onCyclePriority, onNotify, tagOps }: {
  tasks: Task[];
  tags: Tag[];
  onCreateTask: (input: CreateTaskInput) => Promise<unknown>;
  onToggleTask: (id: string) => Promise<unknown>;
  onDeleteTask: (id: string) => Promise<unknown>;
  onCyclePriority: (id: string) => Promise<unknown>;
  /** v1.1.2 A1: create failures surface here; the draft is kept for retry. */
  onNotify?: (message: string) => void;
  tagOps: {
    createTag: (name: string) => Promise<unknown>;
    renameTag: (id: string, name: string) => Promise<unknown>;
    reorderTag: (id: string, direction: number) => Promise<unknown>;
    previewDeleteTag: (id: string) => Promise<TagDeletePreview>;
    deleteTag: (id: string) => Promise<unknown>;
  };
}) {

  const [newTitle, setNewTitle] = useState("");
  const [formTagId, setFormTagId]   = useState("");
  const [formProject, setFormProject] = useState("通用");
  const [formPriority, setFormPriority] = useState<TaskPriority>("med");
  const [formPomodoro, setFormPomodoro] = useState(1);
  const [statusFilter, setStatusFilter] = useState<"all"|"active"|"done">("all");
  const [tagFilter, setTagFilter]     = useState<string>("all");
  const [managerOpen, setManagerOpen] = useState(false);
  // v1.1.2 A1: serial saves — a second submit is ignored until the current
  // create settles, and the title clears only on success.
  const [submitting, setSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const fallbackId = tags.find(t => t.isFallback)?.id ?? "system-other";
  const effectiveTagId = formTagId || fallbackId;

  const addTask = async () => {
    if (submitting) return;
    const title = newTitle.trim(); if (!title) return;
    setSubmitting(true);
    try {
      await onCreateTask({
        title,
        tagId: effectiveTagId,
        project: formProject.trim() || "通用",
        priority: formPriority,
        pomodoroTarget: Math.max(1, Math.min(99, formPomodoro || 1)),
      });
      setNewTitle("");
    } catch (err) {
      onNotify?.(`任务创建失败，草稿已保留：${err instanceof Error ? err.message : String(err)}`);
      inputRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  };
  const toggleTask    = (id: string) => { void onToggleTask(id); };
  const deleteTask    = (id: string) => { void onDeleteTask(id); };
  const cyclePriority = (id: string) => { void onCyclePriority(id); };

  const tagName = (id: string | null) => (id ? tags.find(t => t.id === id)?.name : undefined);

  const statusFiltered = tasks.filter(t => statusFilter==="all" ? true : statusFilter==="active" ? !t.done : t.done);
  const filtered = tagFilter === "all"
    ? statusFiltered
    : statusFiltered.filter(t => t.tagId === tagFilter);
  const fLabels = { all:"全部", active:"进行中", done:"已完成" } as const;
  const tagChip = (id: string, label: string) => (
    <button key={id} onClick={() => setTagFilter(id)} className="btn-filter"
      aria-pressed={tagFilter === id}
      style={{
        fontFamily: "var(--font-sans)", fontSize: 10,
        padding: "2px 8px", borderRadius: 6,
        border: `0.5px solid ${tagFilter===id ? C.hairlineStr : "transparent"}`,
        background: tagFilter===id ? "rgba(27,37,44,0.38)" : "transparent",
        color: tagFilter===id ? C.moonlight : C.textMuted,
        cursor: "pointer", /* focus rings come from index.css (:focus / :focus-visible) */
      }}>{label}</button>
  );
  const smallField = {
    fontFamily: "var(--font-sans)", fontSize: 11, color: C.textPrimary,
    background: C.cardDim, border: `1px solid ${C.hairline}`,
    borderRadius: 7, padding: "5px 7px", cursor: "pointer",
  } as const;

  return (
    <div className="flex flex-col h-full" style={{ position: "relative", zIndex: 2 }}>
      <div style={{ flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 22px" }}>
          <span style={{ fontSize: 13, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>任务</span>
          <span style={{
            fontFamily: "var(--font-mono)", fontSize: 10, color: C.textMuted,
            background: C.cardDim, border: `1px solid ${C.hairline}`,
            borderRadius: 5, padding: "2px 6px",
          }}>{tasks.filter(t=>!t.done).length}</span>
          <button onClick={() => setManagerOpen(true)} className="btn-filter"
            title="创建、重命名、排序或删除标签"
            style={{
              marginLeft: "auto", fontFamily: "var(--font-sans)", fontSize: 10,
              padding: "3px 9px", borderRadius: 6,
              border: `0.5px solid ${C.hairlineStr}`,
              background: "rgba(27,37,44,0.30)",
              color: C.moonlight, cursor: "pointer", /* focus rings come from index.css (:focus / :focus-visible) */
            }}>管理标签</button>
          <div style={{ display: "flex", gap: 3 }}>
            {(["all","active","done"] as const).map(f => (
              <button key={f} onClick={() => setStatusFilter(f)} className="btn-filter"
                aria-pressed={statusFilter===f}
                style={{
                  fontFamily: "var(--font-sans)", fontSize: 11,
                  padding: "3px 9px", borderRadius: 6,
                  border: `0.5px solid ${statusFilter===f ? C.hairlineStr : "transparent"}`,
                  background: statusFilter===f ? "rgba(27,37,44,0.38)" : "transparent",
                  color: statusFilter===f ? C.moonlight : C.textMuted,
                  cursor: "pointer", /* focus rings come from index.css (:focus / :focus-visible) */
                }}>{fLabels[f]}</button>
            ))}
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 4, padding: "4px 22px 7px", flexWrap: "wrap" }}>
          {tagChip("all", "全部标签")}
          {tags.map(t => tagChip(t.id, t.name))}
        </div>
        <HorizonDivider />
      </div>

      <div style={{ flexShrink: 0 }}>
        <div style={{ display: "flex", gap: 7, padding: "9px 22px 5px" }}>
          <input
            ref={inputRef}
            value={newTitle} onChange={e => setNewTitle(e.target.value)}
            onKeyDown={e => {
              // v1.1.2 A2: an Enter that confirms IME candidates must never
              // submit — the composition event reports isComposing (and
              // legacy keyCode 229).
              if (e.key === "Enter" && !e.nativeEvent.isComposing && e.keyCode !== 229) void addTask();
            }}
            placeholder="添加任务…" aria-label="任务标题" className="input-ocean"
            style={{ flex: 1, ...CARD, borderRadius: 10, padding: "8px 12px", fontSize: 12, color: C.textPrimary, fontFamily: "var(--font-sans)" }}
          />
          <button onClick={() => void addTask()} disabled={submitting} aria-busy={submitting} className="btn-add" aria-label="添加任务"
            style={{
              width: 34, height: 34, borderRadius: 9, flexShrink: 0,
              background: "rgba(27,37,44,0.36)",
              backdropFilter: "blur(14px)", WebkitBackdropFilter: "blur(14px)",
              border: `1px solid ${C.hairlineStr}`,
              color: C.moonlight, cursor: "pointer",
              display: "flex", alignItems: "center", justifyContent: "center", /* focus rings come from index.css (:focus / :focus-visible) */
            }}>
            <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
              <path d="M7 2V12M2 7H12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 22px 9px", flexWrap: "wrap" }}>
          <select value={effectiveTagId} onChange={e => setFormTagId(e.target.value)}
            aria-label="任务标签" style={{ ...smallField, minWidth: 74 }}>
            {tags.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
          <input value={formProject} onChange={e => setFormProject(e.target.value)}
            aria-label="所属项目" placeholder="项目"
            style={{ ...smallField, width: 84 }} />
          <select value={formPriority} onChange={e => setFormPriority(e.target.value as TaskPriority)}
            aria-label="优先级" style={{ ...smallField, minWidth: 64 }}>
            <option value="high">高</option>
            <option value="med">中</option>
            <option value="low">低</option>
          </select>
          <input type="number" min={1} max={99} value={formPomodoro}
            onChange={e => setFormPomodoro(Number(e.target.value) || 1)}
            aria-label="预计番茄数"
            style={{ ...smallField, width: 46, fontFamily: "var(--font-mono)" }} />
          <span style={{ fontSize: 9, color: C.textMuted, fontFamily: "var(--font-sans)" }}>个番茄</span>
        </div>
        <HorizonDivider />
      </div>

      <div className="flex-1 overflow-y-auto" style={{ padding: "6px 22px" }}>
        {filtered.length === 0 ? (
          <div style={{ display:"flex", flexDirection:"column", alignItems:"center", justifyContent:"center", height:"100%", gap:8, opacity:0.24, paddingBottom:40 }}>
            <svg width="24" height="24" viewBox="0 0 30 30" fill="none">
              <rect x="3" y="7" width="24" height="2" rx="1" fill={C.silver} />
              <rect x="3" y="14" width="17" height="2" rx="1" fill={C.silver} />
              <rect x="3" y="21" width="11" height="2" rx="1" fill={C.silver} />
            </svg>
            <span style={{ fontSize:11, color:C.textSec, fontFamily:"var(--font-sans)" }}>
              {tagFilter !== "all" ? "该标签下暂无任务" : statusFilter==="done" ? "尚无已完成任务" : "暂无任务"}
            </span>
          </div>
        ) : (
          <div style={{ display:"flex", flexDirection:"column", gap:5, paddingTop:7, paddingBottom:7 }}>
            {filtered.map(task => {
              const taskTagName = tagName(task.tagId);
              return (
              <div key={task.id} className="slide-in task-item"
                style={{ display:"flex", alignItems:"center", gap:9, padding:"9px 12px", ...CARD }}>
                <button onClick={() => toggleTask(task.id)} className="btn-check"
                  aria-label={task.done ? "取消完成" : "标记完成"}
                  style={{
                    width:15, height:15, borderRadius:5, flexShrink:0,
                    border:`1.5px solid ${task.done ? C.silver : "rgba(215,228,230,0.14)"}`,
                    background: task.done ? "rgba(158,173,178,0.10)" : "transparent",
                    cursor:"pointer", display:"flex", alignItems:"center", justifyContent:"center",
                  }}>
                  {task.done && (
                    <svg width="7" height="7" viewBox="0 0 9 9" fill="none">
                      <path d="M1.5 4.5L3.5 6.5L7.5 2.5" stroke={C.silver} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  )}
                </button>
                <button onClick={() => cyclePriority(task.id)}
                  aria-label="切换优先级"
                  style={{ background:"none", border:"none", cursor:"pointer", padding:0, display:"flex" }}>
                  <PriorityPip p={task.priority} />
                </button>
                <span style={{
                  flex:1, fontSize:12, fontFamily:"var(--font-sans)", lineHeight:1.4,
                  color: task.done ? "rgba(165,182,188,0.26)" : C.textSec,
                  textDecoration: task.done ? "line-through" : "none",
                  textDecorationColor: "rgba(165,182,188,0.26)",
                  transition: "all 0.22s",
                }}>{task.title}</span>
                {taskTagName && (
                  <span title={`标签：${taskTagName}`} style={{
                    fontFamily:"var(--font-sans)", fontSize:9, flexShrink:0,
                    color:"rgba(170,190,196,0.55)",
                    background:"rgba(27,37,44,0.20)", border:`0.5px solid ${C.hairline}`,
                    padding:"1px 6px", borderRadius:8,
                  }}>{taskTagName}</span>
                )}
                <span style={{
                  fontFamily:"var(--font-sans)", fontSize:9, color:C.textMuted,
                  background:"rgba(27,37,44,0.26)", border:`1px solid ${C.hairline}`,
                  padding:"1px 5px", borderRadius:4, flexShrink:0,
                }}>{task.project}</span>
                <span style={{ fontFamily:"var(--font-mono)", fontSize:9, color:C.textMuted }}>×{task.pomodoroTarget}</span>
                <button onClick={() => deleteTask(task.id)} className="btn-delete"
                  aria-label="删除任务"
                  style={{
                    width:20, height:20, borderRadius:4, flexShrink:0,
                    background:"none", border:"1px solid transparent",
                    color:"rgba(215,228,230,0.14)", cursor:"pointer",
                    display:"flex", alignItems:"center", justifyContent:"center",
                  }}>
                  <svg width="8" height="8" viewBox="0 0 10 10" fill="none">
                    <path d="M2 2L8 8M8 2L2 8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
                  </svg>
                </button>
              </div>
            );
            })}
          </div>
        )}
      </div>

      <TagManager
        open={managerOpen}
        tags={tags}
        onClose={() => setManagerOpen(false)}
        onCreate={tagOps.createTag}
        onRename={tagOps.renameTag}
        onReorder={tagOps.reorderTag}
        onPreviewDelete={tagOps.previewDeleteTag}
        onDelete={tagOps.deleteTag}
      />
    </div>
  );
}
