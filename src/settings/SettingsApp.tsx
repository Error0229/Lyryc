/** Settings window shell: sidebar navigation and animated page switching. */
import { useEffect, useLayoutEffect, useRef, useState, type ComponentType } from "react";
import { motion } from "motion/react";
import { Brain, Info, Languages, Music2, Palette, Type } from "lucide-react";
import { fade, spring } from "../design/motion";
import { cx } from "../lib/cx";
import { inTauri } from "../services/ipc/backend";
import { onPageRequest, requestedPage, type SettingsPage } from "../state/nav";
import { useThemeAttribute } from "../state/theme";
import { Toaster } from "../ui/Toast";
import { AboutPage } from "./pages/About";
import { TitleBar } from "./TitleBar";
import { AppearancePage } from "./pages/Appearance";
import { FuriganaPage } from "./pages/Furigana";
import { LyricsPage } from "./pages/Lyrics";
import { ProvidersPage } from "./pages/Providers";
import { TranslationPage } from "./pages/Translation";
import type { PageProps } from "./Page";

const nav: Array<{ id: SettingsPage; label: string; icon: ComponentType<{ className?: string }>; Page: ComponentType<PageProps> }> = [
  { id: "appearance", label: "Appearance", icon: Palette, Page: AppearancePage },
  { id: "lyrics", label: "Lyrics & sync", icon: Music2, Page: LyricsPage },
  { id: "translation", label: "Translation", icon: Languages, Page: TranslationPage },
  { id: "providers", label: "AI providers", icon: Brain, Page: ProvidersPage },
  { id: "furigana", label: "Furigana", icon: Type, Page: FuriganaPage },
  { id: "about", label: "About", icon: Info, Page: AboutPage },
];

/**
 * Where the selected row is (offsetTop and height, px), measured: the tint
 * slides to exactly that, whatever the font size, never to a computed guess.
 */
function useRowBox(nav: React.RefObject<HTMLElement | null>, page: SettingsPage) {
  const [box, setBox] = useState<{ y: number; h: number } | null>(null);
  useLayoutEffect(() => {
    const el = nav.current;
    if (!el) return;
    const measure = () => {
      const row = el.querySelector<HTMLElement>("[aria-current=page]");
      if (row) setBox({ y: row.offsetTop, h: row.offsetHeight });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [nav, page]);
  return box;
}

/**
 * The window is created hidden; show it once the page (in its theme) has
 * painted, so it never opens on a blank white frame. The backend shows it
 * anyway after a moment if this never runs.
 */
function useShowWhenPainted() {
  useEffect(() => {
    if (!inTauri) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          const w = getCurrentWindow();
          void w.show().then(() => w.setFocus());
        });
      });
    });
    return () => cancelAnimationFrame(frame);
  }, []);
}

function initialPage(): SettingsPage {
  const fromUrl = new URLSearchParams(location.search).get("page") as SettingsPage | null;
  return requestedPage() ?? (nav.some((n) => n.id === fromUrl) ? fromUrl! : "appearance");
}

export function SettingsApp() {
  useThemeAttribute();
  const [page, setPage] = useState<SettingsPage>(initialPage);
  useEffect(() => onPageRequest(setPage), []);
  const current = nav.find((n) => n.id === page)!;
  const main = useRef<HTMLElement>(null);
  const navRef = useRef<HTMLElement>(null);
  const row = useRowBox(navRef, page);
  useShowWhenPainted();
  // A new page starts at its top.
  useEffect(() => {
    main.current?.scrollTo({ top: 0 });
  }, [page]);

  return (
    <div className="flex h-full flex-col">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        {/* One tint, behind the rows, sliding to the selected one (measured, so
            it lands exactly). Hover is a plain fill on that row: it never slides. */}
        <nav ref={navRef} aria-label="Settings" className="relative flex w-52 shrink-0 flex-col gap-1 border-r border-line bg-surface-sunken/60 p-3">
          {row && (
            <motion.span
              aria-hidden
              className="absolute inset-x-3 top-0 rounded-md bg-accent-soft"
              style={{ height: row.h }}
              initial={false}
              animate={{ y: row.y }}
              transition={spring.snappy}
            />
          )}
          {nav.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              onClick={() => setPage(id)}
              aria-current={id === page ? "page" : undefined}
              className={cx(
                "relative flex h-9.5 shrink-0 items-center gap-2.5 rounded-md px-2.5 text-sm transition-colors duration-(--dur-fast)",
                id === page ? "font-semibold text-fg" : "text-fg-subtle hover:bg-surface-muted/50 hover:text-fg",
              )}
            >
              <Icon className={cx("size-4 shrink-0", id === page && "text-accent")} />
              <span>{label}</span>
            </button>
          ))}
        </nav>
        {/* Never a horizontal scrollbar: content wraps or truncates, and anything that still overflows is clipped. */}
        <main ref={main} className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
          {/* The new page replaces the old at once and fades up: nothing sits blank in between. */}
          <motion.div key={page} initial={{ opacity: 0.4 }} animate={{ opacity: 1 }} transition={fade("base")} className="mx-auto max-w-2xl px-8 pt-8 pb-16">
            <current.Page go={setPage} />
          </motion.div>
        </main>
        <Toaster />
      </div>
    </div>
  );
}
