/** Settings window shell: sidebar navigation and animated page switching. */
import { useEffect, useState, type ComponentType } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Brain, Info, Languages, Music2, Palette, Type } from "lucide-react";
import { duration, ease } from "../design/motion";
import { cx } from "../lib/cx";
import { onPageRequest, requestedPage, type SettingsPage } from "../state/nav";
import { useSettings } from "../state/settings";
import { Toaster } from "../ui/Toast";
import { AboutPage } from "./pages/About";
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

/** Resolve "system" and keep <html data-theme> in sync. */
function useThemeAttribute() {
  const theme = useSettings((s) => s.theme);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "dark" : "light") : theme;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
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

  return (
    <div className="flex h-full">
      <nav aria-label="Settings" className="flex w-52 shrink-0 flex-col gap-0.5 border-r border-line bg-surface-sunken/60 p-3">
        <div className="mb-4 flex items-center gap-2 px-2 pt-1">
          <span className="grid size-6 place-items-center rounded-md bg-accent text-2xs font-bold text-accent-fg">L</span>
          <span className="text-sm font-semibold tracking-tight">Lyryc</span>
        </div>
        {nav.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => setPage(id)}
            aria-current={id === page ? "page" : undefined}
            className={cx(
              "relative flex h-8 items-center gap-2.5 rounded-md px-2 text-sm transition-colors duration-(--dur-fast)",
              id === page ? "text-fg" : "text-fg-muted hover:bg-surface-muted hover:text-fg",
            )}
          >
            {id === page && (
              <motion.span
                layoutId="nav-active"
                className="absolute inset-0 rounded-md bg-surface shadow-1"
                transition={{ type: "spring", stiffness: 520, damping: 40 }}
              />
            )}
            <Icon className="relative size-4 shrink-0" />
            <span className="relative">{label}</span>
          </button>
        ))}
      </nav>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={page}
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0, transition: { duration: duration.slow, ease: ease.out } }}
            exit={{ opacity: 0, transition: { duration: duration.instant, ease: ease.in } }}
            className="mx-auto max-w-2xl px-8 pt-8 pb-16"
          >
            <current.Page go={setPage} />
          </motion.div>
        </AnimatePresence>
      </main>
      <Toaster />
    </div>
  );
}
