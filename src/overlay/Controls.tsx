/** Hover-revealed mini controls for the overlay strip. */
import { motion } from "motion/react";
import { EyeOff, GripVertical, Languages, Minus, Pause, Play, Plus, Settings2, SkipBack, SkipForward } from "lucide-react";
import { spring, fade } from "../design/motion";
import { formatOffset, NUDGE_STEP, trackKey } from "../lib/offset";
import { backend } from "../services/ipc/backend";
import { useSession } from "../state/session";
import { settings, useSettings } from "../state/settings";
import { openSettings } from "../state/nav";
import { IconButton } from "../ui/IconButton";
import { playhead } from "./playhead";

const Divider = () => <span aria-hidden className="mx-1 h-4 w-px bg-glass-line" />;

function OffsetNudge({ track }: { track: { artist: string; title: string } }) {
  const key = trackKey(track.artist, track.title);
  const value = useSettings((s) => s.sync.trackOffsets[key] ?? 0);
  const nudge = (d: number) => settings.setTrackOffset(key, value + d);
  return (
    <div className="flex items-center">
      <IconButton tone="glass" tooltip={false} label="Lyrics later (−0.1 s)" icon={<Minus />} onClick={() => nudge(-NUDGE_STEP)} />
      <button
        type="button"
        title="Track offset · click to reset"
        onClick={() => settings.setTrackOffset(key, 0)}
        className="h-7 min-w-16 rounded-md px-1 font-mono text-2xs text-glass-fg-muted tabular-nums transition-colors hover:bg-glass-hover hover:text-glass-fg"
      >
        {formatOffset(value)}
      </button>
      <IconButton tone="glass" tooltip={false} label="Lyrics earlier (+0.1 s)" icon={<Plus />} onClick={() => nudge(NUDGE_STEP)} />
    </div>
  );
}

function togglePlay(playing: boolean) {
  // Optimistic: the extension confirms via playback-state shortly after.
  useSession.setState({ playing: !playing });
  playhead.setPlaying(!playing);
  void backend.playback(playing ? "pause" : "play");
}

export function Controls() {
  const track = useSession((s) => s.track);
  const playing = useSession((s) => s.playing);
  const translating = useSettings((s) => s.translation.enabled);

  return (
    <motion.div
      initial={{ opacity: 0, x: 8, scale: 0.98 }}
      animate={{ opacity: 1, x: 0, scale: 1, transition: spring.gentle }}
      exit={{ opacity: 0, x: 4, transition: fade("fast") }}
      className="absolute top-1/2 right-2 z-(--z-controls) -translate-y-1/2"
    >
      <div
        role="toolbar"
        aria-label="Lyryc controls"
        className="flex h-9 items-center rounded-full border border-glass-line bg-glass px-1 shadow-2 backdrop-blur-xl"
      >
        <span
          aria-label="Drag to move"
          title="Drag to move"
          onPointerDown={() => void backend.startDrag()}
          className="flex h-7 w-5 cursor-grab items-center justify-center text-glass-fg-muted active:cursor-grabbing [&_svg]:size-4"
        >
          <GripVertical />
        </span>
        {track && (
          <>
            <IconButton tone="glass" tooltip={false} label="Previous" icon={<SkipBack />} onClick={() => void backend.playback("previous")} />
            <IconButton tone="glass" tooltip={false} label={playing ? "Pause" : "Play"} icon={playing ? <Pause /> : <Play />} onClick={() => togglePlay(playing)} />
            <IconButton tone="glass" tooltip={false} label="Next" icon={<SkipForward />} onClick={() => void backend.playback("next")} />
            <Divider />
            <OffsetNudge track={track} />
            <Divider />
          </>
        )}
        <IconButton
          tone="glass"
          tooltip={false}
          label={translating ? "Hide translation" : "Show translation"}
          aria-pressed={translating}
          icon={<Languages />}
          onClick={() => settings.translation({ enabled: !translating })}
        />
        <IconButton tone="glass" tooltip={false} label="Settings" icon={<Settings2 />} onClick={() => void openSettings()} />
        <IconButton tone="glass" tooltip={false} label="Hide (Ctrl+Shift+M to show)" icon={<EyeOff />} onClick={() => void backend.hideOverlay()} />
      </div>
    </motion.div>
  );
}
