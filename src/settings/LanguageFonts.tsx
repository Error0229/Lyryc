/**
 * Settings → Appearance → Fonts by language: the languages given a font of
 * their own, one row each, picked from the installed fonts made for that
 * language. "Add language" adds a row, and it is kept once a font is chosen.
 */
import { useState } from "react";
import { DropdownMenu } from "radix-ui";
import { Plus, X } from "lucide-react";
import { FONT_LANGUAGES, fontsFor, htmlLang, withFont } from "../lib/fonts";
import type { SystemFont } from "../services/ipc/backend";
import type { LyricStyle } from "../state/schema";
import { settings, useSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Row } from "../ui/Field";
import { FontPicker } from "../ui/FontPicker";
import { IconButton } from "../ui/IconButton";
import { Section } from "../ui/Section";

const nameOf = (code: string) => FONT_LANGUAGES.find((l) => l.code === code)?.name ?? code;

export function LanguageFonts({ style, installed }: { style: LyricStyle; installed: readonly SystemFont[] }) {
  // A language just added, until its font is chosen: nothing is saved without one.
  const [adding, setAdding] = useState<string | null>(null);
  const codes = [...Object.keys(style.fonts), ...(adding && !(adding in style.fonts) ? [adding] : [])];
  const addable = FONT_LANGUAGES.filter((l) => !codes.includes(l.code));
  const setFont = (code: string, font: string | null) => {
    settings.lyric({ fonts: withFont(style.fonts, code, font) });
    setAdding(null);
  };

  return (
    <Section
      title="Fonts by language"
      description="Lyrics in these languages, and translations into them, use their own font. What it lacks comes from the font above."
      action={<AddLanguage languages={addable} installed={installed} onAdd={setAdding} />}
    >
      {codes.length === 0 && <p className="px-4 py-4 text-sm text-fg-muted">Every language uses the font above.</p>}
      {codes.map((code) => {
        const own = style.fonts[code];
        const sample = FONT_LANGUAGES.find((l) => l.code === code)?.sample ?? "";
        return (
          <Row
            key={code}
            label={nameOf(code)}
            hint={
              <span lang={htmlLang(code)} className="text-sm text-fg" style={{ fontFamily: own ? `${own}, ${style.fontFamily}` : style.fontFamily }}>
                {sample}
              </span>
            }
          >
            <FontPicker
              label={`${nameOf(code)} font`}
              value={own}
              options={fontsFor(installed, code)}
              placeholder={own ? "Not installed" : "Choose a font"}
              defaultOpen={!own}
              onChange={(font) => setFont(code, font)}
              // Trying fonts leaves a just-added row waiting for its choice. Read now: a tried font is a write too.
              onPreview={(font) => settings.lyric({ fonts: withFont(useSettings.getState().lyric.fonts, code, font ?? null) })}
            />
            <IconButton label={`Remove ${nameOf(code)}`} icon={<X />} onClick={() => setFont(code, null)} />
          </Row>
        );
      })}
    </Section>
  );
}

/** The languages not listed yet; one with no font installed for it can't be picked. */
function AddLanguage({ languages, installed, onAdd }: { languages: typeof FONT_LANGUAGES; installed: readonly SystemFont[]; onAdd: (code: string) => void }) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <Button size="sm" variant="ghost" disabled={languages.length === 0}>
          <Plus /> Add language
        </Button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        {/* Focus stays off the button: the new row's font list opens and takes it. */}
        <DropdownMenu.Content align="end" sideOffset={4} onCloseAutoFocus={(e) => e.preventDefault()} className="z-(--z-popover) min-w-56 animate-pop rounded-md bg-surface p-1 shadow-3">
          {languages.map(({ code, name }) => {
            const count = fontsFor(installed, code).length;
            return (
              <DropdownMenu.Item
                key={code}
                disabled={count === 0}
                onSelect={() => onAdd(code)}
                className="flex h-8 cursor-default items-center justify-between gap-4 rounded-sm px-2 text-sm text-fg outline-none select-none data-disabled:text-fg-subtle data-highlighted:bg-surface-muted"
              >
                <span>{name}</span>
                <span className="text-xs text-fg-subtle">{count ? `${count} font${count === 1 ? "" : "s"}` : "no font installed"}</span>
              </DropdownMenu.Item>
            );
          })}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
