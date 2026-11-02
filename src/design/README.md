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

Semantic names only. The settings window has light and dark themes
(`<html data-theme>`, resolved from the user's choice or the OS).

| Token | Use |
|---|---|
| `bg`, `surface`, `surface-muted`, `surface-sunken` | Page, cards, hover, inset tracks |
| `line`, `line-strong` | Hairlines, input borders |
| `fg`, `fg-muted`, `fg-subtle` | Text hierarchy |
| `accent` (+`-hover`, `-fg`, `-soft`) | Primary action, selection, focus |
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
   The word sweep is a pair of counter-translated layers. Colour changes are
   cross-fades between stacked layers.
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

## Primitives (`src/ui`)

`Button`, `IconButton`, `Switch`, `Slider`, `Select`, `Segmented`, `Row`/`Field`/
`TextInput`/`TextArea`, `Card`/`Section`, `Callout`, `ColorField`, `Tooltip`,
`Toast`/`Toaster`, `Dialog`/`ConfirmDialog`. Radix provides focus, keyboard and
ARIA behaviour for Switch, Slider, Select, Segmented (ToggleGroup), Tooltip and
Dialog. Styling is ours.
