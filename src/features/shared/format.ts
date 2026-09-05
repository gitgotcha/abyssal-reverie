import type { TimerMode, TimerSession } from "../../domain/models";
import type { SessionLog } from "./types";

// ─── Constants ────────────────────────────────────────────────────────────────
// Derived from the shared settings; updated when settings are persisted.
// Until bootstrap loads persisted settings, the defaults are used.
export const MODE_LABELS: Record<TimerMode, string> = { focus: "专注", short: "短休", long: "长休" };

export function pad(n: number) { return String(n).padStart(2, "0"); }
export function formatSeconds(s: number) { return { m: pad(Math.floor(s/60)), s: pad(s%60) }; }
export function uid() { return Math.random().toString(36).slice(2,9); }

/**
 * v1.1.2 D3: "已记录 30 秒" must never render as "已记录 1 分钟" — sub-minute
 * durations spell out their seconds; everything else rounds up to minutes.
 */
export function formatFocusedDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s} 秒`;
  return `${Math.round(s / 60)} 分钟`;
}

/** Projects a persisted session onto the activity-list row shape. */
export function sessionToLog(session: TimerSession): SessionLog {
  const at = new Date(session.startedAt);
  return {
    id: session.id,
    startedAt: session.startedAt,
    time: `${pad(at.getHours())}:${pad(at.getMinutes())}`,
    duration: Math.round(session.focusedSeconds / 60),
    task: session.taskTitleSnapshot,
    tag: session.tagNameSnapshot ?? null,
    mode: session.mode,
    status: session.status,
  };
}

/**
 * v1.1.2 C1: canonical in-memory order for the activity list — newest FIRST
 * (matching the backend's `started_at DESC, rowid DESC`), tie-broken by id.
 * Whatever order records arrive in (bootstrap, append, reload), the newest
 * record always sits at index 0.
 */
export function sortLogsDesc(logs: SessionLog[]): SessionLog[] {
  return [...logs].sort((a, b) =>
    (b.startedAt - a.startedAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** Head-insert (or replace) a log into the newest-first array, deduped by id. */
export function upsertLogNewestFirst(logs: SessionLog[], log: SessionLog): SessionLog[] {
  return sortLogsDesc([log, ...logs.filter(l => l.id !== log.id)]);
}

/** Only completed focus sessions count toward goals and stats (spec §6). */
export function isCountedFocus(log: SessionLog): boolean {
  return log.mode === "focus" && log.status === "completed";
}

export function chineseDate() {
  const d = new Date();
  const wd = ["周日","周一","周二","周三","周四","周五","周六"];
  return `${d.getMonth()+1}月${d.getDate()}日 · ${wd[d.getDay()]}`;
}
