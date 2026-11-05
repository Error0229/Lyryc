import type { ComponentProps, ReactNode } from "react";
import { cx } from "../lib/cx";
import { Tooltip } from "./Tooltip";

type Tone = "ghost" | "glass" | "brand";

const tones: Record<Tone, string> = {
  ghost: "text-fg-muted hover:bg-surface-muted hover:text-fg",
  glass: "text-glass-fg-muted hover:bg-glass-hover hover:text-glass-fg aria-pressed:text-glass-fg aria-pressed:bg-glass-hover",
  /** The strip's own action (Lock / Unlock): the brand fill. */
  brand: "bg-brand text-brand-fg hover:bg-brand-hover active:bg-brand-pressed",
};

export interface IconButtonProps extends Omit<ComponentProps<"button">, "children"> {
  /** Accessible name; also the tooltip. */
  label: string;
  icon: ReactNode;
  tone?: Tone;
  /**
   * Our tooltip (default), none, or "external": the caller shows the name
   * itself (the overlay's shared name tip, which fits a short strip). Never
   * the native title tooltip.
   */
  tooltip?: boolean | "external";
  tooltipSide?: "top" | "bottom" | "left" | "right";
  /** A circle, for buttons inside a pill: the same curve as their container. */
  round?: boolean;
}

export function IconButton({ label, icon, tone = "ghost", tooltip = true, tooltipSide, round, className, type = "button", ...rest }: IconButtonProps) {
  const button = (
    <button
      type={type}
      aria-label={label}
      className={cx(
        "inline-flex size-7 shrink-0 items-center justify-center [&_svg]:size-4",
        round ? "rounded-full" : "rounded-md",
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
  return tooltip === true ? (
    <Tooltip content={label} side={tooltipSide}>
      {button}
    </Tooltip>
  ) : (
    button
  );
}
