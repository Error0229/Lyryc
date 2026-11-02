import type { ComponentProps, ReactNode } from "react";
import { cx } from "../lib/cx";

/** A settings row: label and help on the left, control on the right. */
export function Row({ label, hint, children, htmlFor }: { label: ReactNode; hint?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="flex min-h-14 items-center justify-between gap-6 px-4 py-3">
      <div className="min-w-0">
        <label htmlFor={htmlFor} className="block text-sm font-medium text-fg">
          {label}
        </label>
        {hint && <p className="mt-0.5 text-xs text-fg-muted">{hint}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/** A stacked form field: label above the control, for dialogs and forms. */
export function Field({ label, hint, error, children, htmlFor }: { label: ReactNode; hint?: ReactNode; error?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="grid gap-1.5">
      <label htmlFor={htmlFor} className="text-xs font-medium text-fg-muted">
        {label}
      </label>
      {children}
      {error ? <p className="text-xs text-danger">{error}</p> : hint && <p className="text-xs text-fg-subtle">{hint}</p>}
    </div>
  );
}

const inputBase =
  "w-full rounded-md bg-surface px-2.5 text-sm text-fg shadow-1 placeholder:text-fg-subtle transition-shadow duration-(--dur-fast) focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-50";

export function TextInput({ className, ...rest }: ComponentProps<"input">) {
  return <input className={cx(inputBase, "h-8", className)} {...rest} />;
}

export function TextArea({ className, ...rest }: ComponentProps<"textarea">) {
  return <textarea className={cx(inputBase, "min-h-20 resize-y py-2", className)} {...rest} />;
}
