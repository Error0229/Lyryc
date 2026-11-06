/** The settings theme, applied to a window: resolve "system" and keep <html data-theme> in sync. */
import { useEffect } from "react";
import { useSettings } from "./settings";

export function useThemeAttribute() {
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
