import { ToggleGroup } from "radix-ui";

export interface Chip {
  value: string;
  label: string;
}

export interface ChipsProps {
  value: readonly string[];
  onChange: (v: string[]) => void;
  options: readonly Chip[];
  label: string;
}

/** Multi-choice toggle chips that wrap; selected chips take the soft accent. */
export function Chips({ value, onChange, options, label }: ChipsProps) {
  return (
    <ToggleGroup.Root
      type="multiple"
      value={[...value]}
      onValueChange={onChange}
      aria-label={label}
      className="flex flex-wrap gap-1.5"
    >
      {options.map((o) => (
        <ToggleGroup.Item
          key={o.value}
          value={o.value}
          className="inline-flex h-7 items-center rounded-full bg-surface-sunken px-3 text-xs font-medium text-fg-muted transition-colors duration-(--dur-fast) hover:text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus data-[state=on]:bg-accent-soft data-[state=on]:text-accent"
        >
          {o.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}
