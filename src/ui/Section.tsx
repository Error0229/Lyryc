import type { ReactNode } from "react";
import { cx } from "../lib/cx";

/** Raised surface. Rows inside are separated by hairlines. */
export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cx("divide-y divide-line rounded-lg bg-surface shadow-1", className)}>{children}</div>;
}

/** A titled group of settings. */
export function Section({ title, description, action, children }: { title: string; description?: ReactNode; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="grid gap-3">
      <header className="flex items-end justify-between gap-4 px-1">
        <div>
          <h2 className="text-xs font-semibold tracking-wide text-fg-subtle uppercase">{title}</h2>
          {description && <p className="mt-1 text-xs text-fg-muted">{description}</p>}
        </div>
        {action}
      </header>
      <Card>{children}</Card>
    </section>
  );
}
