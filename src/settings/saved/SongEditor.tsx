import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, FileText, Plus, Trash2 } from "lucide-react";
import { formatLrc, parseLrc, parsePlain } from "../../lib/lrc";
import { songCacheKey } from "../../lib/lyrics";
import type { LyricLine } from "../../lib/types";
import { backend, type SavedChange, type SavedSong, type SavedSongDetail } from "../../services/ipc/backend";
import { useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { Callout } from "../../ui/Callout";
import { ConfirmDialog, Dialog } from "../../ui/Dialog";
import { TextArea, TextInput } from "../../ui/Field";
import { IconButton } from "../../ui/IconButton";
import { Segmented } from "../../ui/Segmented";
import { Select } from "../../ui/Select";
import { cx } from "../../lib/cx";
import { isTiming, usePrealignStatus } from "../usePrealign";
import { blankRow, draftOf, retimed, rowsOf, timeOf, type EditRow } from "./draft";
import { savedOn, sourceLabel, translationMadeWith, translationNames, type LanguageName } from "./labels";

/** Language codes by name, from the backend's list ("zh-TW" → "Chinese Traditional"). */
function useLanguageName(): LanguageName {
  const [names, setNames] = useState<Map<string, string>>(new Map());
  useEffect(() => {
    backend.languages().then((list) => setNames(new Map(list.map(([code, name]) => [code.toLowerCase(), name]))), () => {});
  }, []);
  return (code) => names.get(code.toLowerCase()) ?? code;
}

/**
 * One saved song: its lines with their times and translations, all editable,
 * and each part (lyrics, a translation, a word timing) deletable on its own.
 * Timing by listening runs in the background (`alignLyrics`): the editor shows
 * the song is waiting for it, and the timed song once it's saved.
 */
export function SongEditor({ song, onBack, onChanged }: { song: SavedSong; onBack: () => void; onChanged: () => void }) {
  const [saved, setSaved] = useState<SavedSongDetail | null>(null);
  const [rows, setRows] = useState<EditRow[]>([]);
  const [shown, setShown] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [ask, setAsk] = useState<"forget" | "discard" | null>(null);
  const [lrc, setLrc] = useState<string | null>(null);
  const aligner = useSettings((s) => s.prealign);
  const timing = isTiming(usePrealignStatus(), song.artist, song.track);
  const language = useLanguageName();
  // What the timed song's arrival may replace: unsaved edits are never thrown away.
  const dirtyNow = useRef(false);

  const load = (d: SavedSongDetail | null) => {
    if (!d) return onBack();
    setSaved(d);
    setRows(rowsOf(d.lines, d.translations.map((t) => t.lines)));
    setShown((k) => Math.min(k, Math.max(0, d.translations.length - 1)));
  };

  useEffect(() => {
    backend.savedSong(song.artist, song.track).then(load, (e) => setError(`Couldn't open it: ${String(e)}`));
  }, [song.artist, song.track]);

  // This song's timing ended (Settings says how in a toast): show what's saved now.
  useEffect(
    () =>
      backend.subscribe({
        timed: (t) => {
          if (t.error || songCacheKey(t.artist, t.track) !== songCacheKey(song.artist, song.track)) return;
          if (dirtyNow.current) return setNote("The timed song is saved. Discard your changes to see it.");
          backend.savedSong(song.artist, song.track).then(load, () => {});
        },
      }),
    [song.artist, song.track],
  );

  const draft = useMemo(() => saved && draftOf(saved, rows), [saved, rows]);
  if (!saved || !draft) return <p className="py-10 text-center text-sm text-fg-muted">{error ?? "Opening…"}</p>;

  const apply = (change: SavedChange) => {
    setBusy(true);
    setError(null);
    backend
      .savedSongChange(song.artist, song.track, change)
      .then((d) => {
        onChanged();
        load(d);
      }, (e) => setError(`Couldn't save it: ${String(e)}`))
      .finally(() => setBusy(false));
  };

  const dirty = draft.linesChanged || draft.change !== null;
  dirtyNow.current = dirty;
  const time = (lines: string[] | null) => {
    setError(null);
    return backend.alignLyrics(song.artist, song.track, lines).then(() => setNote(null));
  };
  const edit = (id: number, f: (r: EditRow) => EditRow) => setRows((rs) => rs.map((r) => (r.id === id ? f(r) : r)));
  const last = rows.reduce((t, r) => Math.max(t, timeOf(r) ?? 0), 0);
  const tr = saved.translations[shown];
  const hasTiming = saved.timings.some((t) => !t.rejected);
  const editing = rows.length > 0;
  const kept = [saved.translations.length > 0 && "translation", hasTiming && "word timing"].filter(Boolean).join(" and ");
  const cols = tr ? "grid-cols-[6rem_minmax(0,1fr)_minmax(0,1fr)_2rem]" : "grid-cols-[6rem_minmax(0,1fr)_2rem]";
  const trNames = translationNames(saved.translations, language);
  // No word timing for these words yet: the aligner can listen for it.
  const untimed = saved.song.kind === "lyrics" && saved.timings.length === 0;

  return (
    <div className="grid gap-5 pb-2">
      <header className="flex items-start gap-3">
        <IconButton label="All saved songs" icon={<ArrowLeft />} onClick={() => (dirty ? setAsk("discard") : onBack())} />
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-sm font-semibold text-fg capitalize">{song.track}</h3>
          <p className="truncate text-xs text-fg-muted capitalize">{song.artist}</p>
          <p className="mt-1 text-xs text-fg-subtle">
            {sourceLabel(saved.song)} · {savedOn(saved.song)}
          </p>
        </div>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => setAsk("forget")}>
          <Trash2 /> {saved.song.kind === "lyrics" ? "Delete lyrics" : "Forget"}
        </Button>
      </header>

      {saved.song.kind !== "lyrics" && (
        <div className="grid justify-items-start gap-3">
          <Segmented
            label="What's saved for this song"
            value={saved.song.kind}
            onChange={(kind) => apply({ op: "mark", instrumental: kind === "instrumental" })}
            options={[
              { value: "missing", label: "No lyrics found" },
              { value: "instrumental", label: "Instrumental" },
            ]}
          />
          {!editing && (
            <div className="flex gap-2">
              <Button size="sm" onClick={() => setRows([blankRow(0, 0)])}>
                <Plus /> Add lyrics
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setLrc("")}>
                <FileText /> Paste lyrics
              </Button>
            </div>
          )}
        </div>
      )}

      {timing && (
        <Callout title="Listening to the song">
          It's timed in the background, one song at a time, and saved when it's done. You can close this or open other songs meanwhile.
        </Callout>
      )}
      {note && <Callout title="Timing finished">{note}</Callout>}

      {draft.wordsChanged && kept && (
        <Callout tone="warning" title="You're changing the words">
          Saving deletes this song's {kept}, made for the old words. {saved.translations.length > 0 ? "They're" : "It's"} made again the next time it plays
          {saved.translations.length > 0 && ", and translating uses your AI provider again"}.
        </Callout>
      )}

      {editing && (
        <section className="grid gap-2">
          {saved.translations.length > 1 && (
            <div className="flex items-center justify-end gap-2">
              <span className="text-xs text-fg-muted">{saved.translations.length} translations, one per setting and model</span>
              <Select
                label="Translation shown"
                className="w-72"
                value={String(shown)}
                onChange={(v) => setShown(Number(v))}
                options={trNames.map((label, k) => ({ value: String(k), label }))}
              />
            </div>
          )}
          <div className={cx("grid gap-2 px-1 text-xs font-medium text-fg-muted", cols)} aria-hidden>
            <span>Time</span>
            <span>Words</span>
            {tr && <span>{draft.wordsChanged ? "Translation (deleted on save)" : "Translation"}</span>}
          </div>
          <ul className="grid gap-1.5">
            {rows.map((r) => (
              <li key={r.id} className={cx("grid items-center gap-2", cols)}>
                <TextInput
                  aria-label="Time"
                  aria-invalid={draft.bad.has(r.id)}
                  className={cx("font-mono tabular-nums", draft.bad.has(r.id) && "ring-2 ring-danger")}
                  value={r.time}
                  onChange={(e) => edit(r.id, (x) => retimed(x, e.target.value))}
                />
                <TextInput aria-label="Words" value={r.text} placeholder="(a break)" onChange={(e) => edit(r.id, (x) => ({ ...x, text: e.target.value }))} />
                {tr && (
                  <TextInput
                    aria-label="Translation"
                    value={r.tr[shown]}
                    disabled={draft.wordsChanged}
                    onChange={(e) => edit(r.id, (x) => ({ ...x, tr: x.tr.map((t, k) => (k === shown ? e.target.value : t)) }))}
                  />
                )}
                <IconButton label="Delete line" icon={<Trash2 />} onClick={() => setRows((rs) => rs.filter((x) => x.id !== r.id))} />
              </li>
            ))}
          </ul>
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => setRows((rs) => [...rs, blankRow(last + 2, saved.translations.length)])}>
              <Plus /> Add line
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setLrc(formatLrc(draft.change?.op === "save" ? draft.change.lines : saved.lines))}>
              <FileText /> Edit as text
            </Button>
          </div>
        </section>
      )}

      {(saved.translations.length > 0 || saved.timings.length > 0 || untimed) && (
        <section className="grid gap-1">
          <h4 className="text-xs font-medium text-fg-muted">Also saved</h4>
          {tr && (
            <Part
              label={saved.translations.length > 1 ? `Translation: ${trNames[shown]}` : "Translation"}
              hint={`${translationMadeWith(tr, language)}. Delete it to translate the song again the next time it plays with these settings.`}
              action="Delete"
              disabled={busy}
              onClick={() => apply({ op: "forgetTranslation", key: tr.key })}
            />
          )}
          {saved.timings.map((t) => (
            <Part
              key={t.videoId + t.hash}
              label={t.rejected ? "Word timing (rejected)" : "Word timing"}
              hint={`YouTube video ${t.videoId}. ${t.rejected ? "It didn't fit these lyrics. Delete it to try again" : "Delete it to work it out again"} the next time the song plays.`}
              action="Delete"
              disabled={busy}
              onClick={() => apply({ op: "forgetTiming", videoId: t.videoId, hash: t.hash })}
            />
          ))}
          {untimed && (
            <Part
              label="Word timing"
              hint={
                timing
                  ? "Listening to the song…"
                  : aligner
                    ? "None yet. Word-level timing can listen to the song as it last played on YouTube Music and work it out; the line times stay as they are."
                    : "None yet. Turn on word-level timing under Lyrics & sync to work it out by listening to the song."
              }
              action={timing ? "Listening…" : "Work it out"}
              disabled={busy || timing || !aligner || draft.wordsChanged}
              onClick={() => time(null).catch((e) => setError(`Couldn't time it: ${String(e)}.`))}
            />
          )}
        </section>
      )}

      {(dirty || error) && (
        <footer className="sticky bottom-0 -mx-5 flex items-center justify-end gap-2 border-t border-line bg-surface px-5 py-3">
          {error && <p className="mr-auto text-xs text-danger">{error}</p>}
          {draft.bad.size > 0 && <p className="mr-auto text-xs text-danger">Times look like 1:02.50.</p>}
          {dirty && (
            <>
              <Button variant="ghost" disabled={busy} onClick={() => load(saved)}>
                Discard changes
              </Button>
              <Button variant="primary" disabled={busy || !draft.change} onClick={() => draft.change && apply(draft.change)}>
                {busy ? "Saving…" : "Save"}
              </Button>
            </>
          )}
        </footer>
      )}

      <ConfirmDialog
        open={ask === "forget"}
        onOpenChange={(o) => !o && setAsk(null)}
        title={saved.song.kind === "lyrics" ? "Delete these lyrics?" : "Forget this song?"}
        description="It's looked up again the next time it plays. Its translation and word timing stay saved, for when the same words come back."
        confirmLabel="Delete"
        onConfirm={() => apply({ op: "forget" })}
      />
      <ConfirmDialog
        open={ask === "discard"}
        onOpenChange={(o) => !o && setAsk(null)}
        title="Discard your changes?"
        description="Your edits to this song aren't saved yet."
        confirmLabel="Discard"
        onConfirm={onBack}
      />
      <LyricsDialog
        text={lrc}
        replaces={saved.song.kind === "lyrics" ? kept : null}
        onClose={() => setLrc(null)}
        onApply={(lines) => setRows(rowsOf(lines, saved.translations.map((t) => t.lines)))}
        onTime={time}
      />
    </div>
  );
}

