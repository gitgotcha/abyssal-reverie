import { useState } from "react";
import type { Tag, TagDeletePreview } from "../../domain/models";
import { C, CARD } from "../shared/palette";
import { HorizonDivider } from "../timer/GoalRing";
import { searchEntities } from "../../domain/search";

/** Frosted-glass tag manager (v1.1 §11.5): create, rename, reorder and
 *  safely delete tags. The fallback tag is renameable but never deletable. */
export function TagManager({ open, embedded = false, tags, onClose, onCreate, onRename, onReorder, onPreviewDelete, onDelete }: {
  open: boolean;
  embedded?: boolean;
  tags: Tag[];
  onClose: () => void;
  onCreate: (name: string) => Promise<unknown>;
  onRename: (id: string, name: string) => Promise<unknown>;
  onReorder: (id: string, direction: number) => Promise<unknown>;
  onPreviewDelete: (id: string) => Promise<TagDeletePreview>;
  onDelete: (id: string) => Promise<unknown>;
}) {
  const [newName, setNewName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  const [confirmText, setConfirmText] = useState("");
  const [search, setSearch] = useState("");

  if (!open) return null;

  const run = (id: string, fn: () => Promise<unknown>) =>
    fn().then(() => setRowErrors(prev => ({ ...prev, [id]: "" })))
        .catch((e: unknown) => {
          const message = e && typeof e === "object" && "message" in e
            ? String((e as { message: unknown }).message) : "操作失败";
          setRowErrors(prev => ({ ...prev, [id]: message }));
        });

  const submitCreate = () => {
    const name = newName.trim();
    if (!name) { setCreateError("标签名称不能为空"); return; }
    onCreate(name)
      .then(() => { setNewName(""); setCreateError(null); })
      .catch((e: unknown) => {
        setCreateError(e && typeof e === "object" && "message" in e
          ? String((e as { message: unknown }).message) : "创建失败");
      });
  };

  const beginDelete = (id: string) => {
    onPreviewDelete(id)
      .then((preview: TagDeletePreview) => {
        setConfirmingId(id);
        setConfirmText(
          preview.affectedTasks > 0
          ? `该标签正在被 ${preview.affectedTasks} 个任务使用。删除后，这些任务将变为未设置标签。`
            : "确认删除该标签？"
        );
      })
      .catch((e: unknown) => {
        setRowErrors(prev => ({ ...prev, [id]: e && typeof e === "object" && "message" in e
          ? String((e as { message: unknown }).message) : "删除预览失败" }));
      });
  };

  const iconBtn = (label: string, disabled = false) => ({
    width: 22, height: 22, borderRadius: 5, flexShrink: 0,
    background: "transparent", border: "1px solid transparent",
    color: disabled ? "rgba(215,228,230,0.14)" : C.textMuted,
    cursor: disabled ? "default" : "pointer",
    display: "flex", alignItems: "center", justifyContent: "center",
    opacity: disabled ? 0.5 : 1,
  } as const);
  const visibleTags = search.trim()
    ? searchEntities(tags, search, tag => [tag.name], tag => tag.id)
    : tags;

  return (
    <div role="dialog" aria-modal="true" aria-label="管理标签"
      onClick={onClose}
      style={{
        position: embedded ? "relative" : "fixed", inset: embedded ? undefined : 0,
        zIndex: embedded ? 1 : 85,
        background: embedded ? "transparent" : "rgba(2,3,5,0.45)",
        display: "flex", flex: embedded ? 1 : undefined,
        alignItems: embedded ? "stretch" : "center", justifyContent: "center",
      }}>
      <div role="document" onClick={e => e.stopPropagation()}
        style={{
          width: embedded ? "100%" : "min(420px, 90vw)", maxHeight: embedded ? "100%" : "80vh", overflowY: "auto",
          padding: "18px 20px", borderRadius: 14,
          background: "rgba(8, 13, 18, 0.85)",
          backdropFilter: "blur(24px) saturate(1.05)", WebkitBackdropFilter: "blur(24px) saturate(1.05)",
          border: "1px solid rgba(215,228,230,0.14)",
          boxShadow: embedded ? "none" : "0 18px 48px rgba(2,3,5,0.5)",
          display: "flex", flexDirection: "column", gap: 12,
        }}>
        <div style={{ display: "flex", alignItems: "center" }}>
          <span style={{ fontSize: 13, fontWeight: 500, color: C.textPrimary, fontFamily: "var(--font-sans)" }}>管理标签</span>
          <button onClick={onClose} aria-label="关闭标签管理" className="btn-delete"
            style={{ marginLeft: "auto", ...iconBtn("关闭") }}>
            <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
              <path d="M2 2L8 8M8 2L2 8" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
            </svg>
          </button>
        </div>
        <HorizonDivider />

        <input value={search} onChange={event => setSearch(event.target.value)}
          placeholder="搜索标签…" aria-label="搜索标签"
          className="input-ocean" style={{ fontFamily: "var(--font-sans)", fontSize: 11, color: C.textPrimary,
            background: C.cardDim, border: `1px solid ${C.hairline}`, borderRadius: 7, padding: "6px 9px", cursor: "text" }} />

        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {visibleTags.map(tag => {
            const isRenaming = renamingId === tag.id;
            const isConfirming = confirmingId === tag.id;
            return (
              <div key={tag.id} style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "5px 6px", borderRadius: 8, background: "rgba(27,37,44,0.20)" }}>
                  {isRenaming ? (
                    <input
                      value={renameValue}
                      onChange={e => setRenameValue(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === "Enter") { run(tag.id, () => onRename(tag.id, renameValue)).then(() => setRenamingId(null)); }
                        if (e.key === "Escape") setRenamingId(null);
                      }}
                      autoFocus
                      aria-label="重命名标签"
                      className="input-ocean"
                      style={{ flex: 1, padding: "4px 8px", borderRadius: 6, fontSize: 12, color: C.textPrimary, fontFamily: "var(--font-sans)" }}
                    />
                  ) : (
                    <button
                      onClick={() => { setRenamingId(tag.id); setRenameValue(tag.name); }}
                      aria-label={`重命名标签 ${tag.name}`}
                      title="点击重命名"
                      style={{ flex: 1, textAlign: "left", background: "transparent", border: "none", cursor: "pointer", padding: 0, fontSize: 12, color: C.textSec, fontFamily: "var(--font-sans)" }}>
                      {tag.name}
                    </button>
                  )}
                  {tag.isFallback && (
                    <span title="保底标签不可删除" style={{
                      fontSize: 8, color: C.textMuted, border: `0.5px solid ${C.hairlineStr}`,
                      borderRadius: 4, padding: "1px 4px", flexShrink: 0, fontFamily: "var(--font-sans)",
                    }}>保底</span>
                  )}
                  <button onClick={() => run(tag.id, () => onReorder(tag.id, -1))}
                    disabled={isRenaming} aria-label={`上移标签 ${tag.name}`} title="上移" style={iconBtn("上移", isRenaming)}>
                    <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
                      <path d="M5 8V2M2 5l3-3 3 3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </button>
                  <button onClick={() => run(tag.id, () => onReorder(tag.id, 1))}
                    disabled={isRenaming} aria-label={`下移标签 ${tag.name}`} title="下移" style={iconBtn("下移", isRenaming)}>
                    <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
                      <path d="M5 2v6M2 5l3 3 3-3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </button>
                  {isConfirming ? (
                    <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                      <button onClick={() => run(tag.id, () => onDelete(tag.id)).then(() => setConfirmingId(null))}
                        aria-label="确认删除" className="btn-delete"
                        style={{ fontSize: 10, padding: "2px 8px", borderRadius: 5, color: "rgba(231,138,138,0.95)", border: "1px solid rgba(231,138,138,0.35)", background: "rgba(231,138,138,0.10)", cursor: "pointer", fontFamily: "var(--font-sans)" }}>
                        确认
                      </button>
                      <button onClick={() => setConfirmingId(null)} aria-label="取消删除"
                        style={{ fontSize: 10, padding: "2px 8px", borderRadius: 5, color: C.textMuted, border: `1px solid ${C.hairline}`, background: "transparent", cursor: "pointer", fontFamily: "var(--font-sans)" }}>
                        取消
                      </button>
                    </div>
                  ) : (
                    <button onClick={() => beginDelete(tag.id)} disabled={tag.isFallback || isRenaming}
                      aria-label={`删除标签 ${tag.name}`} title={tag.isFallback ? "保底标签不可删除" : "删除标签"}
                      className="btn-delete" style={iconBtn("删除标签", tag.isFallback)}>
                      <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
                        <path d="M2 2L8 8M8 2L2 8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
                      </svg>
                    </button>
                  )}
                </div>
                {isConfirming && <div style={{ fontSize: 10, color: "rgba(231,190,150,0.9)", padding: "0 6px 2px", fontFamily: "var(--font-sans)" }}>{confirmText}</div>}
                {rowErrors[tag.id] && (
                  <div role="alert" style={{ fontSize: 9, color: "rgba(231,138,138,0.92)", padding: "0 6px", fontFamily: "var(--font-sans)" }}>
                    {rowErrors[tag.id]}
                  </div>
                )}
              </div>
            );
          })}
          {search.trim() && visibleTags.length === 0 && (
            <div style={{ padding: "12px 6px", color: C.textMuted, fontFamily: "var(--font-sans)", fontSize: 11 }}>
              没有匹配的标签
            </div>
          )}
        </div>

        <HorizonDivider />

        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <div style={{ display: "flex", gap: 6 }}>
            <input
              value={newName}
              onChange={e => { setNewName(e.target.value); setCreateError(null); }}
              onKeyDown={e => e.key === "Enter" && submitCreate()}
              placeholder="新标签名称…"
              aria-label="新标签名称"
              className="input-ocean"
              style={{ flex: 1, padding: "6px 10px", borderRadius: 8, fontSize: 12, color: C.textPrimary, fontFamily: "var(--font-sans)" }}
            />
            <button onClick={submitCreate} className="btn-add" aria-label="添加标签"
              style={{
                padding: "5px 12px", borderRadius: 7, fontSize: 11, color: C.moonlight, cursor: "pointer",
                background: "rgba(27,37,44,0.55)", border: `1px solid ${C.hairlineStr}`, fontFamily: "var(--font-sans)",
              }}>
              添加
            </button>
          </div>
          {createError && <div role="alert" style={{ fontSize: 9, color: "rgba(231,138,138,0.92)", fontFamily: "var(--font-sans)" }}>{createError}</div>}
        </div>
      </div>
    </div>
  );
}
