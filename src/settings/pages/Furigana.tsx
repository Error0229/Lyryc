import type { FuriganaMode } from "../../state/schema";
import { settings, useSettings } from "../../state/settings";
import { ColorField } from "../../ui/ColorField";
import { Row } from "../../ui/Field";
import { Section } from "../../ui/Section";
import { Segmented } from "../../ui/Segmented";
import { Slider } from "../../ui/Slider";
import { Switch } from "../../ui/Switch";
import { PageHeader } from "../Page";
import { Preview } from "../Preview";

const set = settings.furigana;
const pct = (v: number) => `${Math.round(v * 100)}%`;

export function FuriganaPage() {
  const f = useSettings((s) => s.furigana);
  return (
    <div className="grid gap-8">
      <PageHeader title="Furigana" description="Readings over kanji in Japanese lyrics. Songs without kana are left alone." />
      <Preview sample="ja" />

      <Section title="Readings">
        <Row label="Show furigana">
          <Switch label="Show furigana" checked={f.enabled} onChange={(enabled) => set({ enabled })} />
        </Row>
        <Row label="Placement" hint="Replace swaps kanji for their reading.">
          <Segmented<FuriganaMode>
            label="Placement"
            value={f.mode}
            onChange={(mode) => set({ mode })}
            options={[
              { value: "over", label: "Above" },
              { value: "under", label: "Below" },
              { value: "replace", label: "Replace" },
            ]}
          />
        </Row>
        <Row label="Size" hint="Relative to the lyric size.">
          <Slider label="Furigana size" value={f.scale} min={0.3} max={0.8} step={0.05} format={pct} onChange={(scale) => set({ scale })} />
        </Row>
        <Row label="Opacity">
          <Slider label="Furigana opacity" value={f.opacity} min={0.3} max={1} step={0.05} format={pct} onChange={(opacity) => set({ opacity })} />
        </Row>
        <Row label="Colour" hint="Before the word is sung; it then takes the sweep colour.">
          <ColorField label="Furigana" value={f.color} onChange={(color) => set({ color })} />
        </Row>
      </Section>
    </div>
  );
}
