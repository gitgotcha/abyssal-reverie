import { useCallback, useEffect, useRef, useState } from "react";
import { HorizonDivider } from "../timer/GoalRing";
import type { AppSettings, ImportPreview } from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { DurationStepper } from "./DurationStepper";
import { useAppGateway } from "../../services/gatewayContext";
import { chineseDate } from "../shared/format";
import { APP_VERSION } from "../../domain/buildInfo";

export function SettingsPanel({ settings, onSaveSettings, onDataChanged }: {
  settings: AppSettings | null;
  onSaveSettings: (settings: AppSettings) => Promise<unknown>;
  onDataChanged: () => void;
}) {
  const gateway = useAppGateway();
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Sync from the latest persisted settings when they change externally.
  useEffect(() => { setDraft(settings); }, [settings]);

  // Latest persisted settings, readable inside the serial save queue.
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  // v1.1 §11.1 (review suggestion 9): saves run through a serial queue so a
  // slow earlier request can never overwrite a newer one (latest-write-wins
  // by ordering). A failed save restores the last persisted values.
  const saveQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  const persist = useCallback((next: AppSettings) => {
    setSaving(true);
    const run = saveQueueRef.current.then(async () => {
      try {
        await onSaveSettings(next);
        setSaveError(null);
      } catch {
        setDraft(settingsRef.current);
        setSaveError("保存失败，已恢复上次保存的设置");
      } finally {
        setSaving(false);
      }
    });
    saveQueueRef.current = run;
    return run;
  }, [onSaveSettings]);

  const update = useCallback((patch: Partial<AppSettings>) => {
    setDraft(prev => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      void persist(next);
      return next;
    });
  }, [persist]);

  // Launch-at-login lives in the autostart plugin / OS registry, not in our
  // SQLite `settings` table, so it is tracked with its own local state.
  const [launchAtLogin, setLaunchAtLogin] = useState<boolean | null>(null);
  useEffect(() => {
    gateway.getAutostart()
      .then(setLaunchAtLogin)
      .catch(() => setLaunchAtLogin(false));
  }, [gateway]);

  const toggleLaunchAtLogin = useCallback((next: boolean) => {
    setLaunchAtLogin(next);
    gateway.setAutostart(next).then(setLaunchAtLogin).catch(() => undefined);
  }, [gateway]);

  // ─── Data export & backup (Item 3) ──────────────────────────────────────────
  const [busy, setBusy] = useState<null | 'backup' | 'csv' | 'import'>(null);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(null);

  const errText = (e: unknown) =>
    e && typeof e === 'object' && 'message' in e ? String((e as { message: unknown }).message) : '操作失败';

  const suggestedBackupName = () => {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
    return `abyssal-reverie-backup-${stamp}.json`;
  };

  const handleExportBackup = useCallback(async () => {
    setBusy('backup');
    setStatus(null);
    try {
      const path = await gateway.pickExportPath(suggestedBackupName());
      if (!path) { setBusy(null); return; }
      const summary = await gateway.exportBackup(path);
      setStatus({ ok: true, text: `已导出备份（${summary.tasks} 个任务、${summary.sessions} 条会话，${(summary.bytes / 1024).toFixed(1)} KB）` });
    } catch (e) {
      setStatus({ ok: false, text: errText(e) });
    } finally {
      setBusy(null);
    }
  }, [gateway]);

  const handleExportCsv = useCallback(async () => {
    setBusy('csv');
    setStatus(null);
    try {
      const path = await gateway.pickExportPath('abyssal-reverie-sessions.csv');
      if (!path) { setBusy(null); return; }
      const summary = await gateway.exportSessionsCsv(path);
      setStatus({ ok: true, text: `已导出会话 CSV（${summary.sessions} 条记录）` });
    } catch (e) {
      setStatus({ ok: false, text: errText(e) });
    } finally {
      setBusy(null);
    }
  }, [gateway]);

  const handlePickImport = useCallback(async () => {
    setBusy('import');
    setStatus(null);
    try {
      const path = await gateway.pickImportPath();
      if (!path) { setBusy(null); return; }
      const preview = await gateway.previewImport(path);
      setPendingImportPath(path);
      setImportPreview(preview);
    } catch (e) {
      setStatus({ ok: false, text: errText(e) });
    } finally {
      setBusy(null);
    }
  }, [gateway]);

  const confirmImport = useCallback(async () => {
    if (!pendingImportPath) return;
    const path = pendingImportPath;
    setImportPreview(null);
    setPendingImportPath(null);
    setBusy('import');
    try {
      const summary = await gateway.importBackup(path);
      setStatus({ ok: true, text: `已导入（${summary.tasks} 个任务、${summary.sessions} 条会话），当前数据已覆盖` });
      onDataChanged();
    } catch (e) {
      setStatus({ ok: false, text: errText(e) });
    } finally {
      setBusy(null);
    }
  }, [pendingImportPath, gateway, onDataChanged]);

  const cancelImport = useCallback(() => {
    setImportPreview(null);
    setPendingImportPath(null);
  }, []);

  const Toggle = ({ value, onChange, label }: { value: boolean; onChange: (v: boolean) => void; label: string }) => (
    <button onClick={() => onChange(!value)} className="btn-toggle"
      role="switch" aria-checked={value} aria-label={label}
      style={{
        width:34, height:19, borderRadius:10, flexShrink:0,
        background: value ? "rgba(27,37,44,0.50)" : "rgba(8,13,18,0.24)",
        border:`1px solid ${value ? C.hairlineStr : C.hairline}`,
        position:"relative", cursor:"pointer",
      }}>
      <span style={{
        position:"absolute", top:3, left: value ? 16 : 3,
        width:11, height:11, borderRadius:"50%",
        background: value ? C.silver : "rgba(215,228,230,0.18)",
        transition:"background-color 0.24s, color 0.24s, border-color 0.24s, box-shadow 0.24s, transform 0.24s, opacity 0.24s",
      }} />
    </button>
  );


  const Section = ({ label, children }: { label: string; children: React.ReactNode }) => (
    <div style={{ marginBottom:9 }}>
      <div style={{ fontFamily:"var(--font-sans)", fontSize:9, letterSpacing:"0.13em", color:"rgba(165,182,188,0.40)", padding:"12px 0 6px", textTransform:"uppercase" }}>{label}</div>
      <div style={{ ...CARD, overflow:"hidden" }}>{children}</div>
    </div>
  );

  const Row = ({ label, hint, last, children }: { label:string; hint?:string; last?:boolean; children:React.ReactNode }) => (
    <div style={{
      display:"flex", alignItems:"center", padding:"11px 13px",
      borderBottom: last ? "none" : `0.5px solid rgba(215,228,230,0.05)`,
    }}>
      <div style={{ flex:1 }}>
        <div style={{ fontSize:12, color:C.textSec, fontFamily:"var(--font-sans)" }}>{label}</div>
        {hint && <div style={{ fontSize:10, color:C.textMuted, marginTop:2, fontFamily:"var(--font-sans)" }}>{hint}</div>}
      </div>
      {children}
    </div>
  );

  const ActionButton = ({ label, onClick, disabled, danger }: {
    label: string; onClick: () => void; disabled?: boolean; danger?: boolean;
  }) => (
    <button onClick={onClick} disabled={disabled} className="btn-action"
      style={{
        padding:"5px 14px", borderRadius:7, fontSize:11, fontFamily:"var(--font-sans)",
        cursor: disabled ? "default" : "pointer",
        color: danger ? "rgba(231,138,138,0.95)" : C.textPrimary,
        background: danger ? "rgba(231,138,138,0.10)" : C.cardDim,
        border:`1px solid ${danger ? "rgba(231,138,138,0.30)" : C.hairline}`,
        opacity: disabled ? 0.45 : 1,
      }}>{label}</button>
  );

  if (!draft) {
    return (
      <div className="flex flex-col h-full" style={{ position:"relative", zIndex:2 }}>
        <div style={{ flexShrink:0 }}>
          <div style={{ padding:"10px 22px" }}>
            <span style={{ fontSize:13, fontWeight:500, color:C.textPrimary, fontFamily:"var(--font-sans)" }}>设置</span>
          </div>
          <HorizonDivider />
        </div>
        <div style={{ flex:1, display:"flex", alignItems:"center", justifyContent:"center" }}>
          <span style={{ fontSize:11, color:C.textMuted }}>加载中…</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full" style={{ position:"relative", zIndex:2 }}>
      <div style={{ flexShrink:0 }}>
        <div style={{ display:"flex", alignItems:"center", padding:"10px 22px" }}>
          <span style={{ fontSize:13, fontWeight:500, color:C.textPrimary, fontFamily:"var(--font-sans)" }}>设置</span>
          {saving && <span style={{ marginLeft:"auto", fontSize:10, color:C.textMuted, fontFamily:"var(--font-sans)" }}>保存中…</span>}
        </div>
        <HorizonDivider />
      </div>
      <div className="flex-1 overflow-y-auto" style={{ padding:"4px 22px" }}>
        <Section label="时长（分钟）">
          <Row label="专注"><DurationStepper value={draft.focusDurationMinutes} onChange={v => update({ focusDurationMinutes: v })} min={1} max={180} ariaLabel="专注时长" errorMessage="请输入 1–180 的整数分钟" /></Row>
          <Row label="短休"><DurationStepper value={draft.shortBreakMinutes} onChange={v => update({ shortBreakMinutes: v })} min={1} max={180} ariaLabel="短休时长" errorMessage="请输入 1–180 的整数分钟" /></Row>
          <Row label="长休" last><DurationStepper value={draft.longBreakMinutes} onChange={v => update({ longBreakMinutes: v })} min={1} max={180} ariaLabel="长休时长" errorMessage="请输入 1–180 的整数分钟" /></Row>
        </Section>
        <Section label="行为">
          <Row label="自动开始休息" hint="专注结束后自动继续"><Toggle label="自动开始休息" value={draft.autoStartBreak} onChange={v => update({ autoStartBreak: v })} /></Row>
          <Row label="声音提示"><Toggle label="声音提示" value={draft.soundEnabled} onChange={v => update({ soundEnabled: v })} /></Row>
          <Row label="桌面通知"><Toggle label="桌面通知" value={draft.notificationEnabled} onChange={v => update({ notificationEnabled: v })} /></Row>
          <Row label="降低动态效果" hint="暂停海洋背景动画，降低 CPU 与电量消耗" last><Toggle label="降低动态效果" value={draft.reduceMotion} onChange={v => update({ reduceMotion: v })} /></Row>
        </Section>
        <Section label="目标">
          <Row label="每日专注次数" last><DurationStepper value={draft.dailyGoal} onChange={v => update({ dailyGoal: v })} min={1} max={50} ariaLabel="每日专注次数" /></Row>
        </Section>
        <Section label="系统">
          <Row label="开机自动启动" hint="登录 Windows 后于后台自动运行"><Toggle label="开机自动启动" value={launchAtLogin ?? false} onChange={toggleLaunchAtLogin} /></Row>
          <Row label="全局快捷键" hint="Ctrl + Alt + 空格：开始 / 暂停（窗口隐藏时也能用）" last><span style={{ fontFamily:"var(--font-mono)", fontSize:10, color:C.textMuted }}>Ctrl+Alt+Space</span></Row>
        </Section>
        <Section label="数据">
          <Row label="导出备份" hint="保存全部任务、会话与设置为 JSON 文件">
            <ActionButton label={busy === 'backup' ? '导出中…' : '导出'} onClick={handleExportBackup} disabled={busy !== null} />
          </Row>
          <Row label="导出会话" hint="导出全部专注 / 休息记录为 CSV 表格">
            <ActionButton label={busy === 'csv' ? '导出中…' : '导出'} onClick={handleExportCsv} disabled={busy !== null} />
          </Row>
          <Row label="导入备份" hint="从 JSON 备份恢复，将覆盖当前数据" last>
            <ActionButton label={busy === 'import' ? '读取中…' : '导入'} onClick={handlePickImport} disabled={busy !== null} />
          </Row>
          {status && (
            <div style={{
              padding:"9px 13px", fontSize:10, lineHeight:1.5,
              color: status.ok ? "rgba(126,200,180,0.92)" : "rgba(231,138,138,0.95)",
              fontFamily:"var(--font-sans)",
            }}>{status.text}</div>
          )}
        </Section>
        <div style={{ ...CARD, borderRadius:12, padding:"11px 13px", marginBottom:20 }}>
          <div style={{ fontSize:9, color:"rgba(165,182,188,0.34)", marginBottom:4, fontFamily:"var(--font-sans)", letterSpacing:"0.10em", textTransform:"uppercase" }}>关于</div>
          <div style={{ fontSize:12, color:C.textSec, fontFamily:"var(--font-sans)" }}>深海专注 · 桌面计时器</div>
          <div style={{ fontSize:10, color:C.textMuted, marginTop:2, fontFamily:"var(--font-mono)", letterSpacing:"0.04em" }}>v{APP_VERSION} · 2026</div>
        </div>
      </div>
      {importPreview && (
        <div
          onClick={cancelImport}
          style={{
            position:"fixed", inset:0, zIndex:50,
            background:"rgba(4,8,12,0.55)",
            display:"flex", alignItems:"center", justifyContent:"center",
          }}
        >
          <div
            onClick={e => e.stopPropagation()}
            style={{ ...CARD, width:300, padding:16, borderRadius:14 }}
          >
            <div style={{ fontSize:13, color:C.textPrimary, fontFamily:"var(--font-sans)", marginBottom:8 }}>确认导入备份？</div>
            <div style={{ fontSize:11, color:C.textSec, fontFamily:"var(--font-sans)", lineHeight:1.7, marginBottom:14 }}>
              将导入 <b style={{ color:C.textPrimary }}>{importPreview.tasks}</b> 个任务、<b style={{ color:C.textPrimary }}>{importPreview.sessions}</b> 条会话，<br />
              并<b style={{ color:"rgba(231,138,138,0.95)" }}>覆盖当前所有数据</b>（不可撤销）。
            </div>
            <div style={{ display:"flex", gap:8, justifyContent:"flex-end" }}>
              <ActionButton label="取消" onClick={cancelImport} />
              <ActionButton label="确认导入" onClick={confirmImport} disabled={busy !== null} danger />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ─── Stats Sidebar — transparent frosted glass, content flat on surface ───────
