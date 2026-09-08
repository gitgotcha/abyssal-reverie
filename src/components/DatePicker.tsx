import { useEffect, useMemo, useRef, useState } from "react";
import { addLocalDays, isValidLocalDate, todayLocalDate } from "../domain/localDate";
import { C } from "../features/shared/palette";

const MONTH_NAMES = ["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"];
const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

function monthKey(year: number, month: number): string {
  return `${year}-${String(month + 1).padStart(2, "0")}`;
}

function parseDate(value: string): { year: number; month: number; day: number } | null {
  if (!isValidLocalDate(value)) return null;
  const [year, month, day] = value.split("-").map(Number);
  return { year, month: month - 1, day };
}

function dateValue(year: number, month: number, day: number): string {
  return `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function shiftMonth(year: number, month: number, delta: number): { year: number; month: number } {
  const shifted = new Date(year, month + delta, 1);
  return { year: shifted.getFullYear(), month: shifted.getMonth() };
}

/** A compact, keyboard-friendly calendar that keeps YYYY-MM-DD values local.
 * The text field remains editable for precise entry while the popover mirrors
 * the familiar Windows calendar affordance (month navigation, today, clear).
 */
export function DatePicker({ value, onChange, ariaLabel = "截止日期", disabled = false }: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const calendarRef = useRef<HTMLDivElement>(null);
  const lastPropValue = useRef(value);
  const [text, setText] = useState(value);
  const [open, setOpen] = useState(false);
  const [inputError, setInputError] = useState<string | null>(null);
  const parsedValue = parseDate(value);
  const today = todayLocalDate();
  const parsedToday = parseDate(today)!;
  const [month, setMonth] = useState(() => parsedValue ? { year: parsedValue.year, month: parsedValue.month } : { year: parsedToday.year, month: parsedToday.month });
  const [focusedDate, setFocusedDate] = useState(value || today);
  const focusedDateRef = useRef(focusedDate);

  // Sync external edits (task switching, reset, or a saved value) without
  // overwriting an in-progress partial date typed into the field.
  useEffect(() => {
    if (value !== lastPropValue.current) {
      lastPropValue.current = value;
      setText(value);
      const next = parseDate(value);
      if (next) setMonth({ year: next.year, month: next.month });
    }
  }, [value]);

  useEffect(() => {
    if (!open) return;
    const initial = value || today;
    focusedDateRef.current = initial;
    setFocusedDate(current => current || initial);
    window.requestAnimationFrame(() => calendarRef.current?.focus());
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const days = useMemo(() => {
    const first = new Date(month.year, month.month, 1);
    // JS Sunday=0; the product calendar starts on Monday.
    const leading = (first.getDay() + 6) % 7;
    const start = dateValue(month.year, month.month, 1);
    return Array.from({ length: 42 }, (_, index) => addLocalDays(start, index - leading));
  }, [month]);

  const commit = (next: string) => {
    if (next !== "" && !isValidLocalDate(next)) return;
    lastPropValue.current = next;
    setText(next);
    setInputError(null);
    onChange(next);
  };

  const showCalendar = () => {
    const initial = value || today;
    focusedDateRef.current = initial;
    setFocusedDate(initial);
    setOpen(true);
  };

  const moveFocusedDate = (delta: number) => {
    const next = addLocalDays(focusedDateRef.current || today, delta);
    focusedDateRef.current = next;
    setFocusedDate(next);
    const parsed = parseDate(next);
    if (parsed && (parsed.year !== month.year || parsed.month !== month.month)) {
      setMonth({ year: parsed.year, month: parsed.month });
    }
  };

  const handleCalendarKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const deltas: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    if (event.key in deltas) {
      event.preventDefault();
      moveFocusedDate(deltas[event.key]);
    } else if (event.key === "Enter") {
      event.preventDefault();
      commit(focusedDateRef.current || today);
      setOpen(false);
    }
  };

  const handleTextChange = (next: string) => {
    setText(next);
    if (next === "" || isValidLocalDate(next)) {
      lastPropValue.current = next;
      setInputError(null);
      onChange(next);
      const parsed = parseDate(next);
      if (parsed) setMonth({ year: parsed.year, month: parsed.month });
    } else if (next.length >= 10) {
      setInputError("请输入有效日期（YYYY-MM-DD）");
    }
  };

  const tomorrow = addLocalDays(today, 1);
  const nextWeek = addLocalDays(today, 7);

  return (
    <div ref={rootRef} style={{ position: "relative", minWidth: 0, width: "100%" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <input
          value={text}
          onChange={event => handleTextChange(event.target.value)}
          onFocus={() => {
            const next = parseDate(text);
            if (next) setMonth({ year: next.year, month: next.month });
          }}
          onKeyDown={event => {
            if (event.key === "ArrowDown" || event.key === "Enter") {
              event.preventDefault();
              showCalendar();
            }
          }}
          placeholder="选择日期"
          aria-label={ariaLabel}
          aria-invalid={inputError ? "true" : undefined}
          disabled={disabled}
          style={{
            flex: 1, minWidth: 0, fontFamily: "var(--font-mono)", fontSize: 11,
            color: C.textPrimary, background: C.cardDim, border: `1px solid ${inputError ? "rgba(231,164,145,0.55)" : C.hairline}`,
            borderRadius: 7, padding: "6px 8px", outline: "none",
          }}
        />
        <button type="button" onClick={() => open ? setOpen(false) : showCalendar()} disabled={disabled}
          aria-label={`${ariaLabel}日历`} aria-expanded={open} aria-haspopup="dialog"
          style={{ width: 29, height: 29, flexShrink: 0, borderRadius: 7, border: `1px solid ${C.hairline}`,
            background: open ? "rgba(27,37,44,0.72)" : C.cardDim, color: C.textMuted,
            cursor: "pointer", fontSize: 14, lineHeight: 1 }}>
          ▦
        </button>
      </div>
      {inputError && <div role="alert" style={{ color: "rgba(231,164,145,0.95)", fontFamily: "var(--font-sans)", fontSize: 9, margin: "3px 2px 0" }}>{inputError}</div>}
      {open && (
        <div ref={calendarRef} role="dialog" tabIndex={-1} onKeyDown={handleCalendarKeyDown} aria-label={`${ariaLabel}选择器`} style={{ position: "absolute", zIndex: 120, right: 0, top: "calc(100% + 7px)", width: 254,
          padding: 10, borderRadius: 12, background: "rgba(8,13,18,0.96)", backdropFilter: "blur(22px) saturate(1.08)",
          WebkitBackdropFilter: "blur(22px) saturate(1.08)", border: `1px solid ${C.hairlineStr}`, boxShadow: "0 16px 42px rgba(2,3,5,0.52)" }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
            <button type="button" onClick={() => setMonth(current => shiftMonth(current.year, current.month, -1))}
              aria-label="上个月" style={navButtonStyle}>‹</button>
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <select aria-label="选择年份" value={month.year}
                onChange={event => setMonth(current => ({ ...current, year: Number(event.target.value) }))}
                style={calendarSelectStyle}>{Array.from({ length: 101 }, (_, index) => month.year - 50 + index).map(year => <option key={year} value={year}>{year}年</option>)}</select>
              <select aria-label="选择月份" value={month.month}
                onChange={event => setMonth(current => ({ ...current, month: Number(event.target.value) }))}
                style={calendarSelectStyle}>{MONTH_NAMES.map((name, index) => <option key={name} value={index}>{name}</option>)}</select>
            </div>
            <button type="button" onClick={() => setMonth(current => shiftMonth(current.year, current.month, 1))}
              aria-label="下个月" style={navButtonStyle}>›</button>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 2, marginBottom: 3 }}>
            {WEEKDAYS.map(day => <span key={day} style={{ color: C.textMuted, fontFamily: "var(--font-sans)", fontSize: 9, textAlign: "center", padding: "2px 0" }}>{day}</span>)}
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 2 }}>
            {days.map(day => {
              const current = parseDate(day)!;
              const inMonth = current.month === month.month && current.year === month.year;
              const selected = day === value;
              const isToday = day === today;
              return <button key={day} type="button" onClick={() => { commit(day); setOpen(false); }}
                aria-label={day} aria-pressed={selected}
                style={{ height: 27, borderRadius: 6, border: selected ? `1px solid ${C.hairlineStr}` : day === focusedDate ? `1px solid ${C.hairline}` : "1px solid transparent",
                  background: selected ? "rgba(186,200,204,0.18)" : isToday ? "rgba(158,173,178,0.09)" : "transparent",
                  color: inMonth ? (selected ? C.textPrimary : C.textSec) : "rgba(190,202,205,0.24)",
                  fontFamily: "var(--font-mono)", fontSize: 10, cursor: "pointer", position: "relative" }}>
                {current.day}
                {isToday && <span aria-hidden="true" style={{ position: "absolute", bottom: 2, left: "50%", width: 3, height: 3, borderRadius: "50%", transform: "translateX(-50%)", background: C.silver }} />}
              </button>;
            })}
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 8, paddingTop: 7, borderTop: `1px solid ${C.hairline}` }}>
            <button type="button" onClick={() => { commit(today); setOpen(false); }} style={footerButtonStyle}>今天</button>
            <button type="button" onClick={() => { commit(tomorrow); setOpen(false); }} style={footerButtonStyle}>明天</button>
            <button type="button" onClick={() => { commit(nextWeek); setOpen(false); }} style={footerButtonStyle}>一周后</button>
            <button type="button" onClick={() => { commit(""); setOpen(false); }} style={footerButtonStyle}>清除</button>
          </div>
        </div>
      )}
    </div>
  );
}

const navButtonStyle = {
  width: 25, height: 25, borderRadius: 6, border: "1px solid rgba(215,228,230,0.08)",
  background: "rgba(27,37,44,0.32)", color: C.textMuted, cursor: "pointer", fontSize: 17, lineHeight: 1,
} as const;

const footerButtonStyle = {
  border: "none", background: "transparent", color: C.textMuted, cursor: "pointer",
  fontFamily: "var(--font-sans)", fontSize: 10, padding: "2px 4px",
} as const;

const calendarSelectStyle = {
  appearance: "none", border: "1px solid transparent", borderRadius: 5, background: "transparent",
  color: C.textPrimary, fontFamily: "var(--font-sans)", fontSize: 11, padding: "2px 3px", cursor: "pointer",
} as const;
