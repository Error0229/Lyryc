/**
 * The tray icon's right-click menu, in our own window (tray.rs creates it
 * hidden and opens it at the cursor). Styled like the settings window, in its
 * theme. Keyboard: arrows move, Enter chooses, Esc closes. Losing focus hides it.
 */
import { useEffect, useLayoutEffect, useRef, useState, type ComponentType, type KeyboardEvent } from "react";
import { Eye, EyeOff, Lock, Move, Power, RotateCcw, Settings2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import seal from "../../src-tauri/icons/tray/48.png";
import { cx } from "../lib/cx";
import { inTauri } from "../services/ipc/backend";
import { useThemeAttribute } from "../state/theme";

interface MenuState {
  visible: boolean;
  locked: boolean;
}

type Item = { id: string; label: string; icon: ComponentType<{ className?: string }>; keys?: string } | "line";

function items({ visible, locked }: MenuState): Item[] {
  return [
    { id: "visible", label: visible ? "Hide lyrics" : "Show lyrics", icon: visible ? EyeOff : Eye, keys: "Ctrl+Shift+M" },
    { id: "locked", label: locked ? "Move lyrics" : "Lock lyrics", icon: locked ? Move : Lock, keys: "Ctrl+Shift+D" },
    { id: "reset", label: "Reset position", icon: RotateCcw },
    "line",
    { id: "settings", label: "Settings…", icon: Settings2 },
    "line",
    { id: "quit", label: "Quit Lyryc", icon: Power },
  ];
}

const MENU_WIDTH = 240;

const act = (id: string) => (inTauri ? invoke("tray_menu_act", { id }) : Promise.resolve(console.info("[tray] would run", id)));

/** The backend's state for the items, re-read every time the menu opens. */
function useMenuState(): MenuState {
  const [state, setState] = useState<MenuState>({ visible: true, locked: true });
  useEffect(() => {
    if (!inTauri) return;
    void invoke<MenuState>("tray_menu_state").then(setState);
    const off = listen<MenuState>("tray-menu-open", (e) => setState(e.payload));
    return () => void off.then((f) => f());
  }, []);
  return state;
}

/** The window is exactly as tall as the menu (it opens hidden, already sized). */
function useFitWindow(ref: React.RefObject<HTMLElement | null>) {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !inTauri) return;
    const fit = () =>
      void import("@tauri-apps/api/window").then(({ getCurrentWindow, LogicalSize }) =>
        getCurrentWindow().setSize(new LogicalSize(MENU_WIDTH, Math.ceil(el.getBoundingClientRect().height))),
      );
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
}

export function TrayMenu() {
  useThemeAttribute();
  const state = useMenuState();
  const ref = useRef<HTMLDivElement>(null);
  useFitWindow(ref);

  // Each opening starts fresh: the menu takes the keyboard, nothing highlighted yet.
  useEffect(() => {
    if (!inTauri) return;
    const off = listen("tray-menu-open", () => ref.current?.focus());
    return () => void off.then((f) => f());
  }, []);

  const onKeyDown = (e: KeyboardEvent) => {
    const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") void act("close");
    else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      buttons[(at + step + buttons.length) % buttons.length]?.focus();
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Lyryc"
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="bg-surface p-1.5 outline-none select-none"
      style={{ width: MENU_WIDTH }}
    >
      <div className="flex items-center gap-2 px-2.5 pt-1 pb-2">
        <img src={seal} alt="" draggable={false} className="size-4 rounded-[4px]" />
        <span className="text-xs font-medium text-fg-muted">Lyryc</span>
      </div>
      {items(state).map((item, i) =>
        item === "line" ? (
          <div key={i} role="separator" className="mx-2.5 my-1 h-px bg-line" />
        ) : (
          <button
            key={item.id}
            type="button"
            role="menuitem"
            onClick={() => void act(item.id)}
            className={cx(
              "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-sm text-fg outline-none",
              "transition-colors duration-(--dur-fast) hover:bg-accent-soft focus-visible:bg-accent-soft",
            )}
          >
            <item.icon className="size-4 shrink-0 text-fg-muted" />
            <span className="flex-1">{item.label}</span>
            {item.keys && <span className="font-mono text-2xs text-fg-subtle">{item.keys}</span>}
          </button>
        ),
      )}
    </div>
  );
}
