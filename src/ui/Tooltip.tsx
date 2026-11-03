import type { ReactNode } from "react";
import { Tooltip as T } from "radix-ui";

export const TooltipProvider = ({ children }: { children: ReactNode }) => (
  <T.Provider delayDuration={500} skipDelayDuration={200}>
    {children}
  </T.Provider>
);

export function Tooltip({ content, side = "top", children }: { content: ReactNode; side?: "top" | "bottom" | "left" | "right"; children: ReactNode }) {
  return (
    <T.Root>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          side={side}
          sideOffset={6}
          className="z-(--z-tooltip) animate-pop rounded-sm bg-glass px-2 py-1 text-2xs font-medium text-glass-fg shadow-2 backdrop-blur-md"
        >
          {content}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}
