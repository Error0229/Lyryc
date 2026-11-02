import { useEffect, useState } from "react";
import { backend } from "../../services/ipc/backend";
import type { TranslationDisplay, TranslationStyle } from "../../state/schema";
import { settings, useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { Callout } from "../../ui/Callout";
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

const set = settings.translation;
const pct = (v: number) => `${Math.round(v * 100)}%`;

export function TranslationPage({ go }: PageProps) {
  const t = useSettings((s) => s.translation);
  const languages = useLanguages();
  const { settings: llm } = useLlm();
  const options = languages.map(([value, label]) => ({ value, label }));
  const routed = !!llm?.routes.translation;

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
