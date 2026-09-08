import { useEffect, useRef, useState } from "react";
import type {
  Category, CreateTaskInput, Project, RelationshipPatch, Tag, TagDeletePreview,
  Task, TaskProgress, UpdateTaskInput,
} from "../../domain/models";
import { C, SIDEBAR_GLASS } from "../shared/palette";
import { ProjectManager } from "../tasks/ProjectManager";
import { TasksPanel } from "../tasks/TasksPanel";
import { TagManager } from "../tags/TagManager";
import { loadManagementState, saveManagementState, type ManagementState } from "./managementState";

export type ManagementView = "tasks" | "projects" | "tags";

export interface ManagementPanelProps {
  tasks: Task[];
  tags: Tag[];
  categories: Category[];
  projects: Project[];
  progress: Record<string, TaskProgress>;
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
}

/**
 * Management is a single destination in the app shell. Tasks, projects and
 * tags are sibling views inside it, so the global sidebar stays quiet while
 * users still have an obvious place to maintain each entity.
 */
export function ManagementPanel(props: ManagementPanelProps) {
  const [managementState, setManagementState] = useState<ManagementState>(() => loadManagementState());
  const rootRef = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);
  const view = managementState.view;
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const update = () => setCompact(root.getBoundingClientRect().width < 680);
    update();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", update);
      return () => window.removeEventListener("resize", update);
    }
    const observer = new ResizeObserver(update);
    observer.observe(root);
    return () => observer.disconnect();
  }, []);
  const setView = (next: ManagementView) => {
    setManagementState(current => {
      const updated = { ...current, view: next };
      saveManagementState(updated);
      return updated;
    });
  };
  const items: Array<[ManagementView, string, string]> = [
    ["tasks", "任务", "管理任务、预算与截止日期"],
    ["projects", "项目", "管理项目与类别"],
    ["tags", "标签", "管理标签与排序"],
  ];

  return (
    <div ref={rootRef} style={{ display: "flex", flexDirection: compact ? "column" : "row", flex: 1, minWidth: 0, minHeight: 0 }} aria-label="管理面板">
      <aside style={{
        width: compact ? "100%" : 142, height: compact ? 54 : undefined, flexShrink: 0, padding: compact ? "7px 10px" : "18px 10px", display: "flex",
        flexDirection: compact ? "row" : "column", alignItems: compact ? "center" : undefined, gap: 5, ...SIDEBAR_GLASS,
        borderRight: compact ? "none" : `1px solid ${C.hairline}`, borderBottom: compact ? `1px solid ${C.hairline}` : "none",
      }}>
        <div style={{ padding: compact ? "0 8px" : "0 8px 12px", color: C.textMuted, fontSize: 10,
          fontFamily: "var(--font-sans)", letterSpacing: "0.08em" }}>管理</div>
        {items.map(([id, label, hint]) => {
          const active = view === id;
          return (
            <button key={id} type="button" onClick={() => setView(id)} aria-current={active ? "page" : undefined}
              title={hint} style={{
                border: `1px solid ${active ? C.hairlineStr : "transparent"}`,
                background: active ? "rgba(215,228,230,0.10)" : "transparent",
                color: active ? C.textPrimary : C.textMuted,
                borderRadius: 9, padding: compact ? "7px 13px" : "9px 10px", textAlign: compact ? "center" : "left",
                fontSize: 12, fontFamily: "var(--font-sans)", cursor: "pointer",
                transition: "background-color 140ms, color 140ms, border-color 140ms",
              }}>{label}</button>
          );
        })}
        <div style={{ marginTop: compact ? 0 : "auto", marginLeft: compact ? "auto" : undefined, padding: compact ? "0 8px" : "10px 8px 0", color: C.textMuted,
          fontSize: 9, lineHeight: 1.5, fontFamily: "var(--font-sans)" }}>
          {!compact && "任务可以独立存在；需要时再绑定项目、标签和截止日期。"}
        </div>
      </aside>
      <section style={{ flex: 1, minWidth: 0, minHeight: 0, display: "flex", overflow: "hidden" }}>
        {view === "tasks" && <TasksPanel {...props} showManagementActions={false}
          initialSearch={managementState.taskSearch}
          initialStatusFilter={managementState.taskStatusFilter}
          initialTagFilter={managementState.taskTagFilter}
          initialScrollTop={managementState.taskScrollTop}
          onManagementStateChange={patch => setManagementState(current => {
            const updated = { ...current, ...patch };
            saveManagementState(updated);
            return updated;
          })} />}
        {view === "projects" && (
          <ProjectManager open embedded categories={props.categories} projects={props.projects}
            onClose={() => setView("tasks")} {...props.projectOps} />
        )}
        {view === "tags" && (
          <TagManager open embedded tags={props.tags}
            onClose={() => setView("tasks")}
            onCreate={props.tagOps.createTag}
            onRename={props.tagOps.renameTag}
            onReorder={props.tagOps.reorderTag}
            onPreviewDelete={props.tagOps.previewDeleteTag}
            onDelete={props.tagOps.deleteTag} />
        )}
      </section>
    </div>
  );
}
