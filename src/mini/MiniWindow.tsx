import { useEffect, useMemo, useState } from "react"
import { createRoot } from "react-dom/client"

import { DEFAULT_SETTINGS } from "../domain/defaults"
import type { AppGateway } from "../services/appGateway"
import type { CompleteTaskInput, TaskProgress, TimerSnapshot } from "../domain/models"
import { TauriAppGateway } from "../services/tauriAppGateway"
import { formatSeconds } from "../features/shared/format"

/**
 * v1.3 C: the floating mini window.
 *
 * All state comes from the shared backend: an initial `bootstrap`, then the
 * `timer-changed` snapshot broadcast after any command and `timer-settled`
 * after the background ticker settles a round. The window never runs its own
 * timer logic — closing it cannot interrupt a running round (roadmap §7).
 *
 * 「完成任务并保存」calls the atomic `complete_task_now` command; on failure
 * the clock stays paused and the task stays open, and the button may be
 * retried (v1.3 C3). A short 撤销 window reopens the task while keeping the
 * real recorded segments (v1.3 C4).
 */

function useMiniTimer(gateway: AppGateway) {
  const [timer, setTimer] = useState<TimerSnapshot | null>(null)
  const [celebrate, setCelebrate] = useState(false)
  const [taskTitle, setTaskTitle] = useState<string | null>(null)
  const [taskProgress, setTaskProgress] = useState<TaskProgress | null>(null)


  useEffect(() => {
    gateway.bootstrap().then(payload => {
      setTimer(payload.timer)
      setTaskTitle(payload.timer.taskTitleSnapshot)
    }).catch(() => undefined)
    const offChanged = gateway.subscribeTimerChanged(snapshot => {
      setTimer(snapshot)
      setTaskTitle(snapshot.taskTitleSnapshot)
      if (snapshot.selectedTaskId) {
        gateway.getTaskProgress(snapshot.selectedTaskId).then(setTaskProgress).catch(() => undefined)
      } else {
        setTaskProgress(null)
      }
    })
    const offSettled = gateway.subscribeTimerSettled(payload => {
      setTimer(payload.timer)
      setTaskTitle(payload.timer.taskTitleSnapshot)
      setTaskProgress(null)
      if (payload.newlyCompleted) {
        setCelebrate(true)
        window.setTimeout(() => setCelebrate(false), 2000)
      }
    })
    return () => { offChanged(); offSettled() }
  }, [gateway])

  // Live progress while active.
  useEffect(() => {
    if (timer?.state !== "running" && timer?.state !== "paused") return
    const pull = () => {
      const id = timer?.selectedTaskId
      if (!id) return
      gateway.getTaskProgress(id).then(setTaskProgress).catch(() => undefined)
    }
    pull()
    const id = setInterval(pull, 5000)
    return () => clearInterval(id)
  }, [timer?.state, timer?.selectedTaskId, gateway])

  return { timer, taskTitle, taskProgress, celebrate, setTimer, setTaskTitle, setTaskProgress }
}

/** v1.3 D: ocean companion — swim (focus), rest (paused/break), celebrate
 *  (done). A numeric-only mode is one click away. */
function Companion({ state, celebrate }: { state: string; celebrate: boolean }) {
  const anim = celebrate
    ? "mini-fish-celebrate 0.9s ease-in-out 2"
    : state === "running"
      ? "mini-fish-swim 6s ease-in-out infinite"
      : "mini-fish-rest 7s ease-in-out infinite";
  return (
    <img src="/media/fish/fish_blue.png" alt="" aria-hidden draggable={false}
      style={{
        width: 26, height: 20, objectFit: "contain", opacity: 0.9,
        transform: "scaleX(-1)",
        animation: anim,
      }} />
  );
}

