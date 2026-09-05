import type { TimerMode } from "../../domain/models";

// ─── Types ────────────────────────────────────────────────────────────────────
export type NavSection = "timer" | "tasks" | "stats" | "settings";

/** A session rendered in the activity list (completed or abandoned). */
export interface SessionLog {
  id: string; time: string; duration: number; task: string;
  mode: TimerMode;
  status: "completed" | "abandoned";
  /** Tag name frozen at session time (v1.1 §11.6). Null for legacy rows. */
  tag: string | null;
  /** Raw session start (epoch ms) — the canonical sort key (v1.1.2 C1). */
  startedAt: number;
}
