import { Select as S } from "radix-ui";
import { Check, ChevronDown } from "lucide-react";
import { cx } from "../lib/cx";

export interface Option<T extends string> {
  value: T;
  label: string;
  hint?: string;
}

export interface SelectProps<T extends string> {
  value: T | undefined;
  onChange: (v: T) => void;
  options: readonly Option<T>[];
  label: string;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}

export function Select<T extends string>({ value, onChange, options, label, placeholder, disabled, className }: SelectProps<T>) {
  return (
    <S.Root value={value} onValueChange={(v) => onChange(v as T)} disabled={disabled}>
      <S.Trigger
        aria-label={label}
        className={cx(
          "inline-flex h-8 w-56 items-center justify-between gap-2 rounded-md bg-surface px-2.5 text-sm text-fg shadow-1",
          "transition-colors duration-(--dur-fast) hover:bg-surface-muted disabled:opacity-50 data-placeholder:text-fg-subtle",
          className,
        )}
      >
        <span className="truncate">
          <S.Value placeholder={placeholder ?? "Choose…"} />
        </span>
        <S.Icon>
          <ChevronDown className="size-3.5 text-fg-subtle" />
        </S.Icon>
      </S.Trigger>
      <S.Portal>
        <S.Content
          position="popper"
          sideOffset={4}
          className="z-(--z-popover) max-h-72 min-w-(--radix-select-trigger-width) animate-pop overflow-hidden rounded-md bg-surface p-1 shadow-3"
        >
          <S.Viewport>
            {options.map((o) => (
              <S.Item
                key={o.value}
                value={o.value}
                className="relative flex h-8 cursor-default items-center gap-2 rounded-sm pr-8 pl-2 text-sm text-fg outline-none select-none data-highlighted:bg-surface-muted"
              >
                <S.ItemText>{o.label}</S.ItemText>
                {o.hint && <span className="text-xs text-fg-subtle">{o.hint}</span>}
                <S.ItemIndicator className="absolute right-2">
                  <Check className="size-3.5 text-accent" />
                </S.ItemIndicator>
              </S.Item>
            ))}
          </S.Viewport>
        </S.Content>
      </S.Portal>
    </S.Root>
  );
}
