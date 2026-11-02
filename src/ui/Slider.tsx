import { Slider as S } from "radix-ui";

export interface SliderProps {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step: number;
  label: string;
  /** Shown to the right of the track. */
  format?: (v: number) => string;
  disabled?: boolean;
}

export function Slider({ value, onChange, min, max, step, label, format, disabled }: SliderProps) {
  return (
    <div className="flex w-56 items-center gap-3">
      <S.Root
        className="relative flex h-5 flex-1 touch-none items-center select-none data-disabled:opacity-50"
        value={[value]}
        onValueChange={([v]) => onChange(v)}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        aria-label={label}
      >
        <S.Track className="relative h-1 flex-1 overflow-hidden rounded-full bg-line">
          <S.Range className="absolute h-full bg-accent" />
        </S.Track>
        <S.Thumb className="block size-4 rounded-full border border-line-strong bg-surface shadow-1 transition-transform duration-(--dur-fast) ease-standard hover:scale-110 active:scale-95" />
      </S.Root>
      {format && <span className="w-14 text-right font-mono text-xs text-fg-muted tabular-nums">{format(value)}</span>}
    </div>
  );
}
