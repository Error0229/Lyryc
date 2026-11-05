/** Overlay data hooks: backend events, translation, furigana, the strip's rows and height. */
import { useEffect, useMemo, useRef } from "react";
import { backend } from "../services/ipc/backend";
import { textSignature, translationKey } from "../lib/lyrics";
import { hasKanji, isJapaneseSong } from "../lib/japanese";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { applyLyrics, reportLlmError, setTrack } from "./actions";
import { playhead } from "./playhead";
import { ROOM_PX, ROOM_X_PX, songRows, stripHeight, wantsTranslation, type Rows } from "./rows";
import { sameTrack } from "./upgrade";
import { isResend, sampleAge } from "../lib/clock";
import type { LyricLine, PrealignStatus } from "../lib/types";
import type { TranslationPrefs } from "../state/schema";

const log = (what: string) => (e: unknown) => console.warn(`[overlay] ${what}:`, e);
const NO_HOVER = { inside: false, target: null };

/** The previous time report's measured-at stamp (see `sampleAge`), and
 * whether stamps have been seen to advance (see `isResend`). */
let lastMeasuredAt: number | null = null;
let stampsAdvance = false;

export function useBackendEvents() {
  useEffect(() => {
    backend.initExtension().catch(log("init_extension_connection"));
    backend
      .currentTrack()
      .then((t) => t && setTrack(t))
      .catch(log("get_current_track"));
    backend
      .stripState()
      .then(({ locked, anchor, firstRun }) => useSession.setState({ locked, anchor, firstRun }))
      .catch(log("strip_state"));

    return backend.subscribe({
      track: setTrack,
      time: (s) => {
        if (s.measuredAt && lastMeasuredAt && s.measuredAt > lastMeasuredAt) stampsAdvance = true;
        // A re-send or an out-of-order report: older than one already used, so it says nothing new.
        if (isResend(s.measuredAt, lastMeasuredAt, stampsAdvance)) return;
        // The report took a while to get here: count that time as played.
        const age = s.isPlaying ? sampleAge(s.measuredAt, lastMeasuredAt, Date.now()) : 0;
        lastMeasuredAt = s.measuredAt ?? null;
        playhead.sample(s.currentTime + age, s.isPlaying, age);
        if (useSession.getState().playing !== s.isPlaying) useSession.setState({ playing: s.isPlaying });
      },
      playing: (p) => {
        playhead.setPlaying(p);
        useSession.setState({ playing: p });
      },
      // Locking once means the strip is placed: the first-run hint is done.
      locked: (locked) => useSession.setState(locked ? { locked, firstRun: false, hover: NO_HOVER } : { locked }),
      anchor: (anchor) => useSession.setState({ anchor }),
      hover: (hover) => useSession.setState({ hover }),
      lyrics: ({ track, lines, source }) => applyLyrics(track, lines, source),
      progress: ({ track, stage }) => {
        const s = useSession.getState();
        if (s.status === "loading" && sameTrack(s.track, track)) useSession.setState({ stage });
      },
      timingFinal: (track) => {
        if (sameTrack(useSession.getState().track, track)) useSession.setState({ pending: false });
      },
      prealign: followAligner,
      llmSettings: () => useSession.setState((s) => ({ llmRevision: s.llmRevision + 1 })),
      ahead: pretranslate,
    });
  }, []);
}

/**
 * The pre-aligner names the song it works on ("Title — Artist"). While that
 * is this song, its word timing is on the way; when it stops without sending
 * any (a rejected alignment, a failed download), nothing more is coming.
 */
function followAligner(st: PrealignStatus) {
  const s = useSession.getState();
  const cut = st.workingOn?.lastIndexOf(" — ") ?? -1;
  const working = cut < 0 ? null : { title: st.workingOn!.slice(0, cut), artist: st.workingOn!.slice(cut + 3) };
  const mine = sameTrack(s.track, working);
  if (mine === s.aligning) return;
  useSession.setState(mine ? { aligning: true } : { aligning: false, pending: false });
}

