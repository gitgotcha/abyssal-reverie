import { useEffect, useRef, useState } from "react";
import type { Task, TaskProgress, TimerMode, TimerSnapshot } from "../../domain/models";
import { DEFAULT_SETTINGS } from "../../domain/defaults";
import { HorizonDivider } from "./GoalRing";
import { C, CARD } from "../shared/palette";
import { MODE_LABELS, formatSeconds } from "../shared/format";
import { playCompletionSound, notifyCompletion } from "../shared/notify";
import { GoalRing } from "./GoalRing";
import { TimerArc } from "./TimerArc";

export function TimerPanel({ timer, tasks, taskProgress, selectedTaskId, onSelectTask, onStart, onPause, onResume, onReset, onResetRequest, onFinish, onSwitchMode, onExpire }: {
  timer: TimerSnapshot | null;
  tasks: Task[];
  /** v1.2 G1: per-task progress (confirmed + provisional), keyed by task id. */
  taskProgress: Record<string, TaskProgress>;
  /** v1.1.2 B2: the current task is the backend's snapshot (or the pending
   *  idle selection owned by App) — never local highlight state. */
  selectedTaskId: string | null;
  onSelectTask: (taskId: string) => void;
  onStart: (mode: TimerMode, taskId: string | null) => void;
  onPause: () => void;
  onResume: () => void;
  /** Direct reset — only legal from idle/done (active resets confirm first). */
  onReset: () => void;
  /** Active reset: App opens the confirm dialog, then runs the reset. */
  onResetRequest: () => void;
  /** Manual "结束" — routes to gateway.finishTimer (v1.1). */
  onFinish: () => void;
  onSwitchMode: (mode: TimerMode) => void;
  onExpire: () => void;
}) {
  // Rust owns the timer; this component only renders its snapshot.
  const state = timer?.state ?? "idle";
  const mode  = timer?.mode ?? "focus";
  const total = timer?.durationSeconds ?? DEFAULT_SETTINGS.focusDurationMinutes * 60;

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Drift-free display tick: refresh `now` while running; remaining derives
  // from the authoritative targetEndAt, never from an incremented counter.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (state !== "running") {
      if (intervalRef.current) { clearInterval(intervalRef.current); intervalRef.current = null; }
      return;
    }
    intervalRef.current = setInterval(() => setNow(Date.now()), 250);
    return () => { if (intervalRef.current) { clearInterval(intervalRef.current); intervalRef.current = null; } };
  }, [state]);

  const remaining = state === "running" && timer?.targetEndAt
    ? Math.max(0, Math.ceil((timer.targetEndAt - now) / 1000))
    : timer?.remainingSeconds ?? total;
  const progress = total > 0 ? remaining / total : 0;

  // Fire onExpire exactly once per session when the countdown reaches zero.
  const expiredRef = useRef<string | null>(null);
  useEffect(() => {
    if (state === "running" && timer?.activeSessionId && timer.targetEndAt && timer.targetEndAt <= now) {
      if (expiredRef.current !== timer.activeSessionId) {
        expiredRef.current = timer.activeSessionId;
        onExpire();
      }
    }
    if (state === "idle") expiredRef.current = null;
  }, [state, timer, now, onExpire]);

  const handleStart = () => {
    if (state === "paused") onResume();
    else if (state === "idle" || state === "done") onStart(mode, selectedTaskId);
  };
  const handlePause = () => { if (state === "running") onPause(); };
  const active = state === "running" || state === "paused";
  const handleReset = () => { if (active) onResetRequest(); else onReset(); };
  const switchMode  = (m: TimerMode) => onSwitchMode(m);

  const { m, s } = formatSeconds(remaining);
  const activeTasks = tasks.filter(t => !t.done);

  const statusText = () => {
    if (state === "done")    return "已完成";
    if (state === "idle")    return MODE_LABELS[mode];
    if (state === "running") return "专注中";
    return "已暂停";
  };

  const ctrlBtn: React.CSSProperties = {
    width: 42, height: 42, borderRadius: "50%",
    background: C.glassClear,
    backdropFilter: "blur(14px)", WebkitBackdropFilter: "blur(14px)",
    border: `1px solid ${C.hairline}`,
    color: C.textMuted, cursor: "pointer",
    display: "flex", alignItems: "center", justifyContent: "center", /* focus rings come from index.css (:focus / :focus-visible) */
  };

  return (
    <div className="flex flex-col h-full overflow-y-auto" style={{ position: "relative", zIndex: 2 }}>

      {/* Mode bar */}
      <div style={{ flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "10px 22px" }}>
          {(["focus","short","long"] as TimerMode[]).map(md => (
            <button key={md} onClick={() => switchMode(md)} className="btn-mode"
              disabled={active}
              title={active ? "请先结束或重置当前计时" : `切换到${MODE_LABELS[md]}`}
              style={{
                fontFamily: "var(--font-sans)", fontSize: 12,
                fontWeight: mode === md ? 500 : 400,
                padding: "4px 13px", borderRadius: 20,
                border: `0.5px solid ${mode === md ? C.hairlineStr : "transparent"}`,
                background: mode === md ? "rgba(27,37,44,0.38)" : "transparent",
                color: mode === md ? C.moonlight : C.textMuted,
                opacity: active ? 0.45 : 1,
                cursor: active ? "default" : "pointer", /* focus rings come from index.css (:focus / :focus-visible) */
              }}>
              {MODE_LABELS[md]}
            </button>
          ))}
          <div style={{ marginLeft: "auto", fontFamily: "var(--font-mono)", fontSize: 10, color: C.textMuted, letterSpacing: "0.05em" }}>
            {timer?.taskTitleSnapshot ?? MODE_LABELS[mode]}
          </div>
        </div>
        <HorizonDivider />
      </div>

      {/* Arc + controls + task selector — centred in the main column */}
      <div style={{
        display: "flex", flexDirection: "column", alignItems: "center",
        justifyContent: "center", flex: 1,
        gap: 12, padding: "8px 22px 11px",
        minHeight: 0,
      }}>

        {/* Arc */}
        <div className="su-1 surface-up" style={{ position: "relative", width: 290, height: 290, flexShrink: 0 }}>
          <div style={{
            position: "absolute", width: 310, height: 310, top: -10, left: -10,
            borderRadius: "50%",
            background: "radial-gradient(circle, rgba(14,22,30,0.14) 0%, transparent 65%)",
            filter: "blur(32px)", pointerEvents: "none",
          }} />
          <TimerArc progress={progress} mode={mode}
            isRunning={state === "running"} isDone={state === "done"} />
          <div style={{
            position: "absolute", inset: 0,
            display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
            gap: 5,
          }}>
            <div style={{
              position: "absolute", width: 188, height: 188, borderRadius: "50%",
              background: "radial-gradient(ellipse 55% 42% at 40% 34%, rgba(215,228,230,0.015) 0%, rgba(14,22,30,0.022) 55%, transparent 80%)",
              backdropFilter: "blur(2px)", WebkitBackdropFilter: "blur(2px)",
              border: "0.5px solid rgba(215,228,230,0.036)",
              pointerEvents: "none",
            }} />
            <div style={{
              fontFamily: "var(--font-display)", fontVariantNumeric: "tabular-nums",
              fontSize: 60, fontWeight: 300, letterSpacing: "-0.026em", lineHeight: 1,
              color: state === "done" ? C.moonlight : C.textPrimary,
              transition: "color 0.5s",
              textShadow: state === "running" ? "0 0 28px rgba(158,173,178,0.10)" : "none",
              position: "relative", zIndex: 1,
            }}>
              {m}<span className={state === "running" ? "colon-blink" : ""}>:</span>{s}
            </div>
            <div style={{
              fontFamily: "var(--font-sans)", fontSize: 10, letterSpacing: "0.12em",
              color: state === "done" ? "rgba(186,200,204,0.58)" : C.textMuted,
              transition: "color 0.4s", position: "relative", zIndex: 1,
            }}>
              {statusText()}
            </div>
          </div>
        </div>

        {/* Controls */}
        <div className="su-2 surface-up" style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <button onClick={handleReset}
            title="重置计时"
            aria-label="重置计时"
            className="btn-ctrl" style={ctrlBtn}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path d="M2 7a5 5 0 1 0 1-3H1" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
              <path d="M1 4V7H4" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>

          {state === "running" ? (
            <button onClick={handlePause} aria-label="暂停计时" className="btn-main"
              style={{
                width: 68, height: 68, borderRadius: "50%",
                background: C.glassTint,
                backdropFilter: "blur(16px)", WebkitBackdropFilter: "blur(16px)",
                border: `1px solid rgba(215,228,230,0.12)`,
                boxShadow: "inset 0 1px 0 rgba(215,228,230,0.07), 0 0 18px rgba(158,173,178,0.06), 0 4px 16px rgba(2,3,5,0.28)",
                color: C.moonlight, cursor: "pointer",
                display: "flex", alignItems: "center", justifyContent: "center", /* focus rings come from index.css (:focus / :focus-visible) */
              }}>
              <svg width="16" height="16" viewBox="0 0 18 18" fill="none">
                <rect x="4" y="3" width="3.5" height="12" rx="1.2" fill="currentColor" />
                <rect x="10.5" y="3" width="3.5" height="12" rx="1.2" fill="currentColor" />
              </svg>
            </button>
          ) : (
            <button onClick={handleStart} aria-label={state === "done" ? "重新开始专注" : "开始专注"} className="btn-main"
              style={{
                width: 68, height: 68, borderRadius: "50%",
                background: state === "done" ? "rgba(27,37,44,0.38)" : "rgba(158,173,178,0.06)",
                backdropFilter: "blur(16px)", WebkitBackdropFilter: "blur(16px)",
                border: `1px solid rgba(215,228,230,0.12)`,
                boxShadow: "inset 0 1px 0 rgba(215,228,230,0.07), 0 0 22px rgba(158,173,178,0.08), 0 4px 16px rgba(2,3,5,0.28)",
                color: C.moonlight, cursor: "pointer",
                display: "flex", alignItems: "center", justifyContent: "center", /* focus rings come from index.css (:focus / :focus-visible) */
              }}>
              {state === "done" ? (
                <svg width="16" height="16" viewBox="0 0 18 18" fill="none">
                  <path d="M3 9a6 6 0 1 0 1.5-4H3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
                  <path d="M3 5V9H7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              ) : (
                <svg width="16" height="16" viewBox="0 0 18 18" fill="none">
                  <path d="M6.5 4L14.5 9L6.5 14V4Z" fill="currentColor" />
                </svg>
              )}
            </button>
          )}

          <button onClick={onFinish} disabled={!active}
            title={active ? "结束本次" : "结束（计时进行中可用）"}
            aria-label="结束本次"
            className="btn-ctrl"
            style={{
              ...ctrlBtn,
              opacity: active ? 1 : 0.35,
              cursor: active ? "pointer" : "default",
            }}>
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
              <rect x="2.5" y="2.5" width="9" height="9" rx="1.6" fill="currentColor" />
            </svg>
          </button>
        </div>

        {/* Task selector */}
        <div className="su-3 surface-up" style={{ width: "min(76%, 520px)", minWidth: 280 }}>
          <HorizonDivider />
          <div style={{ paddingTop: 9 }}>
            <div style={{
              fontFamily: "var(--font-sans)", fontSize: 10,
              letterSpacing: "0.10em", color: C.textMuted, marginBottom: 7,
              textTransform: "uppercase",
            }}>当前任务</div>
            {activeTasks.length === 0 ? (
              <div style={{ padding: "9px 12px", ...CARD, color: C.textMuted, fontSize: 11, fontFamily: "var(--font-sans)", fontStyle: "italic" }}>
                暂无任务，前往任务面板添加
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 4, maxHeight: 148, overflowY: "auto" }}>
                {activeTasks.map(task => {
                  // v1.1.2 B2: highlight follows the backend snapshot / the
                  // pending idle selection passed by App — a click while the
                  // timer runs asks App to switch, it never fakes a highlight.
                  const sel = selectedTaskId === task.id;
                  return (
                    <button key={task.id} onClick={() => onSelectTask(task.id)}
                      className="task-sel-item"
                      style={{
                        position: "relative", overflow: "hidden",
                        padding: "8px 12px",
                        ...CARD,
                        background: sel ? "rgba(27,37,44,0.40)" : C.cardDim,
                        border: `1px solid ${sel ? C.hairlineStr : C.hairline}`,
                        color: sel ? C.moonlight : C.textSec,
                        fontSize: 12, fontFamily: "var(--font-sans)",
                        textAlign: "left", cursor: "pointer", /* focus rings come from index.css (:focus / :focus-visible) */
                        display: "flex", alignItems: "center", gap: 7,
                      }}>
                      {sel && (
                        <div style={{
                          position: "absolute", left: 0, top: 0, bottom: 0, width: 2,
                          background: `linear-gradient(to bottom, transparent 0%, ${C.silver} 30%, ${C.moonlight} 55%, ${C.silver} 78%, transparent 100%)`,
                        }} />
                      )}
                      {sel && (
                        <svg width="5" height="5" viewBox="0 0 6 6" style={{ flexShrink: 0 }}>
                          <circle cx="3" cy="3" r="2.4" fill={C.silver} className="breathe" />
                        </svg>
                      )}
                      <span style={{
                        flex: 1, lineHeight: 1.4, display: "flex", flexDirection: "column", gap: 3, minWidth: 0,
                      }}>
                        <span style={{
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                        }}>{task.title}</span>
                        {(() => {
                          // v1.2 G1: real progress from the segment ledger —
                          // 达预计 shows the explicit badge, extra time is listed.
                          const prog = taskProgress[task.id];
                          if (!prog || prog.targetSeconds <= 0) return null;
                          const pct = Math.round(prog.progress * 100);
                          const over = prog.overSeconds > 0;
                          return (
                            <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                              <span style={{
                                width: 64, height: 3, borderRadius: 2, overflow: "hidden",
                                background: "rgba(215,228,230,0.10)", flexShrink: 0,
                              }}>
                                <span style={{
                                  display: "block", height: "100%", width: `${Math.min(100, pct)}%`,
                                  background: pct >= 100 ? "rgba(186,200,204,0.85)" : "rgba(158,173,178,0.55)",
                                }} />
                              </span>
                              <span style={{
                                fontFamily: "var(--font-sans)", fontSize: 9,
                                color: over ? "rgba(186,200,204,0.85)" : C.textMuted,
                              }}>
                                {pct}%{over ? ` · 超出预计 ${Math.round(prog.overSeconds / 60)} 分钟` : pct >= 100 ? " · 已达预计" : ""}
                              </span>
                            </span>
                          );
                        })()}
                      </span>
                      <span style={{ fontFamily: "var(--font-mono)", fontSize: 9, color: C.textMuted, flexShrink: 0 }}>×{task.pomodoroTarget}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Tasks Panel ──────────────────────────────────────────────────────────────