function Part({ label, hint, action, disabled, onClick }: { label: string; hint: string; action: string; disabled: boolean; onClick: () => void }) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-md px-1 py-1.5">
      <div className="min-w-0">
        <p className="text-sm text-fg">{label}</p>
        <p className="text-xs text-fg-muted">{hint}</p>
      </div>
      <Button size="sm" variant="ghost" className="shrink-0" disabled={disabled} onClick={onClick}>
        {action}
      </Button>
    </div>
  );
}

const count = (n: number) => `${n} ${n === 1 ? "line" : "lines"}`;

/**
 * The lines as text, to paste or bulk-edit; applying replaces the rows (not
 * saved yet). Lines with times ("[01:02.50] words") are used as they are.
 * Lyrics with no times at all go to the aligner (`onTime`), which listens to
 * the song in the background and saves them timed: the dialog closes as soon
 * as they're queued. `replaces`: the song has lyrics that saving them
 * replaces (with what goes with them, e.g. "translation"), or null.
 */
function LyricsDialog({
  text,
  replaces,
  onClose,
  onApply,
  onTime,
}: {
  text: string | null;
  replaces: string | null;
  onClose: () => void;
  onApply: (lines: LyricLine[]) => void;
  onTime: (lines: string[]) => Promise<void>;
}) {
  const [value, setValue] = useState("");
  const [queuing, setQueuing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const aligner = useSettings((s) => s.prealign);
  useEffect(() => {
    setValue(text ?? "");
    setQueuing(false);
    setError(null);
  }, [text]);

  const timed = parseLrc(value);
  const untimed = timed.length > 0 ? [] : parsePlain(value);
  const cant = aligner ? null : "Turn on word-level timing under Lyrics & sync to time them.";
  const replacing = replaces === null ? "" : ` They replace the song's lyrics${replaces ? `, and its ${replaces} goes with them` : ""}.`;
  const note =
    error ??
    (untimed.length > 0
      ? `These lines have no times. ${cant ?? `Word-level timing listens to the song as it last played on YouTube Music to time them, in the background (a minute or so), and saves them timed.${replacing}`}`
      : null);

  const listen = () => {
    setQueuing(true);
    setError(null);
    onTime(untimed).then(onClose, (e) => {
      setQueuing(false);
      setError(`Couldn't time them: ${String(e)}.`);
    });
  };

  return (
    <Dialog
      open={text !== null}
      onOpenChange={(o) => !o && onClose()}
      title="Lyrics as text"
      description='Lines with times look like "[01:02.50] words"; lines without one among them are skipped. Lyrics with no times at all are timed by listening to the song.'
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          {untimed.length > 0 ? (
            <Button variant="primary" disabled={queuing || !!cant} onClick={listen}>
              Time {count(untimed.length)}
            </Button>
          ) : (
            <Button
              variant="primary"
              disabled={timed.length === 0 || value === text}
              onClick={() => {
                onApply(timed);
                onClose();
              }}
            >
              Use {count(timed.length)}
            </Button>
          )}
        </>
      }
    >
      <div className="grid gap-2">
        <TextArea aria-label="Lyrics" className="min-h-72 font-mono text-xs" value={value} readOnly={queuing} onChange={(e) => (setValue(e.target.value), setError(null))} spellCheck={false} />
        <p className={cx("text-xs", error ? "text-danger" : "text-fg-muted")} aria-live="polite">
          {note}
        </p>
      </div>
    </Dialog>
  );
}
