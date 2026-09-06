import type { TimerMode } from "../../domain/models";

// ─── Types ────────────────────────────────────────────────────────────────────
export type NavSection = "timer" | "tasks" | "stats" | "settings";

/** A session rendered in the activity list (completed or abandoned).
 *  R03: the authoritative fields stay on the row; HH:mm / minutes are
 *  display-only projections. */
export interface SessionLog {
  id: string; time: string; duration: number; task: string;
  mode: TimerMode;
  status: "completed" | "abandoned";
  /** Tag name frozen at session time (v1.1 §11.6). Null for legacy rows. */
  tag: string | null;
  /** Raw session start (epoch ms) — the canonical sort key. */
  startedAt: number;
  /** R03: raw end + effective seconds retained for re-sorting/audits. */
  endedAt: number;
  focusedSeconds: number;
  /** R02: visibility gate comes from the backend's authoritative field. */
  statisticsEligible: boolean;
}
