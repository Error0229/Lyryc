/** Overlay data hooks: backend events, translation, furigana, window height. */
import { useEffect, useMemo, useRef, type RefObject } from "react";
import { backend } from "../services/ipc/backend";
import { textSignature, translationKey } from "../lib/lyrics";
import { detectLanguage, shouldTranslate } from "../lib/language";
import { hasKanji, isJapaneseSong } from "../lib/japanese";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { applyLyrics, reportLlmError, setTrack } from "./actions";
import { playhead } from "./playhead";

const log = (what: string) => (e: unknown) => console.warn(`[overlay] ${what}:`, e);

export function useBackendEvents() {
  useEffect(() => {
    backend.initExtension().catch(log("init_extension_connection"));
    backend
      .currentTrack()
      .then((t) => t && setTrack(t))
      .catch(log("get_current_track"));

    return backend.subscribe({
      track: setTrack,
      time: (s) => {
        playhead.sample(s.currentTime, s.isPlaying);
        if (useSession.getState().playing !== s.isPlaying) useSession.setState({ playing: s.isPlaying });
      },
      playing: (p) => {
        playhead.setPlaying(p);
        useSession.setState({ playing: p });
      },
      clickThrough: (on) => useSession.setState({ clickThrough: on }),
      lyrics: ({ track, lines, source }) => applyLyrics(track, lines, source),
      llmSettings: () => useSession.setState((s) => ({ llmRevision: s.llmRevision + 1 })),
    });
  }, []);
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
/** Recent translations by lyric text and prefs: flipping a setting back is free. */
const recent = new Map<string, Record<string, string>>();
const RECENT_MAX = 20;

function remember(key: string, map: Record<string, string>) {
  recent.delete(key);
  recent.set(key, map);
  if (recent.size > RECENT_MAX) recent.delete(recent.keys().next().value!);
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
    const lang = p.sourceLanguage === "auto" ? detectLanguage(lines.map((l) => l.text)) : p.sourceLanguage;
    if (!shouldTranslate(lang, p.targetLanguage, p.filter)) {
      // Filtered out before any LLM call: no cost for songs the user doesn't want translated.
      useSession.setState({ translations: {} });
      return;
    }
    const key = JSON.stringify([signature, p.sourceLanguage, p.targetLanguage, p.style, p.customPrompt, p.preserveRhyme, p.preserveMeaning, p.maintainStructure]);
    const known = recent.get(key);
    if (known) {
      useSession.setState({ translations: known });
      return;
    }
    const delay = translatedSignature.current === signature ? SETTLE_MS : 0;
    translatedSignature.current = signature;
    let live = true;
    const timer = window.setTimeout(() => {
      backend
        .translate(lines, p)
        .then((list) => {
          const map: Record<string, string> = {};
          for (const l of list) map[translationKey(l.original)] = l.translated;
          remember(key, map);
          if (live) useSession.setState({ translations: map });
        })
        .catch((e) => {
          if (!live) return;
          // What is showing was made for other prefs (another target language).
          useSession.setState({ translations: {} });
          reportLlmError(e, "Translation");
        });
    }, delay);
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

/** Keep the native window as tall as the content (backend adds its own padding). */
export function useFitWindow(ref: RefObject<HTMLElement | null>, hasTranslation: boolean, hasFurigana: boolean) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let last = 0;
    let timer = 0;
    const send = () => {
      const h = Math.ceil(el.getBoundingClientRect().height);
      if (Math.abs(h - last) < 2) return;
      last = h;
      backend.fitHeight(h, hasTranslation, hasFurigana).catch(log("adjust_window_height"));
    };
    const ro = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = window.setTimeout(send, 150);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      clearTimeout(timer);
    };
  }, [ref, hasTranslation, hasFurigana]);
}
