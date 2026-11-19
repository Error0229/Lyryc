/**
 * Pick one font from a long list: a button showing the chosen font in
 * itself, opening a list to search (each font shown in itself). Typing
 * narrows it; arrows move, Enter picks, Esc closes. Like a theme picker, the
 * highlighted font is tried out live (`onPreview`) as you move through the
 * list, by arrow, pointer or search: Enter or a click keeps it, Esc or a
 * click away puts back the font it opened on.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Popover } from "radix-ui";
import { Check, ChevronDown, Search } from "lucide-react";
import { cx } from "../lib/cx";

export interface FontChoice {
  /** The CSS font-family value it sets (and is shown in). */
  value: string;
  label: string;
}

export interface FontPickerProps {
  label: string;
  value: string | undefined;
  options: readonly FontChoice[];
  onChange: (value: string) => void;
  /** Shown when `value` is none of the options (or unset). */
  placeholder: string;
  /** Open as soon as it appears: the font is what's being asked for. */
  defaultOpen?: boolean;
  /**
   * Try a font out without choosing it: the highlighted one, once the user
   * moves through the list; on closing without a choice, the font it opened
   * on (undefined: none).
   */
  onPreview?: (value: string | undefined) => void;
}

/** The highlight rests this long before its font is tried: sweeping past fonts doesn't flash each. */
const PREVIEW_DELAY_MS = 90;

export function FontPicker({ label, value, options, onChange, placeholder, defaultOpen = false, onPreview }: FontPickerProps) {
  const [open, setOpen] = useState(defaultOpen);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  // The font this opening started on, whether the user has moved yet (opening
  // tries nothing), and the latest onPreview (a timer may outlive a render).
  const original = useRef(value);
  const touched = useRef(false);
  const preview = useRef(onPreview);
  preview.current = onPreview;
  // The trigger shows the font in use (one being tried, too); the list ticks the one it opened on.
  const chosen = options.find((o) => o.value === value);
  const kept = open ? original.current : value;

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? options.filter((o) => o.label.toLowerCase().includes(q)) : options;
  }, [options, query]);

  // Each opening starts from the whole list, on the chosen font.
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
  }, [open]);
  // Try the highlighted font, once the highlight rests.
  useEffect(() => {
    const o = shown[active];
    if (!open || !touched.current || !o) return;
    const t = setTimeout(() => preview.current?.(o.value), PREVIEW_DELAY_MS);
    return () => clearTimeout(t);
  }, [open, active, shown]);
  // Gone while trying a font (the page changed): put the old one back.
  useEffect(
    () => () => {
      if (touched.current) preview.current?.(original.current);
    },
    [],
  );
  // A block, not an expression: newer WebView2 returns a Promise from scrollIntoView, and an effect may return only its cleanup.
  useEffect(() => {
    list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active, open, shown]);

  const choose = (o: FontChoice | undefined) => {
    if (!o) return;
    touched.current = false;
    onChange(o.value);
    setOpen(false);
  };
  // Closed without a choice: whatever was tried goes, the old font comes back.
  const openChange = (next: boolean) => {
    if (next) original.current = value;
    else if (touched.current) preview.current?.(original.current);
    touched.current = false;
    setOpen(next);
  };
  const highlight = (i: number) => {
    touched.current = true;
    setActive(Math.min(shown.length - 1, Math.max(0, i)));
  };

  return (
    <Popover.Root open={open} onOpenChange={openChange}>
      <Popover.Trigger
        aria-label={label}
        className="inline-flex h-8 w-56 items-center justify-between gap-2 rounded-md bg-surface px-2.5 text-sm text-fg shadow-1 transition-colors duration-(--dur-fast) hover:bg-surface-muted"
      >
        <span className={cx("truncate", !chosen && "text-fg-subtle")} style={chosen && { fontFamily: chosen.value }}>
          {chosen?.label ?? placeholder}
        </span>
        <ChevronDown className="size-3.5 shrink-0 text-fg-subtle" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" sideOffset={4} className="z-(--z-popover) grid w-72 animate-pop gap-1 rounded-md bg-surface p-1 shadow-3">
          <div className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" />
            <input
              aria-label={`Search ${label.toLowerCase()}`}
              placeholder={`Search ${options.length} fonts`}
              value={query}
              onChange={(e) => (setQuery(e.target.value), highlight(0))}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") (e.preventDefault(), highlight(active + 1));
                if (e.key === "ArrowUp") (e.preventDefault(), highlight(active - 1));
                if (e.key === "Enter") (e.preventDefault(), choose(shown[active]));
              }}
              className="h-8 w-full rounded-sm bg-surface-sunken pr-2 pl-8 text-sm text-fg outline-none placeholder:text-fg-subtle"
            />
          </div>
          <div ref={list} role="listbox" aria-label={label} className="max-h-72 overflow-y-auto">
            {shown.length === 0 && <p className="px-2 py-3 text-center text-xs text-fg-muted">No font matches.</p>}
            {shown.map((o, i) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === kept}
                data-index={i}
                onMouseMove={() => i !== active && highlight(i)}
                onClick={() => choose(o)}
                className={cx("relative flex h-8 w-full items-center rounded-sm pr-8 pl-2 text-left text-sm text-fg outline-none", i === active && "bg-surface-muted")}
              >
                <span className="truncate" style={{ fontFamily: o.value }}>
                  {o.label}
                </span>
                {o.value === kept && <Check className="absolute right-2 size-3.5 text-accent" />}
              </button>
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
