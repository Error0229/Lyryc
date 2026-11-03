import { useEffect, useState } from "react";
import { backend } from "../../services/ipc/backend";
import { languageFamily } from "../../lib/language";
import type { LanguageFilterMode, TranslationDisplay, TranslationStyle } from "../../state/schema";
import { settings, useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { Callout } from "../../ui/Callout";
import { Chips } from "../../ui/Chips";
import { ColorField } from "../../ui/ColorField";
import { Row, TextArea } from "../../ui/Field";
import { Section } from "../../ui/Section";
import { Segmented } from "../../ui/Segmented";
import { Select, type Option } from "../../ui/Select";
import { Slider } from "../../ui/Slider";
import { Switch } from "../../ui/Switch";
import { PageHeader, type PageProps } from "../Page";
import { Preview } from "../Preview";
import { useLlm } from "../providers/useLlm";

const fallbackLanguages: Array<[string, string]> = [
  ["auto", "Auto-detect"],
  ["en", "English"],
  ["ja", "Japanese"],
  ["zh", "Chinese"],
  ["ko", "Korean"],
  ["es", "Spanish"],
];

const styles: Option<TranslationStyle>[] = [
  { value: "natural", label: "Natural" },
  { value: "literal", label: "Literal" },
  { value: "poetic", label: "Poetic" },
  { value: "rhyming", label: "Rhyming" },
  { value: "custom", label: "Custom prompt" },
];

function useLanguages() {
  const [list, setList] = useState(fallbackLanguages);
  useEffect(() => {
    backend.languages().then(setList, () => {});
  }, []);
  return list;
}

const filterModes = [
  { value: "all", label: "All languages" },
  { value: "only", label: "Only these" },
  { value: "except", label: "All except" },
] as const;

/** One chip per language family: lyrics show Simplified vs Traditional, not the region. */
function familyOptions(languages: Array<[string, string]>) {
  const seen = new Set<string>();
  return languages
    .filter(([code]) => code !== "auto")
    .map(([code, label]) => ({ value: languageFamily(code), label: label.replace(/ - [^(]*(?=\()/, " ") }))
    .filter((o) => !seen.has(o.value) && seen.add(o.value));
}

const set = settings.translation;
const pct = (v: number) => `${Math.round(v * 100)}%`;

export function TranslationPage({ go }: PageProps) {
  const t = useSettings((s) => s.translation);
  const languages = useLanguages();
  const { settings: llm } = useLlm();
  const options = languages.map(([value, label]) => ({ value, label }));
  const routed = !!llm?.routes.translation;
  const target = options.find((o) => o.value === t.targetLanguage)?.label ?? t.targetLanguage;
  const setFilter = (p: Partial<typeof t.filter>) => set({ filter: { ...t.filter, ...p } });

  return (
    <div className="grid gap-8">
      <PageHeader title="Translation" description="Show a translation under each line, made by the AI provider you choose." />

      {llm && !routed && (
        <Callout
          tone="warning"
          title="Set up a provider to translate"
          action={
            <Button size="sm" variant="primary" onClick={() => go("providers")}>
              Set up a provider
            </Button>
          }
        >
          Translation runs on your own API key. Add a provider and route translation to it.
        </Callout>
      )}

      <Preview forceTranslation />

      <Section title="Translate">
        <Row label="Show translation" hint="Also toggled from the overlay's controls.">
          <Switch label="Show translation" checked={t.enabled} onChange={(enabled) => set({ enabled })} />
        </Row>
        <Row label="Translate to">
          <Select label="Translate to" value={t.targetLanguage} options={options.filter((o) => o.value !== "auto")} onChange={(targetLanguage) => set({ targetLanguage })} />
        </Row>
        <Row label="Lyrics language" hint="Auto-detect works for most songs.">
          <Select label="Lyrics language" value={t.sourceLanguage} options={options} onChange={(sourceLanguage) => set({ sourceLanguage })} />
        </Row>
        <Row label="Style">
          <Select label="Style" value={t.style} options={styles} onChange={(style) => set({ style })} />
        </Row>
        {t.style === "custom" && (
          <div className="px-4 py-3">
            <TextArea
              aria-label="Custom prompt"
              placeholder="e.g. Translate like a 1970s folk singer, keep it short."
              value={t.customPrompt}
              onChange={(e) => set({ customPrompt: e.target.value })}
            />
          </div>
        )}
        <Row label="Keep the rhyme">
          <Switch label="Keep the rhyme" checked={t.preserveRhyme} onChange={(preserveRhyme) => set({ preserveRhyme })} />
        </Row>
        <Row label="Keep the meaning">
          <Switch label="Keep the meaning" checked={t.preserveMeaning} onChange={(preserveMeaning) => set({ preserveMeaning })} />
        </Row>
        <Row label="Keep line structure">
          <Switch label="Keep line structure" checked={t.maintainStructure} onChange={(maintainStructure) => set({ maintainStructure })} />
        </Row>
      </Section>

      <Section
        title="Which songs"
        description={`Translation runs only for songs in the languages you choose. Songs already in ${target} are never translated.`}
      >
        <Row label="Translate songs in" hint="The lyrics language is detected per song.">
          <Segmented<LanguageFilterMode> label="Translate songs in" value={t.filter.mode} options={filterModes} onChange={(mode) => setFilter({ mode })} />
        </Row>
        {t.filter.mode !== "all" && (
          <div className="grid gap-3 px-4 py-3">
            <Chips
              label={t.filter.mode === "only" ? "Translate only these languages" : "Never translate these languages"}
              value={t.filter.languages}
              options={familyOptions(languages)}
              onChange={(languages) => setFilter({ languages })}
            />
            {t.filter.mode === "only" && t.filter.languages.length === 0 && (
              <p className="text-xs text-warning">Pick at least one language, or no song will be translated.</p>
            )}
          </div>
        )}
      </Section>

      <Section title="Display">
        <Row label="Layout">
          <Segmented<TranslationDisplay>
            label="Layout"
            value={t.display}
            onChange={(display) => set({ display })}
            options={[
              { value: "bilingual", label: "Stacked" },
              { value: "side-by-side", label: "Side by side" },
              { value: "translated", label: "Translation only" },
            ]}
          />
        </Row>
        <Row label="Size" hint="Relative to the lyric size.">
          <Slider label="Translation size" value={t.scale} min={0.3} max={1.2} step={0.05} format={pct} onChange={(scale) => set({ scale })} />
        </Row>
        <Row label="Colour">
          <ColorField label="Translation" value={t.color} onChange={(color) => set({ color })} />
        </Row>
        <Row label="Opacity">
          <Slider label="Translation opacity" value={t.opacity} min={0.2} max={1} step={0.05} format={pct} onChange={(opacity) => set({ opacity })} />
        </Row>
      </Section>
    </div>
  );
}
