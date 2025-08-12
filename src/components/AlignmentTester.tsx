import React, { useEffect, useMemo, useState } from "react";
import { useLyricsStore } from "../stores/lyricsStore";
import { fetchLRCLibRaw } from "../services/lrclibClient";
import { alignPlainLyrics, parseLRCToAligned, compareAlignments } from "../services/textAlign";

interface Props {
  totalDurationSec: number;
}

const AlignmentTester: React.FC<Props> = ({ totalDurationSec }) => {
  const { currentTrack, setLyrics } = useLyricsStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [metrics, setMetrics] = useState<{
    mae: number; rmse: number; meanOffset: number; matched: number
  } | null>(null);
  const [previewAligned, setPreviewAligned] = useState(false);

  const disabled = !currentTrack || !totalDurationSec;

  const runTest = async () => {
    if (!currentTrack) return;
    setLoading(true);
    setError(null);
    setMetrics(null);
    try {
      const raw = await fetchLRCLibRaw(currentTrack.title, currentTrack.artist);
      if (!raw || (!raw.plainLyrics && !raw.syncedLyrics)) {
        setError("No LRCLib data to compare");
        return;
      }

      // Our alignment from plain lyrics
      const ours = alignPlainLyrics(raw.plainLyrics || "", {
        totalDurationSec: totalDurationSec,
        minLineDurationSec: 1.2,
        maxLineDurationSec: 7.0,
      });

      // LRCLib alignment from LRC
      const ref = parseLRCToAligned(raw.syncedLyrics || "");
      if (ref.length === 0 || ours.length === 0) {
        setError("Insufficient data for comparison");
        return;
      }

      const m = compareAlignments(ours, ref);
      setMetrics(m);

      if (previewAligned) {
        // Update UI to show our aligned timings
        setLyrics(
          ours.map((l) => ({ time: l.time, duration: l.duration, text: l.text }))
        );
      }
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    // Auto-run when toggled or track changes
    setMetrics(null);
    if (!disabled) {
      // noop; wait for user click
    }
  }, [currentTrack?.title, currentTrack?.artist, totalDurationSec, previewAligned]);

  return (
    <div className="mt-6 p-4 rounded-lg border border-white/15 bg-white/5">
      <div className="flex items-center justify-between mb-2">
        <div className="font-semibold text-white">Alignment Tester</div>
        <div className="flex items-center gap-3">
          <label className="text-white/80 text-sm flex items-center gap-2">
            <input
              type="checkbox"
              checked={previewAligned}
              onChange={(e) => setPreviewAligned(e.target.checked)}
            />
            Preview our alignment
          </label>
          <button
            className={`px-3 py-1 rounded-md text-sm ${disabled || loading ? "opacity-50 cursor-not-allowed" : "bg-blue-600 hover:bg-blue-500"}`}
            onClick={runTest}
            disabled={disabled || loading}
          >
            {loading ? "Running..." : "Run Comparison"}
          </button>
        </div>
      </div>
      {!currentTrack && (
        <div className="text-white/60 text-sm">Play a track on YouTube Music.</div>
      )}
      {error && <div className="text-red-400 text-sm">{error}</div>}
      {metrics && (
        <div className="text-white/90 text-sm mt-2">
          <div>Matched lines: {metrics.matched}</div>
          <div>MAE: {metrics.mae.toFixed(2)}s</div>
          <div>RMSE: {metrics.rmse.toFixed(2)}s</div>
          <div>Mean offset: {metrics.meanOffset.toFixed(2)}s</div>
        </div>
      )}
      {!metrics && !error && (
        <div className="text-white/60 text-xs">Compares our text-only alignment vs LRCLib synced timings.</div>
      )}
    </div>
  );
};

export default AlignmentTester;

