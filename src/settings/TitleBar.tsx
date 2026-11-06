/**
 * The settings window's own title bar (the window has no native chrome): the
 * seal and name as the drag region, minimize and close. Double-clicking the
 * drag region maximizes, as a native one does.
 */
import type { ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";
import seal from "../../src-tauri/icons/tray/48.png";
import { inTauri } from "../services/ipc/backend";
import { Tooltip } from "../ui/Tooltip";

const win = () => (inTauri ? getCurrentWindow() : null);

function WindowButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip content={label} side="bottom">
      <button
        type="button"
        aria-label={label}
        onClick={onClick}
        className="grid h-full w-11 place-items-center text-fg-muted transition-colors duration-(--dur-fast) hover:bg-brand hover:text-brand-fg focus-visible:bg-brand focus-visible:text-brand-fg focus-visible:outline-none active:bg-brand-pressed [&_svg]:size-4"
      >
        {children}
      </button>
    </Tooltip>
  );
}

export function TitleBar() {
  return (
    // One label in the sidebar's text style, on the sidebar's grid: the seal
    // centred over the nav icons (x 20-40), the words starting where the nav
    // labels do (x 48).
    <header data-tauri-drag-region className="flex h-10 shrink-0 items-center gap-2 border-b border-line bg-surface-sunken/60 pl-5 select-none">
      <img src={seal} alt="" draggable={false} className="pointer-events-none size-5 rounded-[5px]" />
      <span data-tauri-drag-region className="text-sm font-medium text-fg-muted">
        Lyryc Settings
      </span>
      <div className="ml-auto flex h-full">
        <WindowButton label="Minimize" onClick={() => void win()?.minimize()}>
          <Minus />
        </WindowButton>
        <WindowButton label="Close" onClick={() => void win()?.close()}>
          <X />
        </WindowButton>
      </div>
    </header>
  );
}
