# Lyryc design system

Quiet, crisp, out of the way. One accent, neutral greys, generous spacing, and
motion that explains rather than decorates.

## Where tokens live

| File | Holds |
|---|---|
| `tokens.css` | Values that switch with the theme (`--c-*` colours, `--elev-*`), z-layers, durations |
| `index.css` `@theme` | Static scales Tailwind turns into utilities: type, radius, easing, keyframes |
| `motion.ts` | The same durations/easings for JS, plus named springs |
| `overlayPresets.ts` | Colour presets for the lyric strip |

Tailwind's default palette is removed (`--color-*: initial`). If a colour is not
a token, it does not exist.

## Colour

**Brand: 錆浅葱 `#3F6F75`**, the カシ seal's teal, is the app's main colour.
`brand` (+`-hover`, `-pressed`, `-fg` paper `#F5EDE0`, `-bright` for lines and
marks on dark glass, `-tint`) live in `tokens.css`; `accent` derives from it
(lifted in the dark theme, where `#3F6F75` text would be too dark). Red is
`danger` only: real errors and destructive actions.

Semantic names only. The settings window has light and dark themes
(`<html data-theme>`, resolved from the user's choice or the OS).

| Token | Use |
|---|---|
| `bg`, `surface`, `surface-muted`, `surface-sunken` | Page, cards, hover, inset tracks |
| `line`, `line-strong` | Hairlines, input borders |
| `fg`, `fg-muted`, `fg-subtle` | Text hierarchy |
| `brand` (+`-hover`, `-pressed`, `-fg`, `-bright`, `-tint`) | The overlay's own actions (Lock/Unlock), Move mode's ring and grips; the settings title bar |
| `accent` (+`-hover`, `-fg`, `-soft`) | Primary action, selection, focus (derived from `brand`) |
| `success`, `danger`, `warning` (+`-soft`) | Status only |
| `glass*` | Overlay chrome: always dark translucent, independent of theme |

The lyric strip doesn't use the theme. Its colours are user settings, exposed as
CSS variables by `overlay/style.ts` (`--lyric-*`, `--tr-*`, `--rt-*`). Presets
(Classic, Karaoke, Glow, Panel, Paper) only set colours and effects. They never
change the user's font or size.

## Type, space, shape

- Type: 11 / 12 / **13 (UI default)** / 14 / 16 / 20 / 24 px (`text-2xs` … `text-2xl`).
  Section titles are 12 px uppercase `fg-subtle`. Page titles are 20 px semibold.
- Space: a 4 px grid (Tailwind's `--spacing`). Rows are at least 56 px high, sections are 32 px apart.
- Radius: `sm` 6 · `md` 8 (controls) · `lg` 12 (cards) · `xl` 16 (dialogs).
- Elevation: `shadow-1` (cards, inputs), `shadow-2` (floating chrome), `shadow-3` (dialogs, popovers, toasts).
- Z-layers: `--z-controls` 20 · `--z-scrim` 40 · `--z-dialog` 50 · `--z-popover` 60 · `--z-toast` 70 · `--z-tooltip` 80.

## Motion

| Token | ms | Use |
|---|---|---|
| `instant` | 80 | Exit of a page before the next enters |
| `fast` | 140 | Hover/press colour, popover in, toast out |
| `base` | 200 | Switches, fades |
| `slow` | 320 | Page enter, hover backdrop |
| `line` | 420 | Lyric line settle (as a spring) |
| `--dur-retime` | 360 | Word fill easing into new timing after a lyric swap |

Easings: `standard` (0.2,0,0,1) for state changes, `out` (0.16,1,0.3,1) for
things arriving, `in` (0.4,0,1,1) for things leaving.

Springs (`motion.ts`), picked by intent and never tuned inline:
`snappy` (thumbs, toggles), `gentle` (controls, toasts), `lyric` (line enter).

Principles:

1. **Transform and opacity only.** Nothing animates layout, colour or clip.
   The word sweep is a pair of counter-translated layers with a static
   feathered mask at its front edge. Colour changes are cross-fades between
   stacked layers.
2. **No React per frame.** One rAF loop (`overlay/playhead.ts`) writes a single
   `--t` variable on the stage. Token progress is computed in CSS from it.
   React renders only when the active line changes.
3. **Arrive slower than you leave.** Enter with `out` or a spring. Exit faster with `in`.
4. **Continuity over spectacle.** When lyrics are re-timed mid-song the line
   keeps its identity (text + occurrence), so there is no exit/enter. The fill
   eases to its new position over `--dur-retime`.
5. **Respect `prefers-reduced-motion`.** `MotionConfig reducedMotion="user"`
   drops transform animations, CSS durations collapse, and the word sweep becomes a
   per-word switch. The highlight still carries the timing information.

## The overlay strip

**Placement** (`src-tauri/src/placement.rs`). The strip hangs from the top or
bottom edge of a display's work area, at a gap, with its centre at a fraction
of the width; one placement per display (name + resolution + scale). It grows
away from its anchored edge. Locked (default) = click-through; unlocked = Move
mode (toolbar, width grips, drag the strip; Esc or Ctrl+Shift+D locks).

**Always a layered window** (`placement::keep_layered`). Click-through is
WS_EX_TRANSPARENT only; WS_EX_LAYERED stays on for good (a subclass re-adds it
to every style change). Dropping it, as tao's click-through toggle does, moved
the window onto a fresh surface that could show solid white on the first
Unlock after start. Layered, the window still takes clicks where it draws
something, and its fully transparent room lets clicks through even in Move mode.

**The drag is ours** (`placement::drag`), not Windows': Windows keeps a dragged
window's top on screen, which would hold the strip a room's height off the top
edge. Ours follows the cursor and puts the strip, every tick, where the drop
would settle it: inside the work area, flush to an edge within 24 px of it, on
the centre line within 24 px of that. What you see while dragging is where it
lands, and the strip can touch the top edge and the taskbar.

**The window is bigger than the strip** (`rows.ts` `ROOM_PX`, `ROOM_X_PX`): the
same transparent room above and below (and 4 px at each end, where the width
grips straddle the strip's ring), click-through while locked, free to hang
off-screen or over the taskbar. Away from the
anchored edge, one slot straddles the strip's edge: the brand Unlock pill when a
locked strip is hovered, the toolbar in Move mode, each with its name tip beyond
it. While the strip is dragged the backend reports the anchor live
(`strip-anchor`), so the pill slides to the other side as the strip crosses the
middle, not after the drop; the room is symmetric, so that moves nothing else.
Move mode marks the strip with a solid brand ring and the width grips. Nothing
in Move mode covers the lyrics.

**Pills.** A pill is a capsule with 4 px around 28 px circular buttons, so the
curves are concentric; its fill is `glass-strong` (a transparent window has
nothing behind it to blur).

**Whole pixels.** Icons are 1–2 px strokes; at a half-pixel offset each stroke
smears over two pixels and looks blurred. So: no `translate(-50%)` centring;
the pill slot centres its pill on a whole device pixel in JS (`Centred` in
Overlay.tsx: flex centring lands on .5 px when the widths differ in parity); the
strip's height is always even, so what is centred in it stays whole.

**Line changes.** A new line fades up into place. With "Show the next line" the
stage scrolls instead (`Stage.tsx`): everything moves up by one row step
(measured: lyric row to next-line row), the old line leaves at the top, and the
preview becomes the new line, growing from the preview's size on the same path,
so it reads as one line moving up. ~320 ms, transform and opacity only.

**Karaoke handover.** A word dims the way it lit up: once it is done, the sung
colour sweeps across it left to right, a character at a time, with the same
feathered edge as the highlight and at the word's own pace (never quicker than
`--dim-min`, 0.35 s). The light leads, the dim trails. Both fronts are on one
copy of the word (the dim is a gradient painted through the glyphs), because
two stacked copies make a word look bolder, as if highlighted twice.

**Hover while locked** comes only from the backend's cursor poll. The window is
click-through then, so the DOM never hears the pointer leave (after Lock, or off
the Unlock pill); trusting its hover would leave the pill stuck on screen.

**Rows** (`overlay/rows.ts`). The height is decided per song: the lyric row,
plus furigana / translation / next-line rows when the song has them. Nothing
that arrives later (a translation, furigana, a retimed line) resizes the strip.

**Icons by default; words only in tooltips.** Every control is an icon. Its
name (at most three words, plus a hotkey: "Search again", "Lock · Esc") shows in
the one shared tip (`NameTip`): over the hovered button, beyond the pill
(sliding between buttons), or beside the status icon, so it fits a strip of
any height. Words stay only where an icon can't carry them: the
lyrics and the title card, the 6 s "No synced lyrics" message, the offset value,
the first-run hint (in the tip's place until something is hovered), the tray menu and the settings window.
Never the native `title=` tooltip: the settings window uses `ui/Tooltip` (an
icon button gets it by default; a disabled control is wrapped in a span that
carries it), and `aria-label` stays the accessible name.

**The tray icon** (`src-tauri/src/tray.rs`). Click shows or hides the lyrics
(after the double-click time, so a double click doesn't flash the strip);
double click opens Settings; right click opens our own menu (`tray.html`,
`tray/TrayMenu.tsx`), not the native one: a small window in the settings
theme, created hidden at startup so it opens at once at the cursor, sized to
its items, gone when it loses focus. Arrows, Enter and Esc work in it.

**One status voice** (`overlay/phase.ts`, `Status.tsx`). One icon, a shape per
state, its name on hover; silence means everything is fine. It sits with the
content, never at the strip's border: just after the lyric line's last word
(scaling with it; the line always keeps room for it, so it never shifts the
line), ahead of the title card's title, or alone in the middle (beside the seal
while idle).

| State | Icon | Name |
|---|---|---|
| Idle (5 s after launch, or hovered) | ▷ in a circle | Waiting for YouTube Music |
| Looking up | magnifier, breathing | Looking up lyrics |
| Identifying | sparkle, breathing | Identifying the song / Searching the web |
| Other sources | globe, breathing | Trying other sources / Finding the original's words |
| Estimated timing | ≈ | Estimated timing |
| Syncing words | four bars rising and falling | Syncing to the vocals |
| Timing improved (2.5 s) | ✓, green | Synced to the vocals / Synced lyrics found |
| Searching again | arrows, spinning | Searching again |
| Search again changed nothing (3 s) | ✓ / magnifier with ✕ | Same lyrics found / Nothing else found |
| Not found (after 6 s) | crossed-out captions | No lyrics found |
| Instrumental (after 4 s) | ♪ | Instrumental |
| Lyrics playing, paused | none | — |

## Primitives (`src/ui`)

`Button`, `IconButton`, `Switch`, `Slider`, `Select`, `Segmented`, `Row`/`Field`/
`TextInput`/`TextArea`, `Card`/`Section`, `Callout`, `ColorField`, `Tooltip`,
`Toast`/`Toaster`, `Dialog`/`ConfirmDialog`. Radix provides focus, keyboard and
ARIA behaviour for Switch, Slider, Select, Segmented (ToggleGroup), Tooltip and
Dialog. Styling is ours.
