import { useState } from "react";
import type { Category, Project } from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { searchEntities } from "../../domain/search";

/** v1.2 F6: project & category management — create projects under a category,
 *  rename, move between categories, archive; categories support create,
 *  rename and archive (deletion requires moving its projects first, which the
 *  archive-first model avoids entirely). */
export function ProjectManager({ open, embedded = false, categories, projects, onClose, createProject, renameProject, archiveProject, createCategory, renameCategory, archiveCategory, moveProject }: {
  open: boolean;
  embedded?: boolean;
  categories: Category[];
  projects: Project[];
  onClose: () => void;
  createProject: (name: string, categoryId: string) => Promise<unknown>;
  renameProject: (id: string, name: string) => Promise<unknown>;
  archiveProject: (id: string, archived: boolean) => Promise<unknown>;
  createCategory: (name: string) => Promise<unknown>;
  renameCategory: (id: string, name: string) => Promise<unknown>;
  archiveCategory: (id: string, archived: boolean) => Promise<unknown>;
  moveProject: (id: string, categoryId: string) => Promise<unknown>;
}) {
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectCategory, setNewProjectCategory] = useState("");
  const [newCategoryName, setNewCategoryName] = useState("");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!open) return null;

  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const field = {
    fontFamily: "var(--font-sans)", fontSize: 12, color: C.textPrimary,
    background: C.cardDim, border: `1px solid ${C.hairline}`,
    borderRadius: 7, padding: "5px 8px",
  } as const;

  const activeCategories = categories.filter(c => c.status === "active");
  const visibleProjects = search.trim()
    ? searchEntities(projects, search, project => [project.name, project.description], project => project.id)
    : projects;

  return (
    <div role="dialog" aria-modal="true" aria-label="管理项目与类别" onClick={onClose}
      style={{
        position: embedded ? "relative" : "fixed", inset: embedded ? undefined : 0,
        zIndex: embedded ? 1 : 85, background: embedded ? "transparent" : "rgba(2,3,5,0.45)",
        display: "flex", flex: embedded ? 1 : undefined,
        alignItems: embedded ? "stretch" : "center", justifyContent: "center",
      }}>
      <div role="document" onClick={e => e.stopPropagation()}
        style={{
          width: embedded ? "100%" : "min(480px, 94vw)", maxHeight: embedded ? "100%" : "86vh", overflowY: "auto",
          padding: "18px 20px", borderRadius: 14,
          background: "rgba(8, 13, 18, 0.88)",
          backdropFilter: "blur(22px) saturate(1.05)", WebkitBackdropFilter: "blur(22px) saturate(1.05)",
          border: "1px solid rgba(215,228,230,0.14)",
          boxShadow: embedded ? "none" : "0 18px 48px rgba(2,3,5,0.5)",
          display: "flex", flexDirection: "column", gap: 12,
        }}>
        <div style={{ fontSize: 13, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>项目与类别</div>

        <input value={search} onChange={event => setSearch(event.target.value)}
          placeholder="搜索项目…" aria-label="搜索项目"
          className="input-ocean" style={{ fontFamily: "var(--font-sans)", fontSize: 11, color: C.textPrimary,
            background: C.cardDim, border: `1px solid ${C.hairline}`, borderRadius: 7, padding: "6px 9px", cursor: "text" }} />

        {activeCategories.filter(cat => !search.trim() || visibleProjects.some(project => project.categoryId === cat.id)).map(cat => {
          const catProjects = visibleProjects.filter(p => p.categoryId === cat.id);
          return (
            <div key={cat.id} style={{ padding: "10px 12px", ...CARD }}>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span style={{ fontSize: 11, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>{cat.name}</span>
                <span style={{ fontFamily: "var(--font-mono)", fontSize: 9, color: C.textMuted }}>{catProjects.length}</span>
                <span style={{ flex: 1 }} />
                <button onClick={() => {
                  const name = window.prompt("类别改名", cat.name);
                  if (name && name.trim()) void run(() => renameCategory(cat.id, name.trim()));
                }} className="btn-filter" style={miniBtn("text")}>改名</button>
                <button onClick={() => void run(() => archiveCategory(cat.id, cat.status !== "archived"))} className="btn-filter" style={miniBtn("text")}>{cat.status === "archived" ? "恢复" : "归档"}</button>
              </div>
              {catProjects.length === 0 ? (
                <div style={{ fontSize: 10, color: C.textMuted, fontFamily: "var(--font-sans)", fontStyle: "italic", padding: "4px 0" }}>
                  尚未添加项目
                </div>
              ) : catProjects.map(p => (
                <div key={p.id} style={{ display: "flex", alignItems: "center", gap: 6, padding: "4px 0" }}>
                  <span style={{
                    flex: 1, fontSize: 11, color: p.status === "archived" ? "rgba(165,182,188,0.30)" : C.textSec,
                    fontFamily: "var(--font-sans)",
                    textDecoration: p.status === "archived" ? "line-through" : "none",
                  }}>{p.name}</span>
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: 9, color: C.textMuted }}>
                    {p.doneTaskCount}/{p.taskCount} 已完成
                  </span>
                  <select value={p.categoryId} onChange={e => void run(() => moveProject(p.id, e.target.value))}
                    aria-label="移动项目到其他类别" style={{ ...field, fontSize: 10, padding: "2px 5px", cursor: "pointer" }}>
                    {activeCategories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                  {p.status === "archived" ? (
                    <button onClick={() => void run(() => archiveProject(p.id, false))} className="btn-filter" style={miniBtn("text")}>恢复</button>
                  ) : (
                    <button onClick={() => void run(() => archiveProject(p.id, true))} className="btn-filter" style={miniBtn("text")}>归档</button>
                  )}
                  <button onClick={() => {
                    const name = window.prompt("项目改名", p.name);
                    if (name && name.trim()) void run(() => renameProject(p.id, name.trim()));
                  }} className="btn-filter" style={miniBtn("text")}>改名</button>
                </div>
              ))}
            </div>
          );
        })}

        {search.trim() && visibleProjects.length === 0 && (
          <div style={{ padding: "12px 4px", color: C.textMuted, fontFamily: "var(--font-sans)", fontSize: 11 }}>
            没有匹配的项目
          </div>
        )}

        <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
          <input value={newProjectName} onChange={e => setNewProjectName(e.target.value)}
            placeholder="新项目名称" aria-label="新项目名称" style={{ ...field, flex: 1, minWidth: 120, cursor: "text" }} />
          <select value={newProjectCategory} onChange={e => setNewProjectCategory(e.target.value)}
            aria-label="所属类别" style={{ ...field, cursor: "pointer" }}>
            <option value="">选择类别</option>
            {activeCategories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
          </select>
          <button onClick={() => {
            if (!newProjectName.trim() || !newProjectCategory) { setError("项目需要名称和类别"); return; }
            void run(async () => {
              await createProject(newProjectName.trim(), newProjectCategory);
              setNewProjectName("");
            });
          }} className="btn-action" disabled={busy}
            style={{
              padding: "5px 12px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
              cursor: "pointer", color: "#0B1116", background: "rgba(186,200,204,0.92)",
              border: "1px solid rgba(215,228,230,0.30)",
            }}>添加项目</button>
        </div>

        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input value={newCategoryName} onChange={e => setNewCategoryName(e.target.value)}
            placeholder="新类别名称" aria-label="新类别名称" style={{ ...field, flex: 1, cursor: "text" }} />
          <button onClick={() => {
            if (!newCategoryName.trim()) return;
            void run(async () => {
              await createCategory(newCategoryName.trim());
              setNewCategoryName("");
            });
          }} className="btn-action" disabled={busy}
            style={{
              padding: "5px 12px", borderRadius: 8, fontSize: 11, fontFamily: "var(--font-sans)",
              cursor: "pointer", color: "rgba(195,212,218,0.85)",
              background: "rgba(27,37,44,0.40)", border: "1px solid rgba(215,228,230,0.12)",
            }}>添加类别</button>
        </div>

        {error && <div role="alert" style={{ fontSize: 11, color: "#ffb3b3", fontFamily: "var(--font-sans)" }}>{error}</div>}

        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button onClick={onClose} className="btn-action"
            style={{
              padding: "6px 14px", borderRadius: 8, fontSize: 12, fontFamily: "var(--font-sans)",
              cursor: "pointer", color: "#0B1116", background: "rgba(186,200,204,0.92)",
              border: "1px solid rgba(215,228,230,0.30)", fontWeight: 500,
            }}>完成</button>
        </div>
      </div>
    </div>
  );

  function miniBtn(_kind: string): React.CSSProperties {
    return {
      fontFamily: "var(--font-sans)", fontSize: 9, padding: "2px 7px", borderRadius: 5,
      border: `0.5px solid ${C.hairlineStr}`, background: "rgba(27,37,44,0.30)",
      color: C.moonlight, cursor: "pointer",
    };
  }
}
