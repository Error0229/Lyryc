import type { ComponentProps, ReactNode } from "react";
import { cx } from "../lib/cx";
import { Tooltip } from "./Tooltip";

type Tone = "ghost" | "glass";

const tones: Record<Tone, string> = {
  ghost: "text-fg-muted hover:bg-surface-muted hover:text-fg",
  glass: "text-glass-fg-muted hover:bg-glass-hover hover:text-glass-fg aria-pressed:text-glass-fg aria-pressed:bg-glass-hover",
};

export interface IconButtonProps extends Omit<ComponentProps<"button">, "children"> {
  /** Accessible name; also the tooltip. */
  label: string;
  icon: ReactNode;
  tone?: Tone;
  /**
   * Rich tooltip (default) or the native title. The overlay window is too
   * short for a floating tooltip, so it uses the native one.
   */
  tooltip?: boolean;
  tooltipSide?: "top" | "bottom" | "left" | "right";
}

export function IconButton({ label, icon, tone = "ghost", tooltip = true, tooltipSide, className, type = "button", ...rest }: IconButtonProps) {
  const button = (
    <button
      type={type}
      aria-label={label}
      title={tooltip ? undefined : label}
      className={cx(
        "inline-flex size-7 shrink-0 items-center justify-center rounded-md [&_svg]:size-4",
        "transition-[background-color,color,transform] duration-(--dur-fast) ease-standard active:scale-90",
        "disabled:pointer-events-none disabled:opacity-40",
        tones[tone],
        className,
      )}
      {...rest}
    >
      {icon}
    </button>
  );
  return tooltip ? (
    <Tooltip content={label} side={tooltipSide}>
      {button}
    </Tooltip>
  ) : (
    button
  );
}
