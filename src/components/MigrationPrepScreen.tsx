import { useEffect, useState } from "react"
import type { AppGateway } from "../services/appGateway"
import type { MigrationParams, MigrationPreview } from "../domain/models"
import type { BootstrapPayload } from "../domain/models"

/**
 * R06: shown when the on-disk database is an OLD schema. Nothing has been
 * converted yet — the preview is read-only and the user must confirm the
 * budget basis and the 通用 mapping before any semantic conversion runs.
 * Cancelling exits the app without touching any data.
 */
export function MigrationPrepScreen({ gateway, onMigrated }: {
  gateway: AppGateway;
  onMigrated: (payload: BootstrapPayload) => void;
}) {
  const [preview, setPreview] = useState<MigrationPreview | null>(null)
  const [budget, setBudget] = useState(25)
  const [generalMapping, setGeneralMapping] = useState<"standalone" | "project">("standalone")
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [previewError, setPreviewError] = useState<string | null>(null)

  useEffect(() => {
    gateway.previewMigration()
      .then(p => {
        setPreview(p)
        setBudget(p.suggestedFocusMinutes)
      })
      .catch(err => setPreviewError(err instanceof Error ? err.message : String(err)))
  }, [gateway])

  const confirm = async () => {
    if (busy) return
    setBusy(true)
    setError(null)
    const params: MigrationParams = { budgetFocusMinutes: budget, generalMapping }
    try {
      const payload = await gateway.confirmMigration(params)
      onMigrated(payload)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  if (previewError) {
    return (
      <Shell>
        <div role="alert" style={{ fontSize: 13, color: "#ffb3b3" }}>
          无法读取迁移预览：{previewError}
        </div>
        <button onClick={() => void gateway.cancelUpgrade()} style={primaryBtn}>
          退出应用
        </button>
      </Shell>
    )
  }

  return (
    <Shell>
      <h1 style={{ fontSize: 16, fontWeight: 500, margin: 0, color: "#E7EFF0" }}>
        需要升级数据库
      </h1>
      <p style={{ fontSize: 12, lineHeight: 1.7, color: "rgba(220,232,236,0.75)", margin: 0 }}>
        检测到旧版本的数据结构。<strong>确认前不会修改任何数据</strong>；取消则退出应用，
        原数据保持不变。
      </p>

      {!preview ? (
        <div style={{ fontSize: 12, color: "rgba(220,232,236,0.6)" }}>正在读取预览…</div>
      ) : (
        <>
          <div style={{ padding: "10px 12px", borderRadius: 10, background: "rgba(27,37,44,0.4)", border: "1px solid rgba(215,228,230,0.14)", fontSize: 12, lineHeight: 1.8, color: "#E7EFF0" }}>
            <div>任务：<strong>{preview.taskCount}</strong> 条 · 专注记录：<strong>{preview.sessionCount}</strong> 条</div>
            {preview.migrationKind === "legacySemantic" ? <div>将创建正式项目：<strong>{preview.projectsToCreate.length}</strong> 个
              {preview.projectsToCreate.length > 0 && (
                <span style={{ color: "rgba(220,232,236,0.6)" }}>
                  （{preview.projectsToCreate.slice(0, 8).join("、")}
                  {preview.projectsToCreate.length > 8 ? " …" : ""}）
                </span>
              )}
            </div> : <div>本次只允许任务的标签和优先级保持“未设置”，不会改写旧任务、预算或历史记录。</div>}
          </div>

          {preview.migrationKind === "legacySemantic" && <><label style={fieldRow}>
            预算基准（每个番茄的分钟数，旧任务预算按此推算，来源会标注为“迁移估算”）
            <input type="number" min={1} max={180} value={budget}
              onChange={e => setBudget(Number(e.target.value) || 1)}
              style={inputStyle} />
          </label>

          <div style={fieldRow}>
            「通用」任务的映射（共 {preview.generalTaskCount} 条）
            <div style={{ display: "flex", gap: 10, marginTop: 6 }}>
              <label style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer" }}>
                <input type="radio" name="general" checked={generalMapping === "standalone"}
                  onChange={() => setGeneralMapping("standalone")} />
                保持独立任务（推荐）
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer" }}>
                <input type="radio" name="general" checked={generalMapping === "project"}
                  onChange={() => setGeneralMapping("project")} />
                创建正式项目「通用」
              </label>
            </div>
          </div></>}
        </>
      )}

      {error && <div role="alert" style={{ fontSize: 12, color: "#ffb3b3" }}>{error}</div>}

      <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
        <button onClick={() => void gateway.cancelUpgrade()} style={secondaryBtn}>
          取消并退出
        </button>
        <button onClick={() => void confirm()} disabled={busy || !preview} style={{
          ...primaryBtn,
          opacity: busy || !preview ? 0.5 : 1,
          cursor: busy || !preview ? "default" : "pointer",
        }}>
          {busy ? "迁移中…" : "确认升级"}
        </button>
      </div>
    </Shell>
  )
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div style={{
      height: "100vh", display: "flex", flexDirection: "column", gap: 14,
      justifyContent: "center", maxWidth: 560, margin: "0 auto", padding: 24,
      background: "#050709", color: "#E7EFF0", fontFamily: "var(--font-sans)",
    }}>
      {children}
    </div>
  )
}

const primaryBtn: React.CSSProperties = {
  padding: "8px 16px", borderRadius: 8, fontSize: 12, fontWeight: 500,
  color: "#0B1116", background: "rgba(186,200,204,0.92)",
  border: "1px solid rgba(215,228,230,0.30)", cursor: "pointer",
}
const secondaryBtn: React.CSSProperties = {
  padding: "8px 16px", borderRadius: 8, fontSize: 12,
  color: "rgba(195,212,218,0.85)", background: "rgba(27,37,44,0.40)",
  border: "1px solid rgba(215,228,230,0.12)", cursor: "pointer",
}
const fieldRow: React.CSSProperties = {
  display: "flex", flexDirection: "column", gap: 6, fontSize: 12,
  color: "rgba(220,232,236,0.75)",
}
const inputStyle: React.CSSProperties = {
  padding: "7px 10px", borderRadius: 8, fontSize: 13, color: "#E7EFF0",
  background: "rgba(27,37,44,0.4)", border: "1px solid rgba(215,228,230,0.14)",
}
