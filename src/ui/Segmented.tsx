import { useId, type ReactNode } from "react";
import { ToggleGroup } from "radix-ui";
import { motion } from "motion/react";
import { spring } from "../design/motion";
import { Tooltip } from "./Tooltip";

export interface Segment<T extends string> {
  value: T;
  label: ReactNode;
  /** Accessible name when the label is an icon; shown in our tooltip. */
  title?: string;
}

export interface SegmentedProps<T extends string> {
  value: T;
  onChange: (v: T) => void;
  options: readonly Segment<T>[];
  label: string;
}

/** Single-choice pill group; the selection thumb slides between options. */
export function Segmented<T extends string>({ value, onChange, options, label }: SegmentedProps<T>) {
  const id = useId();
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => v && onChange(v as T)}
      aria-label={label}
      className="inline-flex h-8 items-center gap-0.5 rounded-md bg-surface-sunken p-0.5"
    >
      {options.map((o) => {
        const item = (
          <ToggleGroup.Item
            key={o.value}
            value={o.value}
            aria-label={o.title}
            className="relative inline-flex h-7 min-w-8 items-center justify-center rounded-[5px] px-2.5 text-xs font-medium text-fg-muted transition-colors duration-(--dur-fast) hover:text-fg data-[state=on]:text-fg [&_svg]:size-3.5"
          >
            {value === o.value && (
              <motion.span layoutId={id} transition={spring.snappy} className="absolute inset-0 rounded-[5px] bg-surface shadow-1" />
            )}
            <span className="relative inline-flex items-center gap-1.5">{o.label}</span>
          </ToggleGroup.Item>
        );
        return o.title ? (
          <Tooltip key={o.value} content={o.title}>
            {item}
          </Tooltip>
        ) : (
          item
        );
      })}
    </ToggleGroup.Root>
  );
}