/**
 * Mirror the settings the backend acts on by itself (word-level timing, the
 * lookahead) to it, which keeps no copy: on start and on every change (also
 * from the settings window, via storage).
 */
export function useSettingsMirror() {
  const prealign = useSettings((s) => s.prealign);
  const lookahead = useSettings((s) => s.lookahead);
  const extraSources = useSettings((s) => s.extraSources);
  useEffect(() => {
    backend.prealignConfigure(prealign, extraSources).catch(log("prealign_configure"));
  }, [prealign, extraSources]);
  useEffect(() => {
    backend.lookaheadConfigure(lookahead, extraSources).catch(log("lookahead_configure"));
  }, [lookahead, extraSources]);
}

/** A settings change waits this long for the next one before translating. */
const SETTLE_MS = 1000;
/** The playing song's translation asks again after these waits: a 503 "high
 * demand" spike passes in seconds, and the backend no longer works around it
 * with dozens of smaller requests. */
const RETRY_MS = [15_000, 45_000];
/** Recent translations by lyric text and prefs: flipping a setting back is free. */
const recent = new Map<string, Record<string, string>>();
const RECENT_MAX = 20;
/** Translations asked for and not back yet, so a song that starts while its
 * pre-translation runs waits for it instead of paying twice. */
const inflight = new Map<string, Promise<Record<string, string>>>();

function remember(key: string, map: Record<string, string>) {
  recent.delete(key);
  recent.set(key, map);
  if (recent.size > RECENT_MAX) recent.delete(recent.keys().next().value!);
}

/** What a translation of these lines with these prefs is known by, or null
 * when the prefs don't want them translated (no LLM call, no cost). */
function requestKey(lines: LyricLine[], p: TranslationPrefs): string | null {
  if (!wantsTranslation(lines, p)) return null;
  return JSON.stringify([textSignature(lines), p.sourceLanguage, p.targetLanguage, p.style, p.customPrompt, p.preserveRhyme, p.preserveMeaning, p.maintainStructure]);
}

function translateOnce(key: string, lines: LyricLine[], p: TranslationPrefs): Promise<Record<string, string>> {
  const known = recent.get(key);
  if (known) return Promise.resolve(known);
  let asked = inflight.get(key);
  if (!asked) {
    asked = backend
      .translate(lines, p)
      .then((list) => {
        const map: Record<string, string> = {};
        for (const l of list) map[translationKey(l.original)] = l.translated;
        remember(key, map);
        return map;
      })
      .finally(() => inflight.delete(key));
    inflight.set(key, asked);
  }
  return asked;
}

/** An upcoming song is minutes away: a failed pre-translation (a 503 spike)
 * asks again after these waits instead of leaving the song to translate when
 * it starts. */
const AHEAD_RETRY_MS = [30_000, 90_000];

/** An upcoming song's lyrics, translated now and kept (here and in the
 * backend's cache), so its translation is there when it starts. Failures stay
 * quiet: the song reports its own when it plays. */
export function pretranslate(lines: LyricLine[]) {
  const ask = (attempt: number) => {
    const p = useSettings.getState().translation;
    const key = requestKey(lines, p);
    if (!key) return;
    translateOnce(key, lines, p).catch(() => {
      if (attempt < AHEAD_RETRY_MS.length) window.setTimeout(() => ask(attempt + 1), AHEAD_RETRY_MS[attempt]);
    });
  };
  ask(0);
}

/**
 * Translate when the lyric *text* or the translation prefs change; retiming
 * alone does not. New lyrics translate at once. A prefs change waits until the
 * user stops changing them: each request costs quota, and a burst of them is
 * what providers answer with 429/503.
 */
