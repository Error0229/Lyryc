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
  /** Width of the whole control (default w-56). */
  className?: string;
}

export function Slider({ value, onChange, min, max, step, label, format, disabled, className = "w-56" }: SliderProps) {
  return (
    <div className={`flex items-center gap-3 ${className}`}>
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
        <S.Thumb className="block size-4 rounded-full border-2 border-accent bg-surface shadow-1 transition-[transform,box-shadow] duration-(--dur-fast) ease-standard outline-none hover:scale-110 focus-visible:ring-4 focus-visible:ring-focus active:scale-95" />
      </S.Root>
      {format && <span className="w-14 text-right font-mono text-xs text-fg-muted tabular-nums">{format(value)}</span>}
    </div>
  );
}
