/**
 * The rects the backend hit-tests the cursor against while the strip is
 * locked (click-through): the Unlock button takes clicks there, the status
 * icon only learns it is hovered. One registry, so each element reports only
 * itself and the backend always gets the whole set.
 */
import { useLayoutEffect, type RefObject } from "react";
import type { HitRect } from "../lib/types";
import { backend } from "../services/ipc/backend";

const rects = new Map<string, HitRect>();
let queued = false;

function flush() {
  if (queued) return;
  queued = true;
  queueMicrotask(() => {
    queued = false;
    void backend.stripHits([...rects.values()]).catch(() => {});
  });
}

/** Report `ref`'s rect under `id` while mounted, and again whenever the window resizes. */
export function useHitRect(id: string, ref: RefObject<HTMLElement | null>, clickable = false, pad = 0) {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      rects.set(id, { id, x: r.x - pad, y: r.y - pad, w: r.width + 2 * pad, h: r.height + 2 * pad, clickable });
      flush();
    };
    measure();
    // Again once the parents have placed it (a parent's layout effect runs after
    // this one), and once whatever it rides in (a line entering) has settled.
    let frame = requestAnimationFrame(() => (frame = requestAnimationFrame(measure)));
    const settled = setTimeout(measure, 500);
    const ro = new ResizeObserver(measure);
    ro.observe(document.documentElement);
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(settled);
      ro.disconnect();
      rects.delete(id);
      flush();
    };
  }, [id, ref, clickable, pad]);
}
