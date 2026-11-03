import type { ComponentProps } from "react";
import { cx } from "../lib/cx";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:bg-accent-hover",
  secondary: "bg-surface text-fg shadow-1 hover:bg-surface-muted",
  ghost: "text-fg-muted hover:bg-surface-muted hover:text-fg",
  danger: "bg-danger-soft text-danger hover:brightness-95",
};

const sizes: Record<Size, string> = {
  sm: "h-7 px-2.5 text-xs gap-1.5",
  md: "h-8 px-3 text-sm gap-2",
};

export interface ButtonProps extends ComponentProps<"button"> {
  variant?: Variant;
  size?: Size;
}

export function Button({ variant = "secondary", size = "md", className, type = "button", ...rest }: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        "inline-flex shrink-0 select-none items-center justify-center rounded-md font-medium whitespace-nowrap",
        "transition-[background-color,color,transform,filter] duration-(--dur-fast) ease-standard",
        "active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-3.5",
        variants[variant],
        sizes[size],
        className,
      )}
      {...rest}
    />
  );
}
