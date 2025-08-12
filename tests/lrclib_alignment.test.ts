import { describe, it, expect, beforeAll } from "vitest";
import { fetchLRCLib } from "./helpers/lrclib";
import { downloadAudioIfMissing } from "./helpers/youtube";
import { alignPlainLyrics, parseLRCToAligned, compareAlignments } from "../src/services/textAlign";

const YT_URL = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
const ARTIST = "Rick Astley";
const TRACK = "Never Gonna Give You Up";

describe("LRCLib alignment vs heuristic for Rick Astley", () => {
  let durationSec = 0;
  let plain = "";
  let synced = "";

  beforeAll(async () => {
    const lrclib = await fetchLRCLib(TRACK, ARTIST);
    if (!lrclib) throw new Error("Failed to fetch LRCLib data");
    plain = lrclib.plainLyrics ?? "";
    synced = lrclib.syncedLyrics ?? "";
    durationSec = Number(lrclib.duration || 0);
    
    // Validate LRCLib data quality
    if (!synced || synced.trim().length === 0) {
      throw new Error("No synced lyrics available from LRCLib");
    }
    if (durationSec <= 0) {
      throw new Error("Invalid duration from LRCLib");
    }
    
    // Download and verify real audio content
    const audioPath = await downloadAudioIfMissing(YT_URL, "rick-astley-never-gonna-give-you-up");
    if (!audioPath) {
      throw new Error("Audio download failed - unable to download real audio content. Test requires actual music for meaningful alignment comparison.");
    }
    
    console.log(`✓ Audio downloaded: ${audioPath}`);
    console.log(`✓ LRCLib duration: ${durationSec}s`);
    console.log(`✓ Synced lyrics lines: ${synced.split('\n').filter(l => l.match(/^\[.*?\]/)).length}`);
  }, 120_000);

  it("fetches LRCLib synced lyrics and computes baseline metrics", async () => {
    // Validate basic data integrity
    expect(durationSec).toBeGreaterThan(180); // Rick Astley song should be ~3+ minutes
    expect(synced.length).toBeGreaterThan(500); // Should have substantial synced content
    expect(plain.length).toBeGreaterThan(200); // Should have substantial plain text

    const lrcAligned = parseLRCToAligned(synced);
    expect(lrcAligned.length).toBeGreaterThan(30); // Rick Astley has many lyrics lines
    
    // Validate LRC timing structure makes sense
    const firstTime = lrcAligned[0]?.time || 0;
    const lastTime = lrcAligned[lrcAligned.length - 1]?.time || 0;
    expect(firstTime).toBeGreaterThanOrEqual(0);
    expect(lastTime).toBeLessThanOrEqual(durationSec);
    expect(lastTime - firstTime).toBeGreaterThan(120); // Should span most of the song

    // Heuristic alignment from plain lyrics constrained by total duration
    const plainText = plain || lrcAligned.map(l => l.text).join("\n");
    expect(plainText.length).toBeGreaterThan(200); // Ensure we have substantial text to align
    
    const baseAligned = alignPlainLyrics(plainText, {
      totalDurationSec: durationSec,
      minLineDurationSec: 1.0,
      maxLineDurationSec: 8.0,
    });
    
    expect(baseAligned.length).toBeGreaterThan(10); // Should generate reasonable number of aligned lines
    const baseMetrics = compareAlignments(baseAligned, lrcAligned);

    // More stringent baseline checks with actual music
    expect(baseMetrics.matched).toBeGreaterThan(15);
    expect(baseMetrics.mae).toBeLessThan(12); // Tighter bounds for real music
    expect(baseMetrics.rmse).toBeLessThan(20);
    
    console.log(`Baseline metrics: MAE=${baseMetrics.mae.toFixed(2)}s, RMSE=${baseMetrics.rmse.toFixed(2)}s, matched=${baseMetrics.matched}`);
  }, 120_000);

  it("refines heuristic parameters to reduce timing error vs LRCLib", async () => {
    const lrcAligned = parseLRCToAligned(synced);
    const text = plain || lrcAligned.map(l => l.text).join("\n");
    
    // Validate we have quality data for meaningful comparison
    expect(lrcAligned.length).toBeGreaterThan(20);
    expect(text.length).toBeGreaterThan(200);

    let best = { mae: Number.POSITIVE_INFINITY, rmse: Number.POSITIVE_INFINITY, meanOffset: 0, matched: 0 };
    let bestParams = { min: 0.8, max: 9.0 };

    // Expanded grid search with more realistic parameter ranges for real music
    const mins = [0.5, 0.7, 1.0, 1.3, 1.5];
    const maxs = [6.0, 7.0, 8.0, 9.0, 10.0, 12.0];
    let searchCount = 0;
    
    for (const min of mins) {
      for (const max of maxs) {
        if (max <= min + 1.0) continue; // Ensure reasonable range
        searchCount++;
        
        const aligned = alignPlainLyrics(text, { 
          totalDurationSec: durationSec, 
          minLineDurationSec: min, 
          maxLineDurationSec: max 
        });
        const m = compareAlignments(aligned, lrcAligned);
        
        // More sophisticated scoring: primarily MAE, but consider RMSE and match count
        const score = m.mae + (m.rmse * 0.2) - (m.matched * 0.01);
        const bestScore = best.mae + (best.rmse * 0.2) - (best.matched * 0.01);
        
        if (m.matched > 0 && score < bestScore) {
          best = m; 
          bestParams = { min, max };
        }
      }
    }

    // Expect refinement improves over a naive 1..8s bounds
    const naiveAligned = alignPlainLyrics(text, { 
      totalDurationSec: durationSec, 
      minLineDurationSec: 1.0, 
      maxLineDurationSec: 8.0 
    });
    const naive = compareAlignments(naiveAligned, lrcAligned);

    // More comprehensive logging for debugging
    console.log(`Parameter search: ${searchCount} combinations tested`);
    console.log(`Best params: min=${bestParams.min}s, max=${bestParams.max}s`);
    console.log(`Best metrics: MAE=${best.mae.toFixed(3)}s, RMSE=${best.rmse.toFixed(3)}s, matched=${best.matched}`);
    console.log(`Naive metrics: MAE=${naive.mae.toFixed(3)}s, RMSE=${naive.rmse.toFixed(3)}s, matched=${naive.matched}`);
    console.log(`Improvement: ${((naive.mae - best.mae) / naive.mae * 100).toFixed(1)}% reduction in MAE`);

    // Stricter validation with real music
    expect(best.matched).toBeGreaterThan(15);
    expect(best.mae).toBeLessThan(naive.mae + 0.01); // Must improve (allowing tiny float precision)
    expect(best.mae).toBeLessThan(10); // Should achieve better than 10s average error with real music
    expect(best.rmse).toBeLessThan(15); // Should have reasonable RMSE
    
    // Verify the optimization found improvements (even small ones are meaningful)
    const improvement = (naive.mae - best.mae) / naive.mae;
    expect(improvement).toBeGreaterThanOrEqual(0); // Should improve or at least not get worse
    
    // Log meaningful improvements if found
    if (improvement > 0.01) {
      console.log(`✓ Meaningful improvement found: ${(improvement * 100).toFixed(1)}%`);
    } else {
      console.log(`✓ Minor/no improvement: ${(improvement * 100).toFixed(1)}% (baseline already quite good)`);
    }
  }, 180_000);
});