function MiniWindow({ gateway }: { gateway: AppGateway }) {
  const { timer, taskTitle, taskProgress, celebrate, setTimer, setTaskTitle, setTaskProgress } = useMiniTimer(gateway)
  const [collapsed, setCollapsed] = useState(false)
  const [alwaysOnTop, setAlwaysOnTop] = useState(true)
  const [busy, setBusy] = useState(false)
  const [numericOnly, setNumericOnly] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [undoableSession, setUndoableSession] = useState<string | null>(null)

  // Persist device-local prefs (position comes from the OS drag).
  useEffect(() => {
    gateway.loadMiniPrefs().then(prefs => {
      if (typeof prefs.alwaysOnTop === "boolean") setAlwaysOnTop(prefs.alwaysOnTop)
      if (typeof prefs.collapsed === "boolean") setCollapsed(prefs.collapsed)
    }).catch(() => undefined)
  }, [gateway])

  useEffect(() => {
    const t = setTimeout(() => {
      gateway.saveMiniPrefs({ alwaysOnTop, collapsed }).catch(() => undefined)
    }, 300)
    return () => clearTimeout(t)
  }, [alwaysOnTop, collapsed, gateway])

  useEffect(() => {
    if (!message) return
    const id = setTimeout(() => setMessage(null), 2600)
    return () => clearTimeout(id)
  }, [message])

  // Undo window: 8 seconds after a successful complete.
  useEffect(() => {
    if (!undoableSession) return
    const id = setTimeout(() => setUndoableSession(null), 8000)
    return () => clearTimeout(id)
  }, [undoableSession])

  const state = timer?.state ?? "idle"
  const mode = timer?.mode ?? "focus"
  const total = timer?.durationSeconds ?? DEFAULT_SETTINGS.focusDurationMinutes * 60

  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (state !== "running") return
    const id = setInterval(() => setNow(Date.now()), 250)
    return () => clearInterval(id)
  }, [state])
  const remaining = state === "running" && timer?.targetEndAt
    ? Math.max(0, Math.ceil((timer.targetEndAt - now) / 1000))
    : timer?.remainingSeconds ?? total
  const { m, s } = formatSeconds(remaining)

  const statusLabel = state === "running" ? "专注中" : state === "paused" ? "已暂停" : state === "done" ? "已完成" : mode === "focus" ? "专注" : "休息"

  const toggle = async () => {
    if (!timer) return
    try {
      const next = state === "running"
        ? await gateway.pauseTimer({ expectedRevision: timer.revision })
        : state === "paused"
          ? await gateway.resumeTimer({ expectedRevision: timer.revision })
          : await gateway.startTimer({ mode, selectedTaskId: timer.selectedTaskId, expectedRevision: timer.revision })
      setTimer(next)
    } catch {
      setMessage("操作失败，已恢复当前状态")
      gateway.bootstrap().then(p => setTimer(p.timer)).catch(() => undefined)
    }
  }

  const completeAndSave = async () => {
    if (busy || !timer?.activeSessionId || !timer.selectedTaskId) return
    setBusy(true)
    const input: CompleteTaskInput = {
      taskId: timer.selectedTaskId,
      expectedRevision: timer.revision,
      activeSessionId: timer.activeSessionId,
    }
    try {
      const result = await gateway.completeTaskNow(input)
      setTimer(result.timer)
      setTaskTitle(result.timer.taskTitleSnapshot)
      setTaskProgress(null)
      if (result.newlyCompleted) {
        const minutes = Math.round(result.segmentSavedMs / 60000)
        setMessage(`任务已完成，已保存 ${minutes} 分钟`)
        setUndoableSession(result.task.id)
      } else {
        setMessage("任务已完成")
      }
    } catch (err) {
      // v1.3 C3: the clock stays paused, the task stays open — retryable.
      setMessage(`保存失败：${err instanceof Error ? err.message : String(err)}`)
      gateway.bootstrap().then(p => setTimer(p.timer)).catch(() => undefined)
    } finally {
      setBusy(false)
    }
  }

  const undoComplete = async () => {
    if (!undoableSession) return
    try {
      await gateway.undoCompleteTask(undoableSession)
      setMessage("已撤销完成，时间记录保留")
      setUndoableSession(null)
    } catch (err) {
      setMessage(`撤销失败：${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const canComplete = Boolean(timer?.activeSessionId && timer.selectedTaskId && mode === "focus" && (state === "running" || state === "paused"))
  const progressPct = useMemo(() => {
    if (!taskProgress || taskProgress.targetSeconds <= 0) return null
    return Math.round(taskProgress.progress * 100)
  }, [taskProgress])

  if (collapsed) {
    return (
      <div style={{ padding: 4 }} data-testid="mini-root">
        <div style={{
          display: "flex", alignItems: "center", gap: 8, padding: "8px 10px",
          borderRadius: 12, background: "rgba(8,13,18,0.82)", border: "1px solid rgba(215,228,230,0.14)",
          backdropFilter: "blur(20px)", WebkitBackdropFilter: "blur(20px)", color: "#E7EFF0",
        }}>
          <span style={{ fontFamily: "var(--font-mono)", fontSize: 15, fontVariantNumeric: "tabular-nums", flex: 1, textAlign: "center" }}>
            {m}:{s}
          </span>
          <MiniBtn label={state === "running" ? "暂停" : "继续"} onClick={toggle} />
          <MiniBtn label="展开" onClick={() => setCollapsed(false)} />
        </div>
        {message && <MiniToast text={message} />}
      </div>
    )
  }

  return (
    <div style={{ padding: 6 }} data-testid="mini-root">
      <div style={{
        height: "calc(100vh - 12px)", display: "flex", flexDirection: "column", gap: 8,
        padding: "10px 12px", borderRadius: 14,
        background: "rgba(8,13,18,0.82)", border: "1px solid rgba(215,228,230,0.14)",
        boxShadow: "0 10px 30px rgba(2,3,5,0.42)",
        backdropFilter: "blur(20px) saturate(1.05)", WebkitBackdropFilter: "blur(20px) saturate(1.05)",
        color: "#E7EFF0", overflow: "hidden",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{
            fontSize: 10, letterSpacing: "0.08em", color: "rgba(215,228,230,0.55)",
            fontFamily: "var(--font-sans)", flex: 1,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>
            {taskTitle ?? "未指定任务"}
          </span>
          <MiniBtn label="主窗" title="返回主窗口" onClick={() => gateway.showMainWindow()} />
          <MiniBtn label={alwaysOnTop ? "置顶✓" : "置顶"} title="切换置顶"
            onClick={() => setAlwaysOnTop(v => !v)} />
          <MiniBtn label={numericOnly ? "🐟" : "数"} title={numericOnly ? "显示伙伴" : "纯数字模式"} onClick={() => setNumericOnly(v => !v)} />
          <MiniBtn label="—" title="折叠成计时条" onClick={() => setCollapsed(true)} />
          <MiniBtn label="×" title="关闭小窗（计时继续）" onClick={() => gateway.toggleMiniWindow()} />
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "center", flex: 1, gap: 12 }}>
          <span style={{
            fontFamily: "var(--font-mono)", fontSize: 38, fontWeight: 300,
            fontVariantNumeric: "tabular-nums", lineHeight: 1,
          }}>
            {m}<span className={state === "running" ? "colon-blink" : ""}>:</span>{s}
          </span>
          <span style={{ fontSize: 10, color: "rgba(215,228,230,0.55)", fontFamily: "var(--font-sans)" }}>
            {statusLabel}
          </span>
          {!numericOnly && <Companion state={celebrate ? "done" : state} celebrate={celebrate} />}
        </div>

        {progressPct !== null && (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <div style={{ flex: 1, height: 3, borderRadius: 2, background: "rgba(215,228,230,0.12)", overflow: "hidden" }}>
              <div style={{ height: "100%", width: `${Math.min(100, progressPct)}%`, background: "rgba(158,173,178,0.7)" }} />
            </div>
            <span style={{ fontSize: 9, color: "rgba(215,228,230,0.55)", fontFamily: "var(--font-sans)" }}>
              {progressPct}%{taskProgress && taskProgress.overSeconds > 0 ? " · 已达预计" : ""}
            </span>
          </div>
        )}

        <div style={{ display: "flex", gap: 6 }}>
          <MiniBtn label={state === "running" ? "暂停" : state === "paused" ? "继续" : "开始"} onClick={toggle} wide />
          <MiniBtn
            label={busy ? "保存中…" : "完成任务并保存"}
            title={canComplete ? "保存当前片段并完成任务，主钟保持暂停" : "需要正在进行的专注轮次和当前任务"}
            onClick={completeAndSave}
            disabled={!canComplete || busy}
            wide
          />
        </div>

        {undoableSession && (
          <button onClick={() => void undoComplete()}
            style={{
              border: "1px solid rgba(255,120,120,0.45)", borderRadius: 8,
              background: "rgba(60,22,22,0.55)", color: "#ffd9d9", fontSize: 10,
              fontFamily: "var(--font-sans)", padding: "4px 8px", cursor: "pointer",
            }}>
            撤销完成（真实时间保留 · 8 秒内）
          </button>
        )}

        {message && <MiniToast text={message} />}
      </div>
    </div>
  )
}

function MiniBtn({ label, title, onClick, disabled, wide }: {
  label: string; title?: string; onClick: () => void; disabled?: boolean; wide?: boolean
}) {
  return (
    <button onClick={onClick} title={title} aria-label={title ?? label} disabled={disabled}
      style={{
        flex: wide ? 1 : undefined,
        border: "1px solid rgba(215,228,230,0.14)", borderRadius: 8,
        background: "rgba(27,37,44,0.42)", color: "#E7EFF0",
        fontSize: 10, fontFamily: "var(--font-sans)",
        padding: "5px 8px", cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.4 : 1, minHeight: 26,
      }}>
      {label}
    </button>
  )
}

function MiniToast({ text }: { text: string }) {
  return (
    <div role="status" style={{
      fontSize: 10, color: "#E7EFF0", background: "rgba(27,37,44,0.65)",
      border: "1px solid rgba(215,228,230,0.14)", borderRadius: 8,
      padding: "4px 8px", fontFamily: "var(--font-sans)",
    }}>
      {text}
    </div>
  )
}

export function MiniApp() {
  const gateway = useMemo(() => new TauriAppGateway(), [])
  return <MiniWindow gateway={gateway} />
}
