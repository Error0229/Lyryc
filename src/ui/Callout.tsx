import type { ReactNode } from "react";
import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import { cx } from "../lib/cx";

type Tone = "info" | "warning" | "danger";

const tones: Record<Tone, { box: string; icon: ReactNode }> = {
  info: { box: "bg-accent-soft", icon: <Info className="text-accent" /> },
  warning: { box: "bg-surface-muted", icon: <TriangleAlert className="text-warning" /> },
  danger: { box: "bg-danger-soft", icon: <CircleAlert className="text-danger" /> },
};

/** Inline notice with an optional action, e.g. "Set up a provider". */
export function Callout({ tone = "info", title, children, action }: { tone?: Tone; title: string; children?: ReactNode; action?: ReactNode }) {
  const t = tones[tone];
  return (
    <div role={tone === "danger" ? "alert" : "note"} className={cx("flex items-start gap-3 rounded-lg p-3.5 [&>svg]:size-4", t.box)}>
      <span className="mt-0.5 shrink-0 [&_svg]:size-4">{t.icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-fg">{title}</p>
        {children && <div className="mt-0.5 text-xs text-fg-muted">{children}</div>}
      </div>
      {action && <div className="shrink-0 self-center">{action}</div>}
    </div>
  );
}
