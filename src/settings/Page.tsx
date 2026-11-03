import type { SettingsPage } from "../state/nav";

export interface PageProps {
  go: (page: SettingsPage) => void;
}

/** Title block shared by every page. */
export function PageHeader({ title, description }: { title: string; description: string }) {
  return (
    <header>
      <h1 className="text-xl font-semibold tracking-tight text-fg">{title}</h1>
      <p className="mt-1 text-sm text-fg-muted">{description}</p>
    </header>
  );
}
