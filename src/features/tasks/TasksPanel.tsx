import { useEffect, useMemo, useRef, useState } from "react";
import type {
  Category, CreateTaskInput, Project, RelationshipPatch, Tag, TagDeletePreview, Task, TaskPriority, TaskProgress,
  UpdateTaskInput,
} from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { HorizonDivider } from "../timer/GoalRing";
import { TagManager } from "../tags/TagManager";
import { TaskDetailDialog } from "./TaskDetailDialog";
import { ProjectManager } from "./ProjectManager";
import { todayLocalDate } from "../../domain/localDate";
import { searchEntities } from "../../domain/search";
import { DatePicker } from "../../components/DatePicker";
import type { ManagementState, ManagementTaskStatus } from "../management/managementState";
import { SearchablePicker, type PickerOption } from "../shared/SearchablePicker";

function PriorityPip({ p }: { p: TaskPriority | null }) {
  const colors: Record<TaskPriority, string> = {
    high: "rgba(190,120,120,0.80)", med: "rgba(170,145,108,0.80)", low: "rgba(158,173,178,0.70)",
  };
  // v5: null = 未设置 → hollow dot instead of a colored pip.
  return <span style={{ width: 5, height: 5, borderRadius: "50%", background: p ? colors[p] : "transparent", border: p ? "none" : "1px solid rgba(200,214,216,0.40)", display: "inline-block", flexShrink: 0 }} />;
}

export const PRIORITY_LABELS: Record<TaskPriority, string> = { high: "高", med: "中", low: "低" };

export function formatProgressLine(task: Task, progress?: TaskProgress): string {
  if (!progress || task.targetSeconds <= 0) return "";
  const total = progress.confirmedSeconds + progress.provisionalSeconds;
  const pct = Math.round(progress.progress * 100);
  const base = `累计 ${Math.round(total / 60)}/${Math.round(task.targetSeconds / 60)} 分钟 · ${pct}%`;
  if (progress.overSeconds > 0) return `${base} · 超出预计 ${Math.round(progress.overSeconds / 60)} 分钟`;
  if (pct >= 100) return `${base} · 已达预计`;
  return base;
}

