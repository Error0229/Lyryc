import { useState } from "react";
import { RotateCcw, Trash2 } from "lucide-react";
import { formatOffset, OFFSET_LIMIT, trackKey } from "../../lib/offset";
import { settings, useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";
import { IconButton } from "../../ui/IconButton";
import { Section } from "../../ui/Section";
import { Slider } from "../../ui/Slider";
import { Switch } from "../../ui/Switch";
import { PageHeader } from "../Page";
import { useCurrentTrack } from "../useTrack";

const RANGE = OFFSET_LIMIT / 2;

function OffsetSlider({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  return (
    <>
      <Slider label={label} value={value} min={-RANGE} max={RANGE} step={0.05} format={formatOffset} onChange={onChange} />
      <IconButton label="Reset to zero" icon={<RotateCcw />} disabled={value === 0} onClick={() => onChange(0)} />
    </>
  );
}

function TrackOffsets({ offsets }: { offsets: Record<string, number> }) {
  const entries = Object.entries(offsets).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) {
    return <p className="px-4 py-6 text-center text-sm text-fg-muted">No per-track offsets yet. Nudge one from the overlay while a song plays.</p>;
  }
  return (
    <ul className="max-h-72 divide-y divide-line overflow-y-auto">
      {entries.map(([key, value]) => (
        <li key={key} className="flex items-center gap-3 px-4 py-2">
          <span className="min-w-0 flex-1 truncate text-sm capitalize">{key}</span>
          <span className="font-mono text-xs text-fg-muted tabular-nums">{formatOffset(value)}</span>
          <IconButton label={`Remove offset for ${key}`} icon={<Trash2 />} onClick={() => settings.setTrackOffset(key, 0)} />
        </li>
      ))}
    </ul>
  );
}

export function LyricsPage() {
  const wordHighlight = useSettings((s) => s.wordHighlight);
  const extraSources = useSettings((s) => s.extraSources);
  const { globalOffset, trackOffsets } = useSettings((s) => s.sync);
  const track = useCurrentTrack();
  const [confirm, setConfirm] = useState(false);
  const key = track ? trackKey(track.artist, track.title) : null;

  return (
    <div className="grid gap-8">
      <PageHeader title="Lyrics & sync" description="How lines are highlighted, and how to fix lyrics that run early or late." />

      <Section title="Display">
        <Row label="Word-by-word highlight" hint="Sweeps each word when the lyrics have word timing. Off shows the line in one colour.">
          <Switch label="Word-by-word highlight" checked={wordHighlight} onChange={settings.setWordHighlight} />
        </Row>
      </Section>

      <Section title="Sources">
        <Row
          label="Extra lyric sources"
          hint="Shows YouTube Music's own lyrics for the track playing there, and asks QQ Music, NetEase and Kugou for timing LRCLIB lacks, sending them the title and artist. Unofficial lookups: they use these services' internal APIs, not public ones, outside their terms, and may stop working."
        >
          <Switch label="Extra lyric sources" checked={extraSources} onChange={settings.setExtraSources} />
        </Row>
      </Section>

      <Section title="Sync" description="Positive values show lyrics earlier. The overlay's ± buttons adjust the current track.">
        <Row label="All tracks">
          <OffsetSlider label="Global offset" value={globalOffset} onChange={settings.setGlobalOffset} />
        </Row>
        {key && track && (
          <Row label="This track" hint={`${track.title} — ${track.artist}`}>
            <OffsetSlider label="Track offset" value={trackOffsets[key] ?? 0} onChange={(v) => settings.setTrackOffset(key, v)} />
          </Row>
        )}
      </Section>

      <Section
        title="Saved track offsets"
        action={
          <Button size="sm" variant="ghost" disabled={Object.keys(trackOffsets).length === 0} onClick={() => setConfirm(true)}>
            Clear all
          </Button>
        }
      >
        <TrackOffsets offsets={trackOffsets} />
      </Section>

      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Clear all offsets?"
        description="Every per-track offset and the global offset go back to zero. This can't be undone."
        confirmLabel="Clear offsets"
        onConfirm={settings.clearOffsets}
      />
    </div>
  );
}
