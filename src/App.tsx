import { useState, useEffect, useRef, useCallback } from "react";
import type { AppSettings, CreateTaskInput, ImportPreview, Statistics, Tag, Task, TaskPriority, TimerMode, TimerSession, TimerSnapshot } from "./domain/models";
import { DEFAULT_SETTINGS, durationSecondsForMode } from "./domain/defaults";
import { todayBoundary, weekBoundaries, weekRange } from "./domain/statistics";
import { formatTrayIndicator } from "./domain/tray";
import { useAppGateway } from "./services/gatewayContext";

import { C, CARD, SIDEBAR_GLASS } from "./features/shared/palette";
import type { NavSection, SessionLog } from "./features/shared/types";
import { MODE_LABELS, sessionToLog, sortLogsDesc, upsertLogNewestFirst, formatFocusedDuration, isCountedFocus } from "./features/shared/format";
import { playCompletionSound, notifyCompletion } from "./features/shared/notify";
import { GoalRing } from "./features/timer/GoalRing";
import { TimerPanel } from "./features/timer/TimerPanel";
import { TasksPanel } from "./features/tasks/TasksPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { StatsPanel, StatsPage } from "./features/stats/StatsPanel";
import { MiniBar } from "./features/shared/MiniBar";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { ToastHost } from "./components/ToastHost";

