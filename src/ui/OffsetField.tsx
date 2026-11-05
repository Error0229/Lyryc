/**
 * A sync offset you can type: shows "+0.35 s"; click (or focus) and it is a
 * plain number to edit. Enter or leaving commits, Esc puts it back. Arrow keys
 * step 0.05 s (0.5 s with Shift) without opening the text up.
 */
import { useRef, useState, type KeyboardEvent } from "react";
import { cx } from "../lib/cx";
import { formatOffset, normalizeOffset, OFFSET_KEY_STEP, parseOffset } from "../lib/offset";

export interface OffsetFieldProps {
  value: number;
  onChange: (v: number) => void;
  label: string;
  /** Styles for where it sits: the settings window or the overlay's dark glass. */
  tone?: "surface" | "glass";
  className?: string;
}

const tones = {
  surface: "bg-surface-sunken text-fg-muted hover:text-fg focus:bg-surface focus:text-fg focus:ring-2 focus:ring-focus",
  glass: "bg-transparent text-glass-fg-muted hover:bg-glass-hover hover:text-glass-fg focus:bg-glass-hover focus:text-glass-fg focus:ring-1 focus:ring-brand-bright",
};

export function OffsetField({ value, onChange, label, tone = "surface", className }: OffsetFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const cancelled = useRef(false);

  const commit = () => {
    const v = draft === null || cancelled.current ? null : parseOffset(draft);
    cancelled.current = false;
    setDraft(null);
    if (v !== null && v !== value) onChange(v);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") e.currentTarget.blur();
    else if (e.key === "Escape") {
      e.stopPropagation(); // Esc here cancels the edit; it doesn't also lock the strip
      cancelled.current = true;
      e.currentTarget.blur();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const step = (e.shiftKey ? 10 : 1) * OFFSET_KEY_STEP * (e.key === "ArrowUp" ? 1 : -1);
      const next = normalizeOffset((draft === null ? value : (parseOffset(draft) ?? value)) + step);
      if (draft !== null) setDraft(next.toFixed(2));
      onChange(next);
    }
  };

  return (
    <input
      type="text"
      inputMode="decimal"
      spellCheck={false}
      aria-label={label}
      value={draft ?? formatOffset(value)}
      onFocus={(e) => {
        setDraft(normalizeOffset(value).toFixed(2));
        const el = e.currentTarget;
        requestAnimationFrame(() => el.select());
      }}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={onKeyDown}
      className={cx(
        "h-7 w-20 rounded-md px-1.5 text-center font-mono text-xs tabular-nums outline-none transition-colors duration-(--dur-fast)",
        tones[tone],
        className,
      )}
    />
  );
}