export function useTranslations() {
  const lyrics = useSession((s) => s.lyrics);
  const p = useSettings((s) => s.translation);
  // A provider set up after a failure retries; a translation that succeeded is
  // in `recent` and costs nothing.
  const llmRevision = useSession((s) => s.llmRevision);
  const signature = useMemo(() => textSignature(lyrics), [lyrics]);
  const translatedSignature = useRef("");

  useEffect(() => {
    const lines = useSession.getState().lyrics;
    if (!p.enabled || lines.length === 0) return;
    const key = requestKey(lines, p);
    if (key === null) {
      // Filtered out before any LLM call: no cost for songs the user doesn't want translated.
      useSession.setState({ translations: {} });
      return;
    }
    const known = recent.get(key);
    if (known) {
      useSession.setState({ translations: known });
      return;
    }
    const delay = translatedSignature.current === signature ? SETTLE_MS : 0;
    translatedSignature.current = signature;
    let live = true;
    const ask = (attempt: number) =>
      translateOnce(key, lines, p)
        .then((map) => {
          if (live) useSession.setState({ translations: map });
        })
        .catch((e) => {
          if (!live) return;
          // What is showing was made for other prefs (another target language).
          useSession.setState({ translations: {} });
          if (attempt < RETRY_MS.length) timer = window.setTimeout(() => ask(attempt + 1), RETRY_MS[attempt]);
          else reportLlmError(e, "Translation");
        });
    let timer = window.setTimeout(() => ask(0), delay);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // Prefs fields are listed individually (and the filter by value: cross-window
    // sync rebuilds objects) so display-only changes don't retranslate.
  }, [signature, llmRevision, p.enabled, p.sourceLanguage, p.targetLanguage, p.style, p.customPrompt, p.preserveRhyme, p.preserveMeaning, p.maintainStructure, p.filter.mode, p.filter.languages.join()]);
}

const pending = new Set<string>();

function ensureFurigana(text: string) {
  if (!hasKanji(text) || pending.has(text) || useSession.getState().furigana[text]) return;
  pending.add(text);
  backend
    .furigana(text)
    .catch(() => [{ text }])
    .then((spans) => useSession.setState((s) => ({ furigana: { ...s.furigana, [text]: spans } })))
    .finally(() => pending.delete(text));
}

/** Fetch readings for the active line and the next two, so they are ready in time. */
export function useFurigana(index: number) {
  const enabled = useSettings((s) => s.furigana.enabled);
  const lyrics = useSession((s) => s.lyrics);
  const japanese = useMemo(() => isJapaneseSong(lyrics), [lyrics]);

  useEffect(() => {
    if (!enabled || !japanese) return;
    lyrics.slice(Math.max(index, 0), Math.max(index, 0) + 3).forEach((l) => ensureFurigana(l.text));
  }, [enabled, japanese, lyrics, index]);

  return enabled && japanese;
}

/**
 * The rows the strip reserves for the song showing. They change when new
 * lyrics land or a setting changes; while the next song is looked up (no
 * lyrics yet, or none found) the last song's rows stay, so nothing resizes.
 */
export function useRows(): Rows {
  const lyrics = useSession((s) => s.lyrics);
  const furigana = useSettings((s) => s.furigana);
  const translation = useSettings((s) => s.translation);
  const nextLine = useSettings((s) => s.nextLine);
  const last = useRef(lyrics);
  if (lyrics.length) last.current = lyrics;
  const lines = last.current;
  const f = furigana;
  const t = translation;
  return useMemo(
    () => songRows(lines, f, t, nextLine),
    // By value: cross-window settings sync rebuilds the objects.
    [lines, nextLine, f.enabled, f.mode, t.enabled, t.display, t.sourceLanguage, t.targetLanguage, t.filter.mode, t.filter.languages.join()],
  );
}

/** Size the strip to the rows: one resize per song, not one per arrival. */
export function useStripHeight(rows: Rows) {
  const lyric = useSettings((s) => s.lyric);
  const furigana = useSettings((s) => s.furigana);
  const translation = useSettings((s) => s.translation);
  const remPx = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  const height = stripHeight(rows, lyric, furigana, translation, remPx);
  useEffect(() => {
    backend.setFrame(height, { x: ROOM_X_PX, y: ROOM_PX }).catch(log("strip_set_frame"));
  }, [height]);
}
