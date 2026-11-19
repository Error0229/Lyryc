/**
 * Minimal toast queue. `toast()` can be called from anywhere; <Toaster/> renders
 * the stack. Toasts slide/fade in from below and collapse out.
 */
import { useRef, type RefObject } from "react";
import { create } from "zustand";
import { AnimatePresence, motion } from "motion/react";
import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { spring, fade } from "../design/motion";
import { cx } from "../lib/cx";

export type ToastTone = "neutral" | "success" | "danger";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastInput {
  title: string;
  description?: string;
  tone?: ToastTone;
  /** What the user can do about it; any of them dismisses the toast. */
  actions?: ToastAction[];
  /** ms; 0 keeps it until dismissed. */
  duration?: number;
  /** Toasts about one thing share a slot: a new one replaces the one before it. */
  slot?: string;
}

interface ToastItem extends ToastInput {
  id: number;
}

const useToasts = create<{ items: ToastItem[] }>()(() => ({ items: [] }));
let nextId = 0;

export function dismiss(id: number) {
  useToasts.setState((s) => ({ items: s.items.filter((t) => t.id !== id) }));
}

/** Dismiss whatever is in `slot`: what it said no longer holds. */
export function dismissSlot(slot: string) {
  useToasts.setState((s) => ({ items: s.items.filter((t) => t.slot !== slot) }));
}

export function toast(input: ToastInput): number {
  const id = ++nextId;
  // Same message twice in a row is noise; replace instead of stacking.
  const replaced = (t: ToastItem) => t.title === input.title || (!!input.slot && t.slot === input.slot);
  useToasts.setState((s) => ({ items: [...s.items.filter((t) => !replaced(t)), { ...input, id }].slice(-3) }));
  const ms = input.duration ?? 5000;
  if (ms > 0) setTimeout(() => dismiss(id), ms);
  return id;
}

const icons = {
  neutral: <Info className="text-glass-fg-muted" />,
  success: <CircleCheck className="text-success" />,
  danger: <CircleAlert className="text-danger" />,
};

const placements = {
  /** Settings window: stack in the corner. */
  corner: "right-3 bottom-3 flex-col-reverse",
  /** Overlay strip: one toast, left edge, vertically centred (the window is short). */
  strip: "top-1/2 left-3 -translate-y-1/2",
};

type Placement = keyof typeof placements;
/** Called with each toast's element, as a hook: the overlay reports its rect so it takes clicks while locked. */
type UseHit = (ref: RefObject<HTMLElement | null>) => void;
const noHit: UseHit = () => {};

export function Toaster({ placement = "corner", useHit = noHit }: { placement?: Placement; useHit?: UseHit }) {
  const all = useToasts((s) => s.items);
  const strip = placement === "strip";
  const items = strip ? all.slice(-1) : all;
  return (
    <div aria-live="polite" className={cx("pointer-events-none fixed z-(--z-toast) flex flex-col gap-2", strip ? "max-w-[calc(100vw-1.5rem)]" : "w-80", placements[placement])}>
      <AnimatePresence initial={false}>
        {items.map((t) => (
          <ToastCard key={t.id} toast={t} strip={strip} useHit={useHit} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function ToastCard({ toast: t, strip, useHit }: { toast: ToastItem; strip: boolean; useHit: UseHit }) {
  const ref = useRef<HTMLDivElement>(null);
  useHit(ref);
  return (
    <motion.div
      ref={ref}
      layout
      initial={{ opacity: 0, y: 12, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1, transition: spring.gentle }}
      exit={{ opacity: 0, scale: 0.98, transition: fade("fast") }}
      role="status"
      className={cx(
        "pointer-events-auto flex gap-2.5 rounded-lg border border-glass-line bg-glass text-glass-fg shadow-3 backdrop-blur-xl [&_svg]:size-4",
        strip ? "items-center px-3 py-2" : "items-start p-3",
      )}
    >
      <span className={cx("shrink-0", !strip && "mt-px")}>{icons[t.tone ?? "neutral"]}</span>
      <div className={cx("min-w-0 flex-1", strip && "flex items-center gap-3")}>
        <p className="truncate text-sm font-medium">{t.title}</p>
        {t.description && !strip && <p className="mt-0.5 text-xs text-glass-fg-muted">{t.description}</p>}
        {t.actions && t.actions.length > 0 && (
          <div className={cx("flex shrink-0 gap-3", !strip && "mt-2")}>
            {t.actions.map((a) => (
              <button
                key={a.label}
                type="button"
                onClick={() => {
                  a.onClick();
                  dismiss(t.id);
                }}
                className="shrink-0 rounded-sm text-xs font-semibold whitespace-nowrap text-glass-fg underline decoration-glass-fg-muted underline-offset-4 hover:decoration-glass-fg"
              >
                {a.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <button type="button" aria-label="Dismiss" onClick={() => dismiss(t.id)} className="shrink-0 text-glass-fg-muted hover:text-glass-fg">
        <X />
      </button>
    </motion.div>
  );
}
