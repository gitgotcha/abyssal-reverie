import { useEffect, useMemo, useRef, useState } from "react";
import { searchEntities } from "../../domain/search";
import { C } from "./palette";

export interface PickerOption {
  value: string;
  label: string;
  group?: string;
  disabled?: boolean;
}

/** Searchable relationship picker used for optional task metadata. It keeps a
 * real combobox for keyboard users, exposes a listbox for screen readers, and
 * never silently assigns a default option. */
export function SearchablePicker({ value, options, onChange, ariaLabel, emptyLabel = "未设置", placeholder = "搜索或选择…", allowClear = true }: {
  value: string;
  options: PickerOption[];
  onChange: (value: string) => void;
  ariaLabel: string;
  emptyLabel?: string;
  placeholder?: string;
  allowClear?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.find(option => option.value === value);
  const selectedLabel = selected?.label ?? (value ? `已归档 · ${value}` : emptyLabel);
  const visible = useMemo(() => {
    const available = options.filter(option => !option.disabled);
    return query.trim() ? searchEntities(available, query, option => [option.label, option.group ?? ""], option => option.value) : available;
  }, [options, query]);
  const choices = useMemo(() => allowClear ? [{ value: "", label: emptyLabel }, ...visible] : visible, [allowClear, emptyLabel, visible]);
  const [activeIndex, setActiveIndex] = useState(0);
  const activeIndexRef = useRef(0);

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const selectedIndex = choices.findIndex(option => option.value === value);
    const next = selectedIndex >= 0 ? selectedIndex : 0;
    activeIndexRef.current = next;
    setActiveIndex(next);
  }, [open, choices, value]);

  const choose = (next: string) => {
    onChange(next);
    setQuery("");
    setOpen(false);
  };

  const moveActive = (delta: number) => {
    if (!choices.length) return;
    const next = (activeIndexRef.current + delta + choices.length) % choices.length;
    activeIndexRef.current = next;
    setActiveIndex(next);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      if (!open) setOpen(true);
      moveActive(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) setOpen(true);
      moveActive(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (open && choices[activeIndexRef.current]) choose(choices[activeIndexRef.current].value);
      else setOpen(true);
    }
  };

  return <div ref={rootRef} style={{ position: "relative", minWidth: 0, width: "100%" }}>
    <input
      role="combobox" aria-label={ariaLabel} aria-expanded={open} aria-controls={`${ariaLabel}-listbox`}
      value={open ? query : selectedLabel}
      placeholder={open ? placeholder : undefined}
      readOnly={!open}
      onFocus={() => setOpen(true)}
      onClick={() => setOpen(true)}
      onChange={event => { setQuery(event.target.value); setOpen(true); }}
      onKeyDown={handleKeyDown}
      style={{ width: "100%", fontFamily: "var(--font-sans)", fontSize: 11, color: C.textPrimary,
        background: C.cardDim, border: `1px solid ${open ? C.hairlineStr : C.hairline}`, borderRadius: 7, padding: "6px 8px", outline: "none", cursor: "pointer" }}
    />
    {open && <div id={`${ariaLabel}-listbox`} role="listbox" aria-label={`${ariaLabel}选项`} style={{ position: "absolute", zIndex: 115, top: "calc(100% + 5px)", left: 0, right: 0,
      maxHeight: 220, overflowY: "auto", padding: 5, borderRadius: 9, background: "rgba(8,13,18,0.97)", backdropFilter: "blur(20px)",
      border: `1px solid ${C.hairlineStr}`, boxShadow: "0 14px 34px rgba(2,3,5,0.48)" }}>
      {choices.map((option, index) => <PickerOptionButton key={option.value || "__empty"} label={option.label} group={option.group} selected={option.value === value} active={index === activeIndex} onClick={() => choose(option.value)} />)}
      {visible.length === 0 && <div style={{ padding: "8px 7px", color: C.textMuted, fontFamily: "var(--font-sans)", fontSize: 10 }}>没有匹配结果</div>}
    </div>}
  </div>;
}

function PickerOptionButton({ label, group, selected, active, onClick }: { label: string; group?: string; selected: boolean; active: boolean; onClick: () => void }) {
  return <button type="button" role="option" aria-selected={selected} onClick={onClick}
    style={{ display: "flex", alignItems: "baseline", gap: 5, width: "100%", border: "1px solid transparent", borderRadius: 6,
      padding: "6px 7px", textAlign: "left", background: active ? "rgba(186,200,204,0.18)" : selected ? "rgba(186,200,204,0.14)" : "transparent",
      color: selected ? C.textPrimary : C.textSec, fontFamily: "var(--font-sans)", fontSize: 11, cursor: "pointer" }}>
    <span>{label}</span>{group && <span style={{ color: C.textMuted, fontSize: 9 }}>· {group}</span>}
  </button>;
}
