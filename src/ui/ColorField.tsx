import { useEffect, useState } from "react";
import { splitAlpha, toHex, withAlpha } from "../lib/color";

/**
 * Swatch (native picker) plus hex text. Accepts #rrggbb or #rrggbbaa; picking
 * from the swatch keeps the existing alpha.
 */
export function ColorField({ value, onChange, label }: { value: string; onChange: (v: string) => void; label: string }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const { rgb, alpha } = splitAlpha(value);

  const commit = () => {
    const hex = toHex(draft);
    if (hex) onChange(hex);
    else setDraft(value);
  };

  return (
    <div className="flex h-8 w-36 items-center gap-2 rounded-md bg-surface pr-2 pl-1 shadow-1">
      <label className="relative size-6 shrink-0 cursor-pointer overflow-hidden rounded-sm shadow-[inset_0_0_0_1px_var(--c-line-strong)]">
        <span className="absolute inset-0" style={{ background: value }} />
        <input
          type="color"
          aria-label={`${label} colour`}
          value={rgb}
          onChange={(e) => onChange(withAlpha(e.target.value, alpha))}
          className="absolute inset-0 cursor-pointer opacity-0"
        />
      </label>
      <input
        aria-label={`${label} hex`}
        value={draft}
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
        className="w-full bg-transparent font-mono text-xs text-fg uppercase focus:outline-none"
      />
    </div>
  );
}
