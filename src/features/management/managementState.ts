import type { ManagementView } from "./ManagementPanel";

export type ManagementTaskStatus = "all" | "todo" | "done";

export interface ManagementState {
  view: ManagementView;
  taskSearch: string;
  taskStatusFilter: ManagementTaskStatus;
  taskTagFilter: string;
  taskScrollTop: number;
  returnContext: { view: ManagementView; taskId?: string } | null;
}
export const MANAGEMENT_STATE_KEY = "abyssal-reverie.management-state";

export const DEFAULT_MANAGEMENT_STATE: ManagementState = {
  view: "tasks",
  taskSearch: "",
  taskStatusFilter: "todo",
  taskTagFilter: "all",
  taskScrollTop: 0,
  returnContext: null,
};

export function loadManagementState(): ManagementState {
  if (typeof window === "undefined") return { ...DEFAULT_MANAGEMENT_STATE };
  try {
    const raw = window.sessionStorage.getItem(MANAGEMENT_STATE_KEY);
    if (!raw) return { ...DEFAULT_MANAGEMENT_STATE };
    const parsed = JSON.parse(raw) as Partial<ManagementState>;
    const status = parsed.taskStatusFilter;
    return {
      ...DEFAULT_MANAGEMENT_STATE,
      ...parsed,
      view: parsed.view === "projects" || parsed.view === "tags" || parsed.view === "tasks" ? parsed.view : "tasks",
      taskStatusFilter: status === "all" || status === "done" || status === "todo" ? status : "todo",
      taskScrollTop: typeof parsed.taskScrollTop === "number" && Number.isFinite(parsed.taskScrollTop) ? Math.max(0, parsed.taskScrollTop) : 0,
    };
  } catch {
    return { ...DEFAULT_MANAGEMENT_STATE };
  }
}

export function saveManagementState(state: ManagementState): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(MANAGEMENT_STATE_KEY, JSON.stringify(state));
  } catch {
    // Storage can be disabled in a locked-down WebView; in-memory state still
    // keeps navigation smooth for the current mount.
  }
}