const NAV_ITEMS: { id: NavSection; label: string; icon: React.JSX.Element }[] = [
  { id: "timer",    label: "专注",   icon: <svg width="15" height="15" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.6"/><path d="M12 7v5l3 3" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/></svg> },
  { id: "tasks",    label: "任务",   icon: <svg width="15" height="15" viewBox="0 0 24 24" fill="none"><path d="M4 6h16M4 12h16M4 18h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/></svg> },
  { id: "stats",    label: "统计",   icon: <svg width="15" height="15" viewBox="0 0 24 24" fill="none"><path d="M5 20V10M12 20V4M19 20v-7" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/></svg> },
  { id: "settings", label: "设置",   icon: <svg width="15" height="15" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.6"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/></svg> },
];

// ─── Ocean Video Background ───────────────────────────────────────────────────
function OceanVideo({ reduceMotion }: { reduceMotion: boolean }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  // R1-04: if the video (or its source) fails to load, fall back to a poster
  // <div> so the ocean scene never degrades to a black screen.
  const [videoFailed, setVideoFailed] = useState(false);

  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;

    // 0.87× — natural deceleration without artificial slow-motion artifacts
    const applyRate = () => { v.playbackRate = 0.87; };
    applyRate();
    v.addEventListener("loadedmetadata", applyRate);

    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");

    // `play()` is unimplemented in some environments (jsdom returns undefined,
    // older WebViews may throw) — never let a background-play attempt crash.
    const play = () => {
      try { void v.play()?.catch(() => {}); } catch { /* playback unavailable */ }
    };

    // Play only when motion is wanted (in-app toggle off, OS reduce-motion off)
    // and the window is actually visible. Pausing on `document.hidden` also
    // covers minimize/occlusion, cutting CPU/GPU cost in the tray.
    const sync = () => {
      if (!reduceMotion && !mq.matches && !document.hidden) play();
      else v.pause();
    };

    const onVisibility = () => sync();
    document.addEventListener("visibilitychange", onVisibility);

    const onMq = () => sync();
    mq.addEventListener("change", onMq);

    sync();

    return () => {
      v.removeEventListener("loadedmetadata", applyRate);
      document.removeEventListener("visibilitychange", onVisibility);
      mq.removeEventListener("change", onMq);
    };
  }, [reduceMotion]);

  if (videoFailed) {
    return (
      <>
        <div
          aria-hidden
          style={{
            position: "fixed", inset: 0, zIndex: 0, display: "block",
            backgroundImage: "url(/media/ocean-poster.jpg)",
            backgroundSize: "cover", backgroundPosition: "center",
            filter: "brightness(0.52) saturate(0.44) contrast(0.94)",
          }}
        />
        <Vignettes />
      </>
    );
  }

  return (
    <>
      {/* Single global ocean — fixed to viewport, every UI surface floats above it */}
      <video
        ref={videoRef}
        autoPlay muted loop playsInline
        poster="/media/ocean-poster.jpg"
        onError={() => setVideoFailed(true)}
        style={{
          position: "fixed", inset: 0,
          width: "100vw", height: "100vh",
          objectFit: "cover",
          zIndex: 0, display: "block",
          pointerEvents: "none",
          // D-Log-pressed grade: dark, desaturated, natural contrast
          filter: "brightness(0.52) saturate(0.44) contrast(0.94)",
        }}
      >
        <source src="/media/ocean-loop.mp4" type="video/mp4" />
      </video>

      <Vignettes />
    </>
  );
}

// Single continuous vignette + radial corners — shared by the video and its
// poster fallback so the look never changes across failure modes.
function Vignettes() {
  return (
    <>
      <div style={{
        position: "fixed", inset: 0, zIndex: 1, pointerEvents: "none",
        background: `linear-gradient(
          180deg,
          rgba(5,7,9,0.60) 0%,
          rgba(5,7,9,0.28) 22%,
          rgba(5,7,9,0.16) 46%,
          rgba(5,7,9,0.22) 70%,
          rgba(5,7,9,0.44) 87%,
          rgba(5,7,9,0.58) 100%
        )`,
      }} />

      <div style={{
        position: "fixed", inset: 0, zIndex: 1, pointerEvents: "none",
        background: "radial-gradient(ellipse 88% 86% at 50% 48%, transparent 28%, rgba(5,7,9,0.48) 100%)",
      }} />
    </>
  );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function Sidebar({ active, onNav }: { active: NavSection; onNav: (s: NavSection) => void }) {
  return (
    <aside className="nav-sidebar" style={{
      width: 56, flexShrink: 0, zIndex: 10, position: "relative",
      display: "flex", flexDirection: "column", alignItems: "center",
      padding: "14px 0",
      ...SIDEBAR_GLASS,
      borderRight: `1px solid ${C.hairline}`,
      boxShadow: "inset -1px 0 0 rgba(255,255,255,0.025), 6px 0 24px rgba(0,0,0,0.12)",
    }}>
      {/* Logo mark */}
      <div style={{
        width: 28, height: 28, borderRadius: "50%",
        background: "rgba(14, 20, 26, 0.35)",
        border: `0.5px solid rgba(215,228,230,0.14)`,
        display: "flex", alignItems: "center", justifyContent: "center",
        marginBottom: 16,
      }}>
        <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
          <circle cx="7" cy="7" r="2.6" stroke="rgba(226,239,239,0.85)" strokeWidth="1.2" />
          <circle cx="7" cy="7" r="5.5" stroke="rgba(226,239,239,0.22)" strokeWidth="0.8" />
        </svg>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {NAV_ITEMS.map((item) => {
          const isActive = active === item.id;
          return (
            <button key={item.id} onClick={() => onNav(item.id)} title={item.label}
              aria-label={item.label}
              className="nav-btn"
              style={{
                width: 36, height: 36, borderRadius: 10,
                background: isActive ? "rgba(255,255,255,0.12)" : "transparent",
                color: isActive ? "#FFFFFF" : "rgba(220, 232, 236, 0.70)",
                border: `0.5px solid ${isActive ? "rgba(255,255,255,0.22)" : "transparent"}`,
                cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
                /* focus rings come from index.css (:focus / :focus-visible) */
                textShadow: isActive ? "0 1px 4px rgba(0,0,0,0.5)" : "none",
              }}>
              {item.icon}
            </button>
          );
        })}
      </div>

      <div style={{ marginTop: "auto" }}>
        <div style={{
          width: 26, height: 26, borderRadius: "50%",
          background: "rgba(255,255,255,0.08)",
          border: `0.5px solid ${C.hairlineStr}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 9, fontWeight: 600, color: "#FFFFFF", fontFamily: "var(--font-sans)",
          textShadow: "0 1px 3px rgba(0,0,0,0.6)",
        }}>AK</div>
      </div>
    </aside>
  );
}

// ─── Timer Arc ────────────────────────────────────────────────────────────────

export default function App() {
  const gateway = useAppGateway();
  const [nav, setNav]     = useState<NavSection>("timer");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [logs, setLogs]   = useState<SessionLog[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [weekStats, setWeekStats] = useState<Statistics | null>(null);
  const [todayStats, setTodayStats] = useState<Statistics | null>(null);
  const [timer, setTimer] = useState<TimerSnapshot | null>(null);
  const [shortcutConflict, setShortcutConflict] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [resetConfirm, setResetConfirm] = useState(false);
  // v1.1.2 B2/B3: pending idle selection + the running-switch confirm dialog.
  const [pendingTaskId, setPendingTaskId] = useState<string | null>(null);
  const [switchConfirm, setSwitchConfirm] = useState<{ taskId: string; title: string } | null>(null);

  // Ref mirrors so async callbacks always see the latest snapshot without
  // becoming stale closures.
  const timerRef = useRef<TimerSnapshot | null>(null);
  const settingsRef = useRef<AppSettings | null>(null);
  useEffect(() => { timerRef.current = timer; }, [timer]);
  useEffect(() => { settingsRef.current = settings; }, [settings]);

  const activeSettings = settings ?? DEFAULT_SETTINGS;
  const durations = useCallback((mode: TimerMode) => durationSecondsForMode(mode, activeSettings), [activeSettings]);

  const applyTimer = useCallback((snapshot: TimerSnapshot) => {
    timerRef.current = snapshot;
    setTimer(snapshot);
  }, []);

  // Full resync after an unexpected gateway failure (e.g. CONFLICT).
  const resync = useCallback(() => {
    gateway.bootstrap()
      .then(payload => {
        setTasks(payload.tasks);
        setTags(payload.tags);
        setLogs(sortLogsDesc(payload.sessions.map(sessionToLog)));
        setTags(payload.tags);
        setSettings(payload.settings);
        applyTimer(payload.timer);
        setTodayStats(payload.statistics);
      })
      .catch(() => undefined);
  }, [gateway, applyTimer]);

  const refreshStats = useCallback(() => {
    const { from, to } = weekRange();
    gateway.getStatistics({ from, to, days: weekBoundaries() })
      .then(stats => { setWeekStats(stats); })
      .catch(() => undefined);
    const today = todayBoundary();
    gateway.getStatistics({ from: today.from, to: today.to, days: [today] })
      .then(stats => { setTodayStats(stats); })
      .catch(() => undefined);
  }, [gateway]);

  const runStart = useCallback(async (snapshot: TimerSnapshot, mode: TimerMode, taskId: string | null) => {
    const next = await gateway.startTimer({ mode, selectedTaskId: taskId, expectedRevision: snapshot.revision });
    applyTimer(next);
  }, [gateway, applyTimer]);

  const runComplete = useCallback(async (snapshot: TimerSnapshot, recovery: boolean) => {
    if (!snapshot.activeSessionId) return;
    const result = await gateway.completeTimer({
      activeSessionId: snapshot.activeSessionId,
      expectedRevision: snapshot.revision,
      recovery,
    });
    applyTimer(result.timer);
    // v1.1.2 C1: append only a genuinely new, statistics-eligible record
    // (breaks and sub-30s sessions never enter the activity view), head-
    // inserted (the array is newest-first) and deduped by id.
    if (result.newlyCompleted && result.session.statisticsEligible) {
      setLogs(p => upsertLogNewestFirst(p, sessionToLog(result.session)));
    }

    if (result.newlyCompleted && !recovery) {
      const s = settingsRef.current;
      if (s?.soundEnabled) playCompletionSound();
      if (s?.notificationEnabled) notifyCompletion(result.session.taskTitleSnapshot);
      // Auto-break per spec 4.2: only on a genuinely new focus completion.
      if (s?.autoStartBreak && result.session.mode === "focus") {
        void runStart(result.timer, "short", null).catch(() => undefined);
      }
    }
  }, [gateway, applyTimer, runStart]);

  const handleExpire = useCallback(async () => {
    const cur = timerRef.current;
    if (!cur) return;
    // v1.1.2 C2: stats refresh AFTER the completion settles — a concurrent
    // read could observe the pre-completion totals.
    await runComplete(cur, false).catch(resync);
    refreshStats();
  }, [runComplete, resync, refreshStats]);

  const handleStart = useCallback((mode: TimerMode, taskId: string | null) => {
    const cur = timerRef.current;
    if (!cur) return;
    runStart(cur, mode, taskId).catch(resync);
  }, [runStart, resync]);

  // v1.1.2 B2/B3: idle clicks just park the selection; a click while a focus
  // round is running/paused asks for confirmation, then atomically closes the
  // current session and opens a new round for the chosen task (A's history
  // stays with A).
  const handleSelectTask = useCallback((taskId: string) => {
    const cur = timerRef.current;
    if (!cur) return;
    if (cur.state === "running" || cur.state === "paused") {
      if (cur.mode !== "focus" || !cur.activeSessionId) {
        setToast("休息轮次不能切换任务");
        return;
      }
      if (cur.selectedTaskId === taskId) return;
      const target = tasks.find(t => t.id === taskId);
      if (!target) return;
      setSwitchConfirm({ taskId, title: target.title });
      return;
    }
    setPendingTaskId(taskId);
  }, [tasks]);

  const confirmSwitchTask = useCallback(() => {
    const cur = timerRef.current;
    const pending = switchConfirm;
    setSwitchConfirm(null);
    if (!cur || !pending || !cur.activeSessionId) return;
    gateway.switchTimerTask({
      expectedRevision: cur.revision,
      activeSessionId: cur.activeSessionId,
      newTaskId: pending.taskId,
    })
      .then(result => {
        applyTimer(result.timer);
        if (result.newlyClosed && result.closedSession) {
          const closed = result.closedSession;
          // v1.1 invariant: the activity view only shows eligible sessions —
          // a sub-30s switch record stays out of the list and statistics.
          if (closed.statisticsEligible) {
            setLogs(p => upsertLogNewestFirst(p, sessionToLog(closed)));
            refreshStats();
            setToast(`已保存「${closed.taskTitleSnapshot}」的 ${Math.max(1, Math.round(closed.focusedSeconds / 60))} 分钟，开始新一轮`);
          } else {
            setToast(`「${closed.taskTitleSnapshot}」本次不足 30 秒，未计入统计；已开始新一轮`);
          }
        }
      })
      .catch(() => {
        resync();
        setToast("切换任务失败，已恢复当前状态");
      });
  }, [gateway, applyTimer, refreshStats, resync, switchConfirm]);

  const runRevisionAction = useCallback(async (
    action: "pause" | "resume" | "reset",
  ) => {
    const cur = timerRef.current;
    if (!cur) return;
    if (action === "pause")   applyTimer(await gateway.pauseTimer({ expectedRevision: cur.revision }));
    if (action === "resume")  applyTimer(await gateway.resumeTimer({ expectedRevision: cur.revision }));
    if (action === "reset")   applyTimer(await gateway.resetTimer({ expectedRevision: cur.revision }));
  }, [gateway, applyTimer]);

  // Refresh the activity list from persisted sessions (used after reset and
  // mode switches, both of which write sessions server-side).
  const resyncLogs = useCallback(() => {
    gateway.listSessions({ limit: 50 })
      .then(sessions => { setLogs(sortLogsDesc(sessions.map(sessionToLog))); })
      .catch(() => undefined);
  }, [gateway]);

  const handlePause = useCallback(() => { runRevisionAction("pause").catch(resync); }, [runRevisionAction, resync]);
  const handleResume = useCallback(() => { runRevisionAction("resume").catch(resync); }, [runRevisionAction, resync]);

  // Reset (= end/abandon the session) refreshes both the activity log and the
  // statistics: the abandoned attempt is stored but never counted (spec §6).
  // v1.1: manual finish via finish_timer — records actual focused time;
  // eligible sessions join the activity view, short ones only toast.
  const runFinish = useCallback(() => {
    const cur = timerRef.current;
    if (!cur || !cur.activeSessionId) return;
    gateway.finishTimer({
      expectedRevision: cur.revision,
      activeSessionId: cur.activeSessionId,
    })
      .then(result => {
        applyTimer(result.timer);
        if (result.newlyFinished) {
          // v1.1.2 C1: head-insert (newest-first), deduped by session id.
          setLogs(p => upsertLogNewestFirst(p, sessionToLog(result.session)));
          refreshStats();
          if (result.statisticsEligible) {
            setToast(`已记录 ${formatFocusedDuration(result.session.focusedSeconds)} 专注`);
          } else {
            setToast("本次不足 30 秒，未计入统计");
          }
        }
      })
      .catch(resync);
  }, [gateway, applyTimer, refreshStats, resync]);

  const handleReset = useCallback(() => {
    runRevisionAction("reset")
      .then(() => { resyncLogs(); refreshStats(); })
      .catch(resync);
  }, [runRevisionAction, resyncLogs, refreshStats, resync]);

  const confirmReset = useCallback(() => {
    setResetConfirm(false);
    handleReset();
    setToast("本次进度已重置。");
  }, [handleReset]);


  // Goal ring / today minutes: completed focus sessions only.
  const countedFocusLogs = logs.filter(isCountedFocus);
  const focusSessionCount = countedFocusLogs.length;

  const handleSwitchMode = useCallback((mode: TimerMode) => {
    const cur = timerRef.current;
    if (!cur) return;
    gateway.switchTimerMode({ mode, expectedRevision: cur.revision })
      .then(applyTimer)
      .then(() => { resyncLogs(); refreshStats(); })
      .catch(resync);
  }, [gateway, applyTimer, resync, resyncLogs, refreshStats]);

  // ─── System tray ──────────────────────────────────────────────────────────
  // The App root owns the tray surface because it has the authoritative timer
  // ref + settings. Remaining is derived drift-free from `targetEndAt`, so the
  // tray stays correct across throttling and system sleep without a local
  // accumulator. Push immediately on any timer change, then every second while
  // running.
  useEffect(() => {
    const push = () => {
      const t = timerRef.current;
      gateway.setTrayIndicator(formatTrayIndicator(t, Date.now())).catch(() => undefined);
    };
    push();
    if (timer?.state !== "running") return;
    const id = setInterval(push, 1000);
    return () => clearInterval(id);
  }, [timer, gateway]);

  // Rust completion backstop → reuse the same idempotent `handleExpire` path.
  useEffect(() => {
    return gateway.subscribeTimerExpired(() => handleExpire());
  }, [gateway, handleExpire]);

  // Tray menu actions route through the existing handlers so the optimistic-
  // concurrency revision flow stays single-sourced.
  useEffect(() => {
    return gateway.subscribeTrayAction(action => {
      const t = timerRef.current;
      if (!t) return;
      if (action === "toggle") {
        if (t.state === "running") handlePause();
        else if (t.state === "paused") handleResume();
      } else if (action === "reset") {
        handleReset();
      }
    });
  }, [gateway, handlePause, handleResume, handleReset]);

  // Global-shortcut conflict (another app owns the hotkey): show a warning
  // banner instead of failing silently. The app launched fine — the hotkey is
  // just disabled until the conflict is resolved and the app restarted.
  useEffect(() => {
    return gateway.subscribeGlobalShortcutConflict(shortcut => {
      setShortcutConflict(shortcut);
    });
  }, [gateway]);

  // Load persisted state once, then recover an expired running timer
  // (recovery=true → no auto-break per spec 4.2).
  useEffect(() => {
    let cancelled = false;
    gateway.bootstrap()
      .then(async payload => {
        if (cancelled) return;
        setTasks(payload.tasks);
        setTags(payload.tags);
        setLogs(sortLogsDesc(payload.sessions.map(sessionToLog)));
        setSettings(payload.settings);
        applyTimer(payload.timer);
        const t = payload.timer;
        if (t.state === "running" && t.activeSessionId && t.targetEndAt && Date.now() >= t.targetEndAt) {
          await runComplete(t, true).catch(resync);
        }
        refreshStats();
      })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, [gateway, applyTimer, runComplete, resync, refreshStats]);

  const saveSettings = useCallback(async (next: AppSettings) => {
    const result = await gateway.saveSettings(next);
    setSettings(result.settings);
    applyTimer(result.timer); // idle/done timers pick up new durations at once
    refreshStats(); // dailyGoal is baked into the statistics payload
  }, [gateway, applyTimer, refreshStats]);

  const createTask = useCallback(async (input: CreateTaskInput) => {
    const task = await gateway.createTask(input);
    setTasks(p => [...p, task]);
  }, [gateway]);

  // ─── Tag operations (v1.1) ─────────────────────────────────────────────────
  const createTagOp = useCallback(async (name: string) => {
    const tag = await gateway.createTag({ name });
    setTags(p => [...p, tag]);
  }, [gateway]);

  const renameTagOp = useCallback(async (id: string, name: string) => {
    const tag = await gateway.updateTag({ id, name });
    setTags(p => p.map(t => (t.id === tag.id ? tag : t)));
  }, [gateway]);

  const reorderTagOp = useCallback(async (id: string, direction: number) => {
    setTags(await gateway.reorderTag({ id, direction }));
  }, [gateway]);

  const removeTag = useCallback(async (id: string) => {
    const result = await gateway.deleteTag(id);
    setTags(result.tags);
    setTasks(result.tasks);
  }, [gateway]);

  const toggleTask = useCallback(async (id: string) => {
    const current = tasks.find(t => t.id === id);
    if (!current) return;
    const updated = await gateway.updateTask({ id, done: !current.done });
    setTasks(p => p.map(t => (t.id === id ? updated : t)));
  }, [gateway, tasks]);

  const deleteTask = useCallback(async (id: string) => {
    await gateway.deleteTask(id);
    setTasks(p => p.filter(t => t.id !== id));
  }, [gateway]);

  const cyclePriority = useCallback(async (id: string) => {
    const cycle: TaskPriority[] = ["low", "med", "high"];
    const current = tasks.find(t => t.id === id);
    if (!current) return;
    const next = cycle[(cycle.indexOf(current.priority) + 1) % cycle.length];
    const updated = await gateway.updateTask({ id, priority: next });
    setTasks(p => p.map(t => (t.id === id ? updated : t)));
  }, [gateway, tasks]);

  // The highlighted task follows the backend while a round exists; while
  // idle/done-waiting it is the user's pending selection (v1.1.2 B2).
  const active = timer?.state === "running" || timer?.state === "paused";
  const selectorTaskId = active ? timer?.selectedTaskId ?? null : pendingTaskId;

  const centerContent = (() => {
    switch (nav) {
      case "timer":    return (
        <TimerPanel
          timer={timer}
          tasks={tasks}
          selectedTaskId={selectorTaskId}
          onSelectTask={handleSelectTask}
          onStart={handleStart}
          onPause={handlePause}
          onResume={handleResume}
          onReset={handleReset}
          onResetRequest={() => setResetConfirm(true)}
          onFinish={runFinish}
          onSwitchMode={handleSwitchMode}
          onExpire={handleExpire}
        />
      );
      case "tasks":    return (
        <TasksPanel
          tasks={tasks}
          tags={tags}
          onCreateTask={createTask}
          onToggleTask={toggleTask}
          onDeleteTask={deleteTask}
          onCyclePriority={cyclePriority}
          onNotify={setToast}
          tagOps={{
            createTag: createTagOp,
            renameTag: renameTagOp,
            reorderTag: reorderTagOp,
            previewDeleteTag: id => gateway.previewDeleteTag(id),
            deleteTag: removeTag,
          }}
        />
      );
      case "stats":    return <StatsPage logs={logs} todayStats={todayStats} stats={weekStats} />;
      case "settings": return <SettingsPanel settings={settings} onSaveSettings={saveSettings} onDataChanged={() => { resync(); refreshStats(); }} />;
    }
  })();

  const showRight = nav === "timer" || nav === "tasks";

  return (
    <div style={{
      width: "100%", height: "100%", position: "relative",
      background: "#050709", overflow: "hidden",
      display: "flex",
    }}>
      <OceanVideo reduceMotion={activeSettings.reduceMotion} />

      {shortcutConflict && (
        <div
          role="alert"
          style={{
            position: "absolute", top: 12, left: "50%", transform: "translateX(-50%)",
            zIndex: 50, maxWidth: "min(680px, 92vw)", padding: "10px 14px",
            borderRadius: 10, background: "rgba(60, 22, 22, 0.92)",
            border: "1px solid rgba(255, 120, 120, 0.55)", color: "#ffd9d9",
            fontSize: 13, lineHeight: 1.5, boxShadow: "0 8px 28px rgba(0,0,0,0.45)",
            display: "flex", alignItems: "center", gap: 12, backdropFilter: "blur(6px)",
          }}
        >
          <span style={{ flex: 1 }}>
            全局快捷键 <code style={{ color: "#ffb3b3" }}>{shortcutConflict}</code> 被其它程序占用，热键已禁用。关闭占用程序后重新打开本应用即可恢复。
          </span>
          <button
            onClick={() => setShortcutConflict(null)}
            aria-label="关闭提示"
            style={{
              background: "transparent", border: "none", color: "#ffd9d9",
              fontSize: 16, lineHeight: 1, cursor: "pointer", padding: 2,
            }}
          >
            ×
          </button>
        </div>
      )}

      {switchConfirm && (
        <ConfirmDialog
          open
          message={`切换到「${switchConfirm.title}」？将保存当前任务的已专注时间，并为新任务开始新一轮。`}
          confirmLabel="切换任务"
          onConfirm={confirmSwitchTask}
          onCancel={() => setSwitchConfirm(null)}
        />
      )}

      <Sidebar active={nav} onNav={setNav} />

      {/* Content: main (1fr) + right sidebar (clamp width) on same ocean canvas */}
      <div
        className={`content-area${showRight ? " with-right" : ""}`}
        style={{
          gridTemplateColumns: showRight
            ? "1fr clamp(240px, 18vw, 320px)"
            : "1fr",
        }}
      >
        <main style={{ overflow: "hidden", display: "flex", minWidth: 0 }}>
          <div key={nav} className="panel-enter" style={{ flex: 1, display: "flex", overflow: "hidden", minWidth: 0 }}>
            {centerContent}
          </div>
        </main>

        {showRight && <StatsPanel logs={logs} todayStats={todayStats} stats={weekStats} reduceMotion={activeSettings.reduceMotion} />}
      </div>
    </div>
  );
}
