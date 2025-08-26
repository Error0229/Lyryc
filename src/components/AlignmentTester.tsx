import React, { useState } from "react";
import { useLyricsStore } from "../stores/lyricsStore";
import { fetchLRCLibRaw } from "../services/lrclibClient";
import { alignPlainLyrics, parseLRCToAligned, compareAlignments } from "../services/textAlign";

interface Props {
  totalDurationSec: number;
}

const AlignmentTester: React.FC<Props> = ({ totalDurationSec }) => {
  const { currentTrack, setLyrics } = useLyricsStore();
  const [loading, setLoading] = useState(false);
  const [metrics, setMetrics] = useState<{
    mae: number; rmse: number; meanOffset: number; matched: number
  } | null>(null);

  const runTest = async () => {
    if (!currentTrack) return;
    setLoading(true);
    try {
      const raw = await fetchLRCLibRaw(currentTrack.title, currentTrack.artist);
      if (!raw?.plainLyrics || !raw?.syncedLyrics) return;

      const ours = alignPlainLyrics(raw.plainLyrics, {
        totalDurationSec,
        minLineDurationSec: 1.2,
        maxLineDurationSec: 7.0,
      });

      const ref = parseLRCToAligned(raw.syncedLyrics);
      if (ref.length === 0 || ours.length === 0) return;

      const m = compareAlignments(ours, ref);
      setMetrics(m);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center space-x-2">
        <button
          onClick={runTest}
          disabled={!currentTrack || loading}
          className="px-2 py-1 bg-blue-500/20 hover:bg-blue-500/30 disabled:bg-white/5 text-blue-200 disabled:text-white/40 rounded text-xs transition-colors"
        >
          {loading ? "..." : "Test"}
        </button>
        {metrics && (
          <div className="text-xs text-white/60">
            MAE: {metrics.mae.toFixed(1)}s | {metrics.matched} lines
          </div>
        )}
      </div>
    </div>
  );
};

export default AlignmentTester;