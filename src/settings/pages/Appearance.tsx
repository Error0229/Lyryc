import { AlignCenter, AlignLeft, AlignRight, Monitor, Moon, RotateCcw, Sun } from "lucide-react";
import { matchPreset, overlayPresets } from "../../design/overlayPresets";
import { cx } from "../../lib/cx";
import type { FontWeight, LyricStyle, TextAlign, UiTheme } from "../../state/schema";
import { settings, useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { ColorField } from "../../ui/ColorField";
import { Row } from "../../ui/Field";
import { Section } from "../../ui/Section";
import { Segmented } from "../../ui/Segmented";
import { Select } from "../../ui/Select";
import { Slider } from "../../ui/Slider";
import { Switch } from "../../ui/Switch";
import { PageHeader } from "../Page";
import { Preview } from "../Preview";

const fonts = [
  { value: "Inter, system-ui, sans-serif", label: "Inter / System" },
  { value: "'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif", label: "Segoe UI" },
  { value: "'Noto Sans JP', 'Yu Gothic UI', 'Hiragino Sans', sans-serif", label: "Noto Sans JP" },
  { value: "Georgia, 'Times New Roman', serif", label: "Serif" },
  { value: "'JetBrains Mono', 'Cascadia Code', monospace", label: "Monospace" },
];

const px = (v: number) => `${Math.round(v)} px`;
const set = settings.lyric;

function Presets({ style }: { style: LyricStyle }) {
  const active = matchPreset(style);
  return (
    <div className="grid grid-cols-5 gap-2 p-3">
      {overlayPresets.map((p) => (
        <button
          key={p.id}
          type="button"
          onClick={() => set(p.style)}
          aria-pressed={active === p.id}
          className={cx(
            "group grid gap-1.5 rounded-md p-1.5 text-left transition-[transform,background-color] duration-(--dur-fast) ease-standard active:scale-[0.98]",
            active === p.id ? "bg-accent-soft" : "hover:bg-surface-muted",
          )}
        >
          <span
            className="grid h-12 place-items-center rounded-sm bg-[#2a2e36] text-base font-semibold"
            style={{ background: p.style.backgroundOpacity ? p.style.background : undefined }}
          >
            <span style={{ color: p.style.activeColor, textShadow: p.style.glow ? `0 0 10px ${p.style.glowColor}` : undefined }}>
              Aa<span style={{ color: p.style.upcomingColor }}>a</span>
            </span>
          </span>
          <span className={cx("px-0.5 text-xs", active === p.id ? "font-medium text-fg" : "text-fg-muted")}>{p.name}</span>
        </button>
      ))}
    </div>
  );
}

export function AppearancePage() {
  const theme = useSettings((s) => s.theme);
  const style = useSettings((s) => s.lyric);

  return (
    <div className="grid gap-8">
      <PageHeader title="Appearance" description="How the lyric strip looks. Changes apply to the overlay immediately." />
      <Preview />

      <Section title="Window">
        <Row label="Settings theme" hint="The overlay keeps its own colours below.">
          <Segmented<UiTheme>
            label="Settings theme"
            value={theme}
            onChange={settings.setTheme}
            options={[
              { value: "system", label: <Monitor />, title: "System" },
              { value: "light", label: <Sun />, title: "Light" },
              { value: "dark", label: <Moon />, title: "Dark" },
            ]}
          />
        </Row>
      </Section>

      <Section title="Style" description="Presets set colours and effects; your font and size stay.">
        <Presets style={style} />
      </Section>

      <Section
        title="Typography"
        action={
          <Button size="sm" variant="ghost" onClick={settings.resetLyric}>
            <RotateCcw /> Reset style
          </Button>
        }
      >
        <Row label="Font">
          <Select label="Font" value={fonts.find((f) => f.value === style.fontFamily)?.value} placeholder="Custom" options={fonts} onChange={(fontFamily) => set({ fontFamily })} />
        </Row>
        <Row label="Size">
          <Slider label="Size" value={style.fontSize} min={1} max={5} step={0.05} format={(v) => px(v * 16)} onChange={(fontSize) => set({ fontSize })} />
        </Row>
        <Row label="Weight">
          <Segmented<`${FontWeight}`>
            label="Weight"
            value={`${style.fontWeight}`}
            onChange={(v) => set({ fontWeight: Number(v) as FontWeight })}
            options={[
              { value: "400", label: "Regular" },
              { value: "500", label: "Medium" },
              { value: "600", label: "Semibold" },
              { value: "700", label: "Bold" },
            ]}
          />
        </Row>
        <Row label="Alignment">
          <Segmented<TextAlign>
            label="Alignment"
            value={style.align}
            onChange={(align) => set({ align })}
            options={[
              { value: "left", label: <AlignLeft />, title: "Left" },
              { value: "center", label: <AlignCenter />, title: "Center" },
              { value: "right", label: <AlignRight />, title: "Right" },
            ]}
          />
        </Row>
        <Row label="Letter spacing">
          <Slider label="Letter spacing" value={style.letterSpacing} min={-0.05} max={0.2} step={0.005} format={(v) => `${(v * 100).toFixed(1)}%`} onChange={(letterSpacing) => set({ letterSpacing })} />
        </Row>
        <Row label="Line height">
          <Slider label="Line height" value={style.lineHeight} min={1} max={2} step={0.05} format={(v) => v.toFixed(2)} onChange={(lineHeight) => set({ lineHeight })} />
        </Row>
      </Section>

      <Section title="Colours" description="Words sweep from upcoming to active, then settle as sung.">
        <Row label="Upcoming words">
          <ColorField label="Upcoming words" value={style.upcomingColor} onChange={(upcomingColor) => set({ upcomingColor })} />
        </Row>
        <Row label="Active word">
          <ColorField label="Active word" value={style.activeColor} onChange={(activeColor) => set({ activeColor })} />
        </Row>
        <Row label="Sung words">
          <ColorField label="Sung words" value={style.sungColor} onChange={(sungColor) => set({ sungColor })} />
        </Row>
        <Row label="Plain text" hint="Used when word highlighting is off.">
          <ColorField label="Plain text" value={style.textColor} onChange={(textColor) => set({ textColor })} />
        </Row>
      </Section>

      <Section title="Effects">
        <Row label="Shadow" hint="Keeps lyrics readable over any background.">
          {style.shadow && <ColorField label="Shadow" value={style.shadowColor} onChange={(shadowColor) => set({ shadowColor })} />}
          <Switch label="Shadow" checked={style.shadow} onChange={(shadow) => set({ shadow })} />
        </Row>
        {style.shadow && (
          <Row label="Shadow softness">
            <Slider label="Shadow softness" value={style.shadowBlur} min={2} max={48} step={1} format={px} onChange={(shadowBlur) => set({ shadowBlur })} />
          </Row>
        )}
        <Row label="Glow" hint="A soft halo on sung words.">
          {style.glow && <ColorField label="Glow" value={style.glowColor} onChange={(glowColor) => set({ glowColor })} />}
          <Switch label="Glow" checked={style.glow} onChange={(glow) => set({ glow })} />
        </Row>
      </Section>

      <Section title="Background">
        <Row label="Colour">
          <ColorField label="Background" value={style.background} onChange={(background) => set({ background })} />
        </Row>
        <Row label="Soft backdrop" hint="A feathered shadow behind the words, so they stay readable on light or busy screens.">
          <Slider label="Soft backdrop" value={style.scrim} min={0} max={1} step={0.05} format={(v) => `${Math.round(v * 100)}%`} onChange={(scrim) => set({ scrim })} />
        </Row>
        <Row label="Opacity" hint="0% leaves only the text on screen.">
          <Slider label="Background opacity" value={style.backgroundOpacity} min={0} max={100} step={1} format={(v) => `${Math.round(v)}%`} onChange={(backgroundOpacity) => set({ backgroundOpacity })} />
        </Row>
      </Section>
    </div>
  );
}