export function TasksPanel({
  tasks, tags, categories, projects, progress, focusDurationMinutes, onCreateTask, onToggleTask,
  onArchiveTask, onStartFocus, onCompleteTask, onCyclePriority, onUpdateTask, onApplyTaskRelationship, onNotify, tagOps,
  projectOps, showManagementActions = true,
  initialSearch = "", initialStatusFilter = "todo", initialTagFilter = "all", initialScrollTop = 0,
  onManagementStateChange,
}: {
  tasks: Task[];
  tags: Tag[];
  categories: Category[];
  projects: Project[];
  progress: Record<string, TaskProgress>;
  /** 任务 2.1：已保存的专注时长（分钟）——新建换算提示的唯一来源，杜绝写死 25。 */
  focusDurationMinutes: number;
  onCreateTask: (input: CreateTaskInput) => Promise<unknown>;
  onToggleTask: (id: string) => Promise<unknown>;
  onArchiveTask: (id: string, archived: boolean) => Promise<unknown>;
  onStartFocus: (taskId: string) => void;
  onCompleteTask: (taskId: string) => Promise<unknown>;
  onCyclePriority: (id: string) => Promise<unknown>;
  onUpdateTask: (input: UpdateTaskInput) => Promise<unknown>;
  onApplyTaskRelationship: (patch: RelationshipPatch) => Promise<unknown>;
  onNotify?: (message: string) => void;
  showManagementActions?: boolean;
  initialSearch?: string;
  initialStatusFilter?: ManagementTaskStatus;
  initialTagFilter?: string;
  initialScrollTop?: number;
  onManagementStateChange?: (patch: Partial<Pick<ManagementState, "taskSearch" | "taskStatusFilter" | "taskTagFilter" | "taskScrollTop">>) => void;
  tagOps: {
    createTag: (name: string) => Promise<unknown>;
    renameTag: (id: string, name: string) => Promise<unknown>;
    reorderTag: (id: string, direction: number) => Promise<unknown>;
    previewDeleteTag: (id: string) => Promise<TagDeletePreview>;
    deleteTag: (id: string) => Promise<unknown>;
  };
  projectOps: {
    createProject: (name: string, categoryId: string) => Promise<unknown>;
    renameProject: (id: string, name: string) => Promise<unknown>;
    archiveProject: (id: string, archived: boolean) => Promise<unknown>;
    createCategory: (name: string) => Promise<unknown>;
    renameCategory: (id: string, name: string) => Promise<unknown>;
    archiveCategory: (id: string, archived: boolean) => Promise<unknown>;
    moveProject: (id: string, categoryId: string) => Promise<unknown>;
  };
}) {

  const [newTitle, setNewTitle] = useState("");
  const [formTagId, setFormTagId]   = useState("");
  const [formProjectId, setFormProjectId] = useState("");
  const [formPriority, setFormPriority] = useState<TaskPriority | "">("");
  const [formPomodoro, setFormPomodoro] = useState(1);
  const [formDeadline, setFormDeadline] = useState("");
  const [formNotes, setFormNotes] = useState("");
  const [moreOpen, setMoreOpen] = useState(false);
  const [search, setSearch] = useState(initialSearch);
  const [statusFilter, setStatusFilter] = useState<ManagementTaskStatus>(initialStatusFilter);
  const [tagFilter, setTagFilter]     = useState<string>(initialTagFilter);
  const [managerOpen, setManagerOpen] = useState(false);
  const [projectOpen, setProjectOpen] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [today, setToday] = useState(() => todayLocalDate());
  const inputRef = useRef<HTMLInputElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = initialScrollTop;
  }, [initialScrollTop]);

  useEffect(() => {
    onManagementStateChange?.({ taskSearch: search, taskStatusFilter: statusFilter, taskTagFilter: tagFilter });
  }, [search, statusFilter, tagFilter]);

  // Deadline state must refresh even while the page is otherwise idle. The
  // local-midnight timer covers an open window; visibility/focus refreshes
  // cover sleep, hibernation and tray-to-window restoration.
  useEffect(() => {
    let timer: number | undefined;
    const scheduleMidnightRefresh = () => {
      setToday(todayLocalDate());
      const now = new Date();
      const next = new Date(now);
      next.setHours(24, 0, 0, 0);
      timer = window.setTimeout(scheduleMidnightRefresh, Math.max(1000, next.getTime() - now.getTime() + 50));
    };
    const refreshOnVisible = () => {
      if (!document.hidden) setToday(todayLocalDate());
    };
    scheduleMidnightRefresh();
    document.addEventListener("visibilitychange", refreshOnVisible);
    window.addEventListener("focus", refreshOnVisible);
    return () => {
      if (timer !== undefined) window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", refreshOnVisible);
      window.removeEventListener("focus", refreshOnVisible);
    };
  }, []);

  // 任务 2.1（红灯①转绿）：提示由已保存设置权威派生（原 `projects.length >= 0 ? 25 : 25`）。
  const minutePerPomodoro = focusDurationMinutes;

  const addTask = async () => {
    if (submitting) return;
    const title = newTitle.trim(); if (!title) return;
    setSubmitting(true);
    try {
      await onCreateTask({
        title,
        projectId: formProjectId || undefined,
        tagId: formTagId || undefined,
        priority: formPriority || undefined,
        pomodoroTarget: Math.max(1, Math.min(99, formPomodoro || 1)),
        deadline: formDeadline || undefined,
        notes: formNotes || undefined,
      });
      setNewTitle("");
      setFormDeadline("");
      setFormNotes("");
    } catch (err) {
      onNotify?.(`任务创建失败，草稿已保留：${err instanceof Error ? err.message : String(err)}`);
      inputRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  };

  const tagName = (id: string | null) => (id ? tags.find(t => t.id === id)?.name : undefined);
  const projectOptions: PickerOption[] = categories.filter(c => c.status === "active").flatMap(category =>
    projects.filter(project => project.categoryId === category.id && project.status !== "archived")
      .map(project => ({ value: project.id, label: project.name, group: category.name }))
  );
  const tagOptions: PickerOption[] = tags.map(tag => ({ value: tag.id, label: tag.name }));

  // Combined filtering: keyword / status / tag. Scroll position is preserved
  // because the list container never unmounts across filter tweaks.
  const filtered = useMemo(() => {
    const kw = search.trim();
    const candidates = tasks.filter(t => {
      if (t.status === "archived" && statusFilter !== "all") return false;
      if (statusFilter === "todo" && (t.status === "done" || t.status === "archived")) return false;
      if (statusFilter === "done" && t.status !== "done") return false;
      if (tagFilter !== "all" && t.tagId !== tagFilter) return false;
      return true;
    });
    return kw
      ? searchEntities(candidates, kw, t => [t.title, t.project, tagName(t.tagId) ?? ""], t => t.id)
      : candidates;
  }, [tasks, tags, search, statusFilter, tagFilter]);
  const filtersActive = search.trim() !== "" || statusFilter !== "todo" || tagFilter !== "all";

  const detailTask = tasks.find(t => t.id === detailId) ?? null;

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
          }}>{tasks.filter(t => t.status === "todo").length}</span>
          {showManagementActions && <button onClick={() => setProjectOpen(true)} className="btn-filter"
            title="创建项目与类别"
            style={{
              marginLeft: "auto", fontFamily: "var(--font-sans)", fontSize: 10,
              padding: "3px 9px", borderRadius: 6,
              border: `0.5px solid ${C.hairlineStr}`,
              background: "rgba(27,37,44,0.30)",
              color: C.moonlight, cursor: "pointer",
            }}>管理项目</button>}
          {showManagementActions && <button onClick={() => setManagerOpen(true)} className="btn-filter"
            title="创建、重命名、排序或删除标签"
            style={{
              fontFamily: "var(--font-sans)", fontSize: 10,
              padding: "3px 9px", borderRadius: 6,
              border: `0.5px solid ${C.hairlineStr}`,
              background: "rgba(27,37,44,0.30)",
              color: C.moonlight, cursor: "pointer",
            }}>管理标签</button>}
          <div style={{ display: "flex", gap: 3 }}>
            {([["todo","待办"],["done","已完成"],["all","全部"]] as const).map(([f, label]) => (
              <button key={f} onClick={() => setStatusFilter(f)} className="btn-filter"
                aria-pressed={statusFilter===f}
                style={{
                  fontFamily: "var(--font-sans)", fontSize: 11,
                  padding: "3px 9px", borderRadius: 6,
                  border: `0.5px solid ${statusFilter===f ? C.hairlineStr : "transparent"}`,
                  background: statusFilter===f ? "rgba(27,37,44,0.38)" : "transparent",
                  color: statusFilter===f ? C.moonlight : C.textMuted,
                  cursor: "pointer",
                }}>{label}</button>
            ))}
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 4, padding: "4px 22px 7px", flexWrap: "wrap" }}>
          <input
            ref={searchRef}
            value={search} onChange={e => setSearch(e.target.value)}
            placeholder="搜索任务、项目…" aria-label="搜索任务" className="input-ocean"
            style={{ ...smallField, flex: 1, minWidth: 140, cursor: "text", padding: "4px 9px" }}
          />
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
              // v1.1.2 A2: an Enter that confirms IME candidates must never submit.
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
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
            <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
              <path d="M7 2V12M2 7H12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 22px 9px", flexWrap: "wrap" }}>
          <span style={{ width: 126, flexShrink: 0 }}>
            <SearchablePicker value={formProjectId} onChange={setFormProjectId} options={projectOptions}
              ariaLabel="所属项目" emptyLabel="独立任务" placeholder="搜索项目…" />
          </span>
          <select value={formPriority} onChange={e => setFormPriority(e.target.value as TaskPriority)}
            aria-label="优先级" style={{ ...smallField, minWidth: 64 }}>
            <option value="">未设置</option>
            <option value="high">高</option>
            <option value="med">中</option>
            <option value="low">低</option>
          </select>
          <input type="number" min={1} max={99} value={formPomodoro}
            onChange={e => setFormPomodoro(Number(e.target.value) || 1)}
            aria-label="预计番茄数"
            style={{ ...smallField, width: 46, fontFamily: "var(--font-mono)" }} />
          <span style={{ fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)" }}>
            个番茄 = {formPomodoro * minutePerPomodoro} 分钟
          </span>
          <button onClick={() => setMoreOpen(o => !o)} className="btn-filter"
            aria-expanded={moreOpen}
            style={{
              marginLeft: "auto", fontFamily: "var(--font-sans)", fontSize: 10,
              padding: "3px 9px", borderRadius: 6,
              border: `0.5px solid ${moreOpen ? C.hairlineStr : "transparent"}`,
              background: moreOpen ? "rgba(27,37,44,0.38)" : "transparent",
              color: C.textMuted, cursor: "pointer",
            }}>更多选项</button>
        </div>
        {moreOpen && (
          <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 22px 9px", flexWrap: "wrap" }}>
            <span style={{ width: 126, flexShrink: 0 }}>
              <SearchablePicker value={formTagId} onChange={setFormTagId} options={tagOptions}
                ariaLabel="任务标签" emptyLabel="未设置" placeholder="搜索标签…" />
            </span>
            <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)" }}>
              截止
              <span style={{ width: 142 }}>
                <DatePicker value={formDeadline} onChange={setFormDeadline} />
              </span>
            </label>
            <input value={formNotes} onChange={e => setFormNotes(e.target.value)}
              placeholder="备注" aria-label="备注"
              style={{ ...smallField, flex: 1, minWidth: 120, cursor: "text" }} />
          </div>
        )}
        <HorizonDivider />
      </div>

      <div ref={listRef} className="flex-1 overflow-y-auto" onScroll={event => {
        onManagementStateChange?.({ taskScrollTop: event.currentTarget.scrollTop });
      }} style={{ padding: "6px 22px" }}>
        {tasks.length === 0 ? (
          <EmptyHint text="还没有任务，添加第一个吧" />
        ) : filtered.length === 0 ? (
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8, paddingTop: 40 }}>
            <span style={{ fontSize: 11, color: C.textSec, fontFamily: "var(--font-sans)" }}>当前筛选没有结果</span>
            {filtersActive && (
              <button onClick={() => { setSearch(""); setStatusFilter("todo"); setTagFilter("all"); }} className="btn-filter"
                style={{
                  fontFamily: "var(--font-sans)", fontSize: 11, padding: "4px 12px", borderRadius: 6,
                  border: `0.5px solid ${C.hairlineStr}`, background: "rgba(27,37,44,0.30)",
                  color: C.moonlight, cursor: "pointer",
                }}>清除筛选</button>
            )}
          </div>
        ) : (
          <div style={{ display:"flex", flexDirection:"column", gap:5, paddingTop:7, paddingBottom:7 }}>
            {filtered.map(task => {
              const taskTagName = tagName(task.tagId);
              const prog = progress[task.id];
              const line = formatProgressLine(task, prog);
              const pct = prog ? Math.round(prog.progress * 100) : 0;
              const overdue = Boolean(task.deadline && task.deadline < today && !task.done && task.status !== "archived");
              return (
              <div key={task.id} className="slide-in task-item"
                style={{ display:"flex", alignItems:"center", gap:9, padding:"9px 12px", ...CARD, flexWrap:"wrap" }}>
                <button onClick={() => void onToggleTask(task.id)} className="btn-check"
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
                <button onClick={() => void onCyclePriority(task.id)}
                  title={`优先级：${task.priority ? PRIORITY_LABELS[task.priority] : "未设置"}`}
                  aria-label={`切换优先级，当前${task.priority ? PRIORITY_LABELS[task.priority] : "未设置"}`}
                  style={{ background:"none", border:"none", cursor:"pointer", padding:0, display:"flex" }}>
                  <PriorityPip p={task.priority} />
                </button>
                <button onClick={() => setDetailId(task.id)}
                  title="打开任务详情"
                  style={{
                    flex: 1, minWidth: 120, textAlign: "left",
                    background: "none", border: "none", padding: 0, cursor: "pointer",
                    display: "flex", flexDirection: "column", gap: 3,
                  }}>
                  <span style={{
                    fontSize:12, fontFamily:"var(--font-sans)", lineHeight:1.4,
                    color: task.done ? "rgba(165,182,188,0.26)" : C.textSec,
                    textDecoration: task.done ? "line-through" : "none",
                    textDecorationColor: "rgba(165,182,188,0.26)",
                    display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical",
                    overflow: "hidden",
                    transition: "color 0.22s, opacity 0.22s, transform 0.22s",
                  }}>{task.title}</span>
                  {line && (
                    <span style={{ display:"flex", alignItems:"center", gap:6 }}>
                      <span style={{
                        width: 72, height: 3, borderRadius: 2, overflow: "hidden",
                        background: "rgba(215,228,230,0.10)", flexShrink: 0,
                      }}>
                        <span style={{
                          display: "block", height: "100%", width: `${Math.min(100, pct)}%`,
                          background: pct >= 100 ? "rgba(186,200,204,0.85)" : "rgba(158,173,178,0.55)",
                        }} />
                      </span>
                      <span style={{ fontFamily:"var(--font-sans)", fontSize:9, color: C.textMuted }}>{line}</span>
                    </span>
                  )}
                </button>
                {task.deadline && (
                  <span style={{
                    fontFamily:"var(--font-mono)", fontSize:9,
                    color: overdue ? "rgba(231,164,145,0.95)" : C.textMuted, flexShrink:0,
                  }}>{overdue ? "逾期 · " : ""}{task.deadline.slice(5)}</span>
                )}
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
                {!task.done && (
                  <button onClick={() => onStartFocus(task.id)} className="btn-filter"
                    title="选为当前任务并开始专注"
                    style={{
                      fontFamily:"var(--font-sans)", fontSize:9, flexShrink:0,
                      padding:"2px 7px", borderRadius:5,
                      border:`0.5px solid ${C.hairlineStr}`, background:"rgba(27,37,44,0.30)",
                      color:C.moonlight, cursor:"pointer",
                    }}>专注</button>
                )}
                <button onClick={() => void onArchiveTask(task.id, true)} className="btn-delete"
                  title="归档（可撤销）" aria-label="归档任务"
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

      <ProjectManager
        open={projectOpen}
        categories={categories}
        projects={projects}
        onClose={() => setProjectOpen(false)}
        {...projectOps}
      />

      {detailTask && (
        <TaskDetailDialog
          task={detailTask}
          progress={progress[detailTask.id]}
          categories={categories}
          projects={projects}
          tags={tags}
          onClose={() => setDetailId(null)}
          onSave={async patch => {
            await onUpdateTask({ id: detailTask.id, ...patch });
          }}
          onApplyRelationship={onApplyTaskRelationship}
          onToggleDone={() => void onToggleTask(detailTask.id)}
          onArchive={() => void onArchiveTask(detailTask.id, true)}
          onComplete={async () => {
            await onCompleteTask(detailTask.id);
            setDetailId(null);
          }}
        />
      )}
    </div>
  );

  function tagChip(id: string, label: string) {
    return (
      <button key={id} onClick={() => setTagFilter(id)} className="btn-filter"
        aria-pressed={tagFilter === id}
        style={{
          fontFamily: "var(--font-sans)", fontSize: 10,
          padding: "2px 8px", borderRadius: 6,
          border: `0.5px solid ${tagFilter===id ? C.hairlineStr : "transparent"}`,
          background: tagFilter===id ? "rgba(27,37,44,0.38)" : "transparent",
          color: tagFilter===id ? C.moonlight : C.textMuted,
          cursor: "pointer",
        }}>{label}</button>
    );
  }
}

function EmptyHint({ text }: { text: string }) {
  return (
    <div style={{ display:"flex", flexDirection:"column", alignItems:"center", justifyContent:"center", height:"100%", gap:8, paddingBottom:40 }}>
      <svg width="24" height="24" viewBox="0 0 30 30" fill="none">
        <rect x="3" y="7" width="24" height="2" rx="1" fill={C.silver} />
        <rect x="3" y="14" width="17" height="2" rx="1" fill={C.silver} />
        <rect x="3" y="21" width="11" height="2" rx="1" fill={C.silver} />
      </svg>
      <span style={{ fontSize:11, color:C.textSec, fontFamily:"var(--font-sans)" }}>{text}</span>
    </div>
  );
}
