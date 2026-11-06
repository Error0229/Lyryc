//! Where the lyric strip sits, and whether it is locked.
//!
//! A placement is relative to one display's work area (the screen minus the
//! taskbar): the edge the strip hangs from, its distance from that edge, where
//! its centre sits across the width (0..1) and its width, in logical px. That
//! survives DPI and resolution changes that raw x/y don't, and a taller strip
//! never moves its anchored edge (a bottom strip grows up, not into the
//! taskbar). One placement is kept per display (name + resolution + scale) in
//! `placement.json` in the config dir.
//!
//! Locked (the default) = click-through. Unlocked = Move mode: draggable (by
//! our own drag, not Windows': see `drag`), toolbar showing. While locked the strip still follows the cursor: a poll
//! hit-tests it against the rects the overlay reports, so the Unlock icon can
//! take a click while the rest stays click-through.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Monitor, WebviewWindow, Window, WindowEvent};

// ---------------------------------------------------------------- geometry

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn scaled(self, k: f64) -> Rect {
        Rect { x: self.x * k, y: self.y * k, w: self.w * k, h: self.h * k }
    }
    fn round(self) -> Rect {
        Rect { x: self.x.round(), y: self.y.round(), w: self.w.round(), h: self.h.round() }
    }
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    /// Distance from a point to the rect; 0 inside.
    fn distance(&self, x: f64, y: f64) -> f64 {
        let dx = (self.x - x).max(x - (self.x + self.w)).max(0.0);
        let dy = (self.y - y).max(y - (self.y + self.h)).max(0.0);
        dx.hypot(dy)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub anchor: Edge,
    /// Distance from the anchored edge of the work area.
    pub gap: f64,
    /// The strip's centre across the work area, 0 (left) to 1 (right).
    pub center: f64,
    pub width: f64,
}

/// Within this many px of an edge, or of the centre line, the strip snaps to it (flush to the edge).
pub const SNAP: f64 = 24.0;
/// The default spot's distance from the taskbar. Only the default: a strip
/// dragged near an edge snaps flush to it.
pub const DEFAULT_GAP: f64 = 24.0;
pub const MIN_WIDTH: f64 = 480.0;
const DEFAULT_WIDTH: f64 = 1100.0;
const DEFAULT_SHARE: f64 = 0.6;

impl Placement {
    /// Bottom centre, 60% of the width (at most 1100 px), 24 px above the taskbar.
    pub fn default_for(work: Rect) -> Self {
        let width = (work.w * DEFAULT_SHARE).min(DEFAULT_WIDTH).max(MIN_WIDTH.min(work.w));
        Self { anchor: Edge::Bottom, gap: DEFAULT_GAP, center: 0.5, width }
    }

    /// The window rect for a strip `h` tall, kept inside the work area.
    pub fn rect(&self, h: f64, work: Rect) -> Rect {
        let w = self.width.clamp(MIN_WIDTH.min(work.w), work.w);
        let h = h.min(work.h);
        let x = (work.x + self.center * work.w - w / 2.0).clamp(work.x, work.x + work.w - w);
        let gap = self.gap.clamp(0.0, work.h - h);
        let y = match self.anchor {
            Edge::Top => work.y + gap,
            Edge::Bottom => work.y + work.h - gap - h,
        };
        Rect { x, y, w, h }
    }

    /// Where the user left the strip, as a placement: hung from the edge its
    /// centre is nearer to, snapped flush to that edge and to the centre line
    /// when close.
    pub fn settle(r: Rect, work: Rect) -> Self {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let anchor = if cy < work.y + work.h / 2.0 { Edge::Top } else { Edge::Bottom };
        let gap = match anchor {
            Edge::Top => r.y - work.y,
            Edge::Bottom => work.y + work.h - (r.y + r.h),
        };
        let gap = if gap <= SNAP { 0.0 } else { gap };
        let center = if (cx - (work.x + work.w / 2.0)).abs() <= SNAP {
            0.5
        } else {
            ((cx - work.x) / work.w).clamp(0.0, 1.0)
        };
        Self { anchor, gap, center, width: r.w.clamp(MIN_WIDTH.min(work.w), work.w) }
    }

    /// Same spot to within half a pixel: not worth a save or a move.
    fn same(&self, o: &Placement) -> bool {
        self.anchor == o.anchor
            && (self.gap - o.gap).abs() < 0.5
            && (self.width - o.width).abs() < 0.5
            && (self.center - o.center).abs() < 1e-4
    }
}

/// Transparent room around the strip, logical px: `y` above and below it (the
/// Unlock pill / Move toolbar sit there, on whichever side is away from the
/// anchor), `x` at each end (Move mode's width grips straddle the strip's
/// edge). The same on both sides, so the strip sits at the same spot in its
/// window whatever the anchor: flipping it moves nothing. The room may hang
/// off-screen or over the taskbar; only the strip has to stay in the work area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct Room {
    pub x: f64,
    pub y: f64,
}

/// The window around a strip.
pub fn window_of(strip: Rect, room: Room) -> Rect {
    Rect { x: strip.x - room.x, y: strip.y - room.y, w: strip.w + 2.0 * room.x, h: strip.h + 2.0 * room.y }
}

/// The strip inside a window: what a placement describes.
pub fn strip_of(window: Rect, room: Room) -> Rect {
    Rect {
        x: window.x + room.x,
        y: window.y + room.y,
        w: (window.w - 2.0 * room.x).max(0.0),
        h: (window.h - 2.0 * room.y).max(0.0),
    }
}

// ---------------------------------------------------------------- displays

/// A display, in physical px (Windows' virtual-screen coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub key: String,
    pub scale: f64,
    pub bounds: Rect,
    pub work: Rect,
}

impl Display {
    fn from_monitor(m: &Monitor) -> Self {
        let (p, s, wa) = (m.position(), m.size(), m.work_area());
        Self {
            key: display_key(m.name().map(String::as_str), s.width, s.height, m.scale_factor()),
            scale: m.scale_factor(),
            bounds: Rect { x: p.x as f64, y: p.y as f64, w: s.width as f64, h: s.height as f64 },
            work: Rect {
                x: wa.position.x as f64,
                y: wa.position.y as f64,
                w: wa.size.width as f64,
                h: wa.size.height as f64,
            },
        }
    }

    fn work_logical(&self) -> Rect {
        self.work.scaled(1.0 / self.scale)
    }
}

/// What a placement is saved under: the same monitor at another resolution or
/// scale gets its own, because the old one would put the strip somewhere else.
pub fn display_key(name: Option<&str>, w: u32, h: u32, scale: f64) -> String {
    format!("{}|{w}x{h}|{}%", name.unwrap_or("display"), (scale * 100.0).round())
}

/// The display under a physical point, else the nearest one.
pub fn display_at(displays: &[Display], x: f64, y: f64) -> Option<usize> {
    displays
        .iter()
        .position(|d| d.bounds.contains(x, y))
        .or_else(|| {
            (0..displays.len()).min_by(|&a, &b| {
                displays[a].bounds.distance(x, y).total_cmp(&displays[b].bounds.distance(x, y))
            })
        })
}

// ---------------------------------------------------------------- store

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Store {
    /// The display the strip was last placed on.
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub displays: HashMap<String, Placement>,
}

impl Store {
    /// The display to start on and its placement: the last one used if it is
    /// connected, else the first connected display with a saved placement
    /// (the primary first), else the primary at the default spot.
    pub fn choose(&self, displays: &[Display], primary: usize) -> (usize, Placement) {
        let saved = |i: usize| self.displays.get(&displays[i].key).map(|p| (i, *p));
        let last = self.last.as_ref().and_then(|k| displays.iter().position(|d| &d.key == k));
        last.and_then(saved)
            .or_else(|| saved(primary))
            .or_else(|| (0..displays.len()).find_map(saved))
            .unwrap_or_else(|| (primary, Placement::default_for(displays[primary].work_logical())))
    }

    fn load(path: &Path) -> Option<Store> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text)
            .map_err(|e| warn!("placement: {} unreadable ({e}); starting from the default spot", path.display()))
            .ok()
    }

    fn save(&self, path: &Path) {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
            std::fs::rename(&tmp, path)
        };
        if let Err(e) = write() {
            warn!("placement: couldn't save {}: {e}", path.display());
        }
    }
}

// ---------------------------------------------------------------- runtime

/// A rect the overlay reports, in CSS px from the window's top-left. A
/// `clickable` one takes clicks while the strip is locked.
#[derive(Clone, Debug, Deserialize)]
pub struct Hit {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub clickable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Hover {
    pub inside: bool,
    /// The id of the reported rect under the cursor.
    pub target: Option<String>,
}

pub struct Strip {
    path: Option<PathBuf>,
    store: Store,
    display: Display,
    placement: Placement,
    /// The strip's height (what shows), logical px.
    height: f64,
    /// Transparent room around it, logical px.
    room: Room,
    /// The anchor the overlay was last told: live while dragging, before any settle.
    told: Edge,
    locked: bool,
    /// No placement was ever saved: the strip opens in Move mode with a hint.
    first_run: bool,
    shown: bool,
    /// The window's bounds as last set (its client area), physical px.
    bounds: Rect,
    /// The strip inside it, physical px: what "the cursor is over the strip" means.
    strip: Rect,
    hits: Vec<Hit>,
    hover: Hover,
    /// Click-through is off right now because the cursor is on a clickable rect.
    hit_open: bool,
}

pub type StripState = Mutex<Strip>;

/// Bumped by every move or resize of the window; only the latest one is acted on.
static MOVES: AtomicU64 = AtomicU64::new(0);
/// Our drag is running: the window's moves are its own.
static DRAGGING: AtomicBool = AtomicBool::new(false);
/// How often the drag follows the cursor.
const DRAG_TICK: Duration = Duration::from_millis(6);
const SETTLE_AFTER: Duration = Duration::from_millis(250);
const HOVER_POLL: Duration = Duration::from_millis(66);
/// Shown even if the overlay never reports its height.
const SHOW_ANYWAY: Duration = Duration::from_secs(3);
const START_HEIGHT: f64 = 120.0;

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn displays(app: &AppHandle) -> (Vec<Display>, usize) {
    let Some(w) = main_window(app) else { return (Vec::new(), 0) };
    let list: Vec<Display> = w.available_monitors().unwrap_or_default().iter().map(Display::from_monitor).collect();
    let primary = w
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| Display::from_monitor(&m))
        .and_then(|p| list.iter().position(|d| d.key == p.key && d.bounds == p.bounds))
        .unwrap_or(0);
    (list, primary)
}

/// Place the (still hidden) window, apply the lock state, start the cursor poll.
pub fn init(app: &AppHandle) {
    let path = app.path().app_config_dir().ok().map(|d| d.join("placement.json"));
    let stored = path.as_deref().and_then(Store::load);
    let first_run = stored.is_none();
    let store = stored.unwrap_or_default();
    let (list, primary) = displays(app);
    let Some(fallback) = list.get(primary).cloned() else {
        warn!("placement: no displays reported; leaving the window where it is");
        return;
    };
    let (i, placement) = store.choose(&list, primary);
    let display = list.get(i).cloned().unwrap_or(fallback);
    info!(
        "placement: {} on {} ({:?}, gap {:.0}, centre {:.2}, width {:.0}){}",
        if first_run { "first run, default spot" } else { "restored" },
        display.key,
        placement.anchor,
        placement.gap,
        placement.center,
        placement.width,
        if first_run { ", opening in Move mode" } else { "" }
    );
    app.manage(StripState::new(Strip {
        path,
        store,
        display,
        placement,
        height: START_HEIGHT,
        room: Room::default(),
        told: placement.anchor,
        locked: !first_run,
        first_run,
        shown: false,
        bounds: Rect::default(),
        strip: Rect::default(),
        hits: Vec::new(),
        hover: Hover::default(),
        hit_open: false,
    }));
    // No native shadow: it would box the transparent room, and it adds an
    // invisible frame (`set_bounds` measures one anyway). The overlay draws its own outline.
    if let Some(w) = main_window(app) {
        keep_layered(&w);
        set_click_through(&w, !first_run);
        let _ = w.set_shadow(false);
    }
    apply(app);
    let poll = app.clone();
    tauri::async_runtime::spawn(async move { poll_hover(poll).await });
    let late = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_ANYWAY).await;
        show_once(&late);
    });
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut Strip) -> T) -> Option<T> {
    let state = app.try_state::<StripState>()?;
    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
    Some(f(&mut s))
}

/// Move and size the window to the placement, in one step.
fn apply(app: &AppHandle) {
    let Some(bounds) = with(app, |s| {
        let strip = s.placement.rect(s.height, s.display.work_logical());
        let k = s.display.scale;
        s.strip = strip.scaled(k).round();
        let r = window_of(strip, s.room).scaled(k).round();
        let changed = r != s.bounds;
        s.bounds = r;
        changed.then_some(r)
    }) else {
        return;
    };
    if let (Some(r), Some(w)) = (bounds, main_window(app)) {
        set_bounds(&w, r);
    }
}

/// Put the window's client area (what shows) at `r`, with one SetWindowPos:
/// two separate calls land a frame apart, and the strip visibly jumps in
/// between. The window rect is bigger than what shows by an invisible frame
/// (DWM's shadow and resize border, which comes and goes with the shadow),
/// so it is measured each time.
#[cfg(windows)]
fn set_bounds(w: &WebviewWindow, r: Rect) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    let Ok(hwnd) = w.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let _ = w.run_on_main_thread(move || unsafe {
        let hwnd = hwnd as _;
        let (mut outer, mut client) = (std::mem::zeroed::<RECT>(), std::mem::zeroed::<RECT>());
        let mut origin = POINT { x: 0, y: 0 };
        if GetWindowRect(hwnd, &mut outer) == 0 || GetClientRect(hwnd, &mut client) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
            return;
        }
        let (left, top) = (origin.x - outer.left, origin.y - outer.top);
        let right = (outer.right - outer.left) - client.right - left;
        let bottom = (outer.bottom - outer.top) - client.bottom - top;
        let (x, y, cw, ch) = (r.x as i32, r.y as i32, r.w as i32, r.h as i32);
        SetWindowPos(hwnd, std::ptr::null_mut(), x - left, y - top, cw + left + right, ch + top + bottom, SWP_NOZORDER | SWP_NOACTIVATE);
    });
}

#[cfg(not(windows))]
fn set_bounds(w: &WebviewWindow, r: Rect) {
    use tauri::{PhysicalPosition, PhysicalSize};
    let _ = w.set_size(PhysicalSize::new(r.w as u32, r.h as u32)); // inner size
    let _ = w.set_position(PhysicalPosition::new(r.x as i32, r.y as i32));
}

/// Click-through on or off: tao adds or removes WS_EX_TRANSPARENT (and
/// WS_EX_LAYERED with it, but `keep_layered` holds that one on).
fn set_click_through(w: &WebviewWindow, on: bool) {
    let _ = w.set_ignore_cursor_events(on);
}

/// Keep the overlay a layered window for good, from before it is first shown.
///
/// tao's click-through adds WS_EX_TRANSPARENT | WS_EX_LAYERED and turning it
/// off drops both, rewriting the whole extended style. Dropping WS_EX_LAYERED
/// moves the window onto a fresh plain surface, and the first time that
/// happens after start (the first Unlock) the room around the strip could
/// show solid white until something else repainted it (user report; seen
/// here: 39% of the window white after a fresh start's first unlock, and a
/// repaint plus re-applied blur-behind didn't clear it). Locked mode already
/// draws correctly as a layered window, and a layered window with no layered
/// attributes still takes clicks (checked with WindowFromPoint on the strip,
/// the toolbar and the room). So the window never leaves that state: a
/// subclass adds WS_EX_LAYERED back to every extended-style change, whoever
/// makes it, and click-through is only WS_EX_TRANSPARENT coming and going.
#[cfg(windows)]
fn keep_layered(w: &WebviewWindow) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, STYLESTRUCT, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_LAYERED};
    const ID: usize = 0x4c59_5259; // "LYRY"

    unsafe extern "system" fn hold(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        if msg == WM_STYLECHANGING && wparam as i32 == GWL_EXSTYLE {
            // SAFETY: for WM_STYLECHANGING, lparam points to the STYLESTRUCT
            // Windows is about to apply, valid and writable for this call.
            (*(lparam as *mut STYLESTRUCT)).styleNew |= WS_EX_LAYERED;
        }
        if msg == WM_NCDESTROY {
            // The window is going away: take the subclass off it, as the docs ask.
            RemoveWindowSubclass(hwnd, Some(hold), ID);
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    let Ok(hwnd) = w.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    // SAFETY: hwnd is the live overlay window, and this runs on its own (main)
    // thread, which SetWindowSubclass requires.
    let _ = w.run_on_main_thread(move || unsafe {
        let hwnd = hwnd as HWND;
        SetWindowSubclass(hwnd, Some(hold), ID, 0);
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongW(hwnd, GWL_EXSTYLE, (ex | WS_EX_LAYERED) as i32);
    });
}

#[cfg(not(windows))]
fn keep_layered(_: &WebviewWindow) {}

/// Show the window the first time it is asked to, on the main thread, so it
/// comes after the `set_bounds` already queued there: never at the wrong spot.
fn show_once(app: &AppHandle) {
    if with(app, |s| std::mem::replace(&mut s.shown, true)) == Some(false) {
        if let Some(w) = main_window(app) {
            let shown = w.clone();
            let _ = w.run_on_main_thread(move || {
                let _ = shown.show();
            });
        }
    }
}

/// The overlay's strip height and the room it wants above and below it
/// (logical px): the strip keeps its anchored edge. The first call also shows
/// the window, already in place.
pub fn set_frame(app: &AppHandle, height: f64, room: Room) {
    let changed = with(app, |s| {
        let changed = (s.height - height).abs() >= 1.0 || s.room != room;
        s.height = height;
        s.room = room;
        changed
    });
    if changed == Some(true) {
        apply(app);
    }
    show_once(app);
}

/// Move mode's width grips: the strip widens about its centre. `done` (the
/// grip let go) saves it.
pub fn set_width(app: &AppHandle, width: f64, done: bool) {
    with(app, |s| {
        let work = s.display.work_logical();
        s.placement.width = width.clamp(MIN_WIDTH.min(work.w), work.w);
        if done && !s.first_run {
            s.store.displays.insert(s.display.key.clone(), s.placement);
            if let Some(path) = &s.path {
                s.store.save(path);
            }
        }
    });
    apply(app);
}

pub fn locked(app: &AppHandle) -> bool {
    with(app, |s| s.locked).unwrap_or(true)
}

/// Lock (click-through) or unlock (Move mode). Unlocking focuses the strip so Esc can lock it again.
pub fn set_locked(app: &AppHandle, locked: bool) {
    let Some(w) = main_window(app) else { return };
    let save = with(app, |s| {
        s.locked = locked;
        s.hit_open = false;
        s.hover = Hover::default();
        // Locking once is what "placed" means: no first-run Move mode next launch.
        let save = locked && s.first_run;
        if save {
            s.first_run = false;
            let key = s.display.key.clone();
            s.store.displays.insert(key.clone(), s.placement);
            s.store.last = Some(key);
        }
        save.then(|| (s.path.clone(), s.store.clone()))
    });
    if let Some((Some(path), store)) = save.flatten() {
        store.save(&path);
    }
    set_click_through(&w, locked);
    if !locked {
        show_once(app);
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("strip-locked", locked);
    info!("strip {}", if locked { "locked" } else { "unlocked (Move mode)" });
}

pub fn toggle_locked(app: &AppHandle) -> bool {
    let locked = !locked(app);
    set_locked(app, locked);
    locked
}

/// `strip-anchor`, once per change: the overlay puts its toolbar on the other side.
fn tell_anchor(app: &AppHandle, anchor: Edge) {
    if with(app, |s| std::mem::replace(&mut s.told, anchor) != anchor) == Some(true) {
        let _ = app.emit("strip-anchor", anchor);
    }
}

/// Back to the default spot on the current display.
pub fn reset(app: &AppHandle) {
    let anchor = with(app, |s| {
        s.placement = Placement::default_for(s.display.work_logical());
        s.store.displays.insert(s.display.key.clone(), s.placement);
        s.store.last = Some(s.display.key.clone());
        if let Some(path) = &s.path {
            s.store.save(path);
        }
        s.placement.anchor
    });
    apply(app);
    if let Some(anchor) = anchor {
        tell_anchor(app, anchor);
    }
}

/// The window moved or resized. Unlocked, the user did it: once it has rested
/// (and the mouse button is up), snap it, anchor it to the nearer edge and
/// save. Locked, the user can't move it, so once it has rested it is either
/// where we put it or the system moved it (`put_back`). Waiting out the rest
/// also lets our own queued moves land before anything is compared.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" || !matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
        return;
    }
    let app = window.app_handle().clone();
    if DRAGGING.load(Ordering::SeqCst) {
        return; // our drag settles as it goes
    }
    let moved_by_user = !locked(&app);
    // While it is still moving, the overlay follows: the toolbar swaps sides as
    // soon as the strip crosses the middle, not after the drop.
    if moved_by_user {
        if let Some((_, (_, p))) = placement_now(&app) {
            tell_anchor(&app, p.anchor);
        }
    }
    let mine = MOVES.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE_AFTER).await;
        while mouse_down() {
            if MOVES.load(Ordering::SeqCst) != mine {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if MOVES.load(Ordering::SeqCst) != mine {
            return;
        }
        if moved_by_user {
            settle(&app);
        } else if locked(&app) {
            put_back(&app);
        }
    });
}

/// A locked window that isn't where we put it was moved by the system: after
/// sleep, a lock or a display change, Windows pulls a window whose top hangs
/// off-screen (the room above a top-anchored strip) back onto the screen. The
/// cursor poll and the hit rects still go by `bounds`, so the strip would draw
/// in one place and take hover and the Unlock click in another. Put it back,
/// on its display if that display is still there.
fn put_back(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    if w.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(pos), Ok(size)) = (w.inner_position(), w.inner_size()) else { return };
    let now = Rect { x: pos.x as f64, y: pos.y as f64, w: size.width as f64, h: size.height as f64 };
    let (list, primary) = displays(app);
    let moved = with(app, |s| {
        if now == s.bounds || list.is_empty() {
            return None;
        }
        if !list.contains(&s.display) {
            let (i, placement) = s.store.choose(&list, primary);
            s.display = list[i.min(list.len() - 1)].clone();
            s.placement = placement;
        }
        s.bounds = Rect::default(); // stale: make `apply` move it
        Some((now, s.display.key.clone(), s.placement.anchor))
    })
    .flatten();
    let Some((now, key, anchor)) = moved else { return };
    info!("placement: the window was moved to {now:?} by the system; putting it back on {key}");
    apply(app);
    tell_anchor(app, anchor);
}

/// The display under a window (physical px) and the placement its strip would settle to.
fn settle_at(list: &[Display], window: Rect, room: Room) -> Option<(Display, Placement)> {
    let display = list.get(display_at(list, window.x + window.w / 2.0, window.y + window.h / 2.0)?)?.clone();
    let strip = strip_of(window.scaled(1.0 / display.scale), room);
    let placement = Placement::settle(strip, display.work_logical());
    Some((display, placement))
}

/// Where the window is now (its client area, physical px), the display under
/// it, and the placement that spot would settle to.
fn placement_now(app: &AppHandle) -> Option<(Rect, (Display, Placement))> {
    let w = main_window(app)?;
    // The client area; the strip is inside it, and a placement describes the strip.
    let (Ok(pos), Ok(size)) = (w.inner_position(), w.inner_size()) else { return None };
    let now = Rect { x: pos.x as f64, y: pos.y as f64, w: size.width as f64, h: size.height as f64 };
    let room = with(app, |s| s.room)?;
    let (list, _) = displays(app);
    Some((now, settle_at(&list, now, room)?))
}

/// Move mode's drag, from a press on the strip until the button is let go.
///
/// Windows' own drag keeps a window's top on screen, so the transparent room
/// above the strip would hold the strip itself that far off the top edge. This
/// one moves the strip: each tick it goes where the drop would settle it
/// (flush to an edge or on the centre line when near, inside the work area),
/// with the room free to hang off-screen. What you see while dragging is where
/// it lands.
pub fn drag(app: &AppHandle) {
    // Elsewhere there's no button state to follow (`mouse_down` is Windows
    // only): the system drags the window, and `on_window_event` settles it.
    #[cfg(not(windows))]
    if !locked(app) {
        if let Some(w) = main_window(app) {
            let _ = w.start_dragging();
        }
        return;
    }
    if locked(app) || DRAGGING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        follow_cursor(&app);
        DRAGGING.store(false, Ordering::SeqCst);
        commit(&app);
    });
}

fn follow_cursor(app: &AppHandle) {
    let (Some(start), Some((origin, room))) = (cursor(app), with(app, |s| (s.bounds, s.room))) else { return };
    let (list, _) = displays(app);
    while mouse_down() {
        if let Some((x, y)) = cursor(app) {
            let window = Rect { x: origin.x + x - start.0, y: origin.y + y - start.1, ..origin };
            if let Some((display, placement)) = settle_at(&list, window, room) {
                with(app, |s| {
                    s.display = display;
                    s.placement = placement;
                });
                apply(app);
                tell_anchor(app, placement.anchor);
            }
        }
        std::thread::sleep(DRAG_TICK);
    }
}

/// Save where the strip is now (unless it is still waiting for its first lock).
fn commit(app: &AppHandle) {
    let Some((key, p)) = with(app, |s| {
        s.store.displays.insert(s.display.key.clone(), s.placement);
        s.store.last = Some(s.display.key.clone());
        if !s.first_run {
            if let Some(path) = &s.path {
                s.store.save(path);
            }
        }
        (s.display.key.clone(), s.placement)
    }) else {
        return;
    };
    info!("placement: dropped on {} ({:?}, gap {:.0}, centre {:.2}, width {:.0})", key, p.anchor, p.gap, p.center, p.width);
}

/// The cursor, physical px.
#[cfg(windows)]
fn cursor(_: &AppHandle) -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x as f64, p.y as f64))
}

#[cfg(not(windows))]
fn cursor(app: &AppHandle) -> Option<(f64, f64)> {
    app.cursor_position().ok().map(|c| (c.x, c.y))
}

fn settle(app: &AppHandle) {
    let Some((now, (display, placement))) = placement_now(app) else { return };
    let key = display.key.clone();
    let moved = with(app, |s| {
        s.bounds = now; // where it really is, so `apply` snaps it even when the placement didn't change
        if s.placement.same(&placement) && s.display == display {
            return None;
        }
        s.display = display;
        s.placement = placement;
        s.store.displays.insert(s.display.key.clone(), placement);
        s.store.last = Some(s.display.key.clone());
        if !s.first_run {
            if let Some(path) = &s.path {
                s.store.save(path);
            }
        }
        Some(())
    });
    let Some(()) = moved.flatten() else {
        apply(app);
        tell_anchor(app, placement.anchor);
        return;
    };
    info!(
        "placement: settled on {} ({:?}, gap {:.0}, centre {:.2}, width {:.0})",
        key, placement.anchor, placement.gap, placement.center, placement.width
    );
    apply(app); // the snap
    tell_anchor(app, placement.anchor);
}

#[cfg(windows)]
fn mouse_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    // Physical buttons: either one, so swapped (left-handed) buttons count too.
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) | GetAsyncKeyState(VK_RBUTTON as i32)) as u16 & 0x8000 != 0 }
}

#[cfg(not(windows))]
fn mouse_down() -> bool {
    false
}

/// While locked, follow the cursor ~15 times a second: tell the overlay when
/// it is over the strip (and which reported rect), and drop click-through
/// only over a clickable rect.
async fn poll_hover(app: AppHandle) {
    let mut tick = tokio::time::interval(HOVER_POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        if locked(&app) {
            if let Ok(c) = app.cursor_position() {
                track_cursor(&app, c.x, c.y);
            }
        }
    }
}

fn track_cursor(app: &AppHandle, x: f64, y: f64) {
    let Some((hover, open)) = with(app, |s| {
        let (lx, ly) = ((x - s.bounds.x) / s.display.scale, (y - s.bounds.y) / s.display.scale);
        let in_window = s.bounds.contains(x, y);
        let hit = s.hits.iter().find(|h| in_window && Rect { x: h.x, y: h.y, w: h.w, h: h.h }.contains(lx, ly));
        // Over the strip, or over a reported rect beside it (the Unlock pill straddles its edge).
        let inside = s.strip.contains(x, y) || hit.is_some();
        let hover = Hover { inside, target: hit.map(|h| h.id.clone()) };
        let open = hit.is_some_and(|h| h.clickable);
        let hover = (hover != s.hover).then(|| {
            s.hover = hover.clone();
            hover
        });
        let open = (open != s.hit_open).then(|| {
            s.hit_open = open;
            open
        });
        (hover, open)
    }) else {
        return;
    };
    if let (Some(open), Some(w)) = (open, main_window(app)) {
        set_click_through(&w, !open);
    }
    if let Some(h) = hover {
        let _ = app.emit("strip-hover", h);
    }
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripView {
    locked: bool,
    anchor: Edge,
    first_run: bool,
}

#[tauri::command]
pub fn strip_state(app: AppHandle) -> Result<StripView, String> {
    with(&app, |s| StripView { locked: s.locked, anchor: s.told, first_run: s.first_run })
        .ok_or_else(|| "strip not placed yet".into())
}

#[tauri::command]
pub async fn strip_set_locked(app: AppHandle, locked: bool) {
    set_locked(&app, locked);
}

/// The overlay's hit rects (the Unlock icon, the status icon), replacing the last set.
#[tauri::command]
pub fn strip_hits(app: AppHandle, hits: Vec<Hit>) {
    with(&app, |s| s.hits = hits);
}

/// The strip's height (CSS px) and the room the overlay wants around it.
#[tauri::command]
pub async fn strip_set_frame(app: AppHandle, height: f64, room: Room) {
    let room = Room { x: room.x.clamp(0.0, 40.0), y: room.y.clamp(0.0, 200.0) };
    set_frame(&app, height.clamp(40.0, 600.0), room);
}

/// A press on the strip in Move mode: drag it until the button is let go.
#[tauri::command]
pub fn strip_drag(app: AppHandle) {
    drag(&app);
}

#[tauri::command]
pub async fn strip_set_width(app: AppHandle, width: f64, done: bool) {
    set_width(&app, width, done);
}

#[tauri::command]
pub async fn strip_reset(app: AppHandle) {
    reset(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920×1080 with a 48 px taskbar.
    const WORK: Rect = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1032.0 };

    fn display(key: &str, x: f64) -> Display {
        let bounds = Rect { x, y: 0.0, w: 1920.0, h: 1080.0 };
        Display { key: key.into(), scale: 1.0, bounds, work: Rect { h: 1032.0, ..bounds } }
    }

    #[test]
    fn default_is_bottom_centre_above_the_taskbar() {
        let p = Placement::default_for(WORK);
        let r = p.rect(135.0, WORK);
        assert_eq!(r, Rect { x: 410.0, y: 1032.0 - 24.0 - 135.0, w: 1100.0, h: 135.0 });
        // 60% on a small screen, but never under the minimum width.
        assert_eq!(Placement::default_for(Rect { w: 1280.0, ..WORK }).width, 768.0);
        assert_eq!(Placement::default_for(Rect { w: 700.0, ..WORK }).width, MIN_WIDTH);
    }

    #[test]
    fn growing_keeps_the_anchored_edge() {
        let bottom = Placement::default_for(WORK);
        let (a, b) = (bottom.rect(94.0, WORK), bottom.rect(152.0, WORK));
        assert_eq!(a.y + a.h, b.y + b.h, "bottom strip grows up");
        let top = Placement { anchor: Edge::Top, gap: 0.0, ..bottom };
        assert_eq!(top.rect(94.0, WORK).y, top.rect(152.0, WORK).y, "top strip grows down");
    }

    #[test]
    fn rect_stays_inside_the_work_area() {
        let p = Placement { anchor: Edge::Bottom, gap: 5000.0, center: 0.99, width: 3000.0 };
        let r = p.rect(2000.0, WORK);
        assert_eq!(r, WORK);
        let p = Placement { anchor: Edge::Top, gap: 10.0, center: 0.0, width: 600.0 };
        assert_eq!(p.rect(100.0, WORK).x, 0.0);
    }

    #[test]
    fn settle_anchors_to_the_nearer_edge_and_snaps() {
        // Dropped 20 px above the taskbar, 10 px right of centre: snaps flush, and centred.
        let r = Rect { x: 420.0, y: 1032.0 - 20.0 - 120.0, w: 1100.0, h: 120.0 };
        let p = Placement::settle(r, WORK);
        assert_eq!(p, Placement { anchor: Edge::Bottom, gap: 0.0, center: 0.5, width: 1100.0 });
        assert_eq!(p.rect(120.0, WORK).y + 120.0, 1032.0, "touching the taskbar");
        // Dropped touching the edge stays touching it.
        assert_eq!(Placement::settle(Rect { y: 1032.0 - 120.0, ..r }, WORK).gap, 0.0);
        // Near the top: anchored there, flush.
        let p = Placement::settle(Rect { y: 3.0, ..r }, WORK);
        assert_eq!((p.anchor, p.gap), (Edge::Top, 0.0));
        assert_eq!(p.rect(120.0, WORK).y, 0.0);
        // Dragged partly off the bottom: back inside, flush.
        assert_eq!(Placement::settle(Rect { y: 1000.0, ..r }, WORK).gap, 0.0);
        // Further than the snap distance: left where it was dropped.
        assert_eq!(Placement::settle(Rect { y: 1032.0 - 40.0 - 120.0, ..r }, WORK).gap, 40.0);
    }

    #[test]
    fn settle_keeps_a_deliberate_spot() {
        let r = Rect { x: 100.0, y: 300.0, w: 600.0, h: 120.0 };
        let p = Placement::settle(r, WORK);
        assert_eq!(p.anchor, Edge::Top);
        assert_eq!(p.gap, 300.0);
        assert!((p.center - 400.0 / 1920.0).abs() < 1e-9);
        // And putting it back gives the same rect: settling is stable.
        assert_eq!(p.rect(120.0, WORK), r);
        assert!(Placement::settle(p.rect(120.0, WORK), WORK).same(&p));
    }

    #[test]
    fn settle_on_a_second_display_uses_its_work_area() {
        let work = Rect { x: 1920.0, ..WORK };
        let p = Placement::settle(Rect { x: 1920.0 + 410.0, y: 1032.0 - 60.0 - 135.0, w: 1100.0, h: 135.0 }, work);
        assert_eq!(p, Placement { gap: 60.0, ..Placement::default_for(work) });
    }

    #[test]
    fn the_window_adds_room_on_both_sides_and_gives_it_back() {
        let strip = Rect { x: 410.0, y: 900.0, w: 1100.0, h: 72.0 };
        let room = Room { x: 4.0, y: 60.0 };
        let w = window_of(strip, room);
        assert_eq!(w, Rect { x: 406.0, y: 840.0, w: 1108.0, h: 192.0 });
        assert_eq!(strip_of(w, room), strip);
        // Settling the window of a placed strip finds the same placement, either anchor.
        for anchor in [Edge::Bottom, Edge::Top] {
            let p = Placement { anchor, gap: 0.0, center: 0.5, width: 1100.0 };
            let w = window_of(p.rect(72.0, WORK), room);
            assert!(Placement::settle(strip_of(w, room), WORK).same(&p));
        }
    }

    #[test]
    fn a_drag_past_an_edge_puts_the_strip_flush_against_it() {
        let room = Room { x: 4.0, y: 60.0 };
        let display = display("d", 0.0);
        // The window dragged so far up that the strip itself is above the screen.
        let window = window_of(Rect { x: 410.0, y: -30.0, w: 1100.0, h: 72.0 }, room);
        let (_, p) = settle_at(&[display.clone()], window, room).unwrap();
        assert_eq!((p.anchor, p.gap), (Edge::Top, 0.0));
        assert_eq!(p.rect(72.0, WORK).y, WORK.y); // the strip touches the top; its room hangs off-screen
        // And down past the taskbar.
        let window = window_of(Rect { x: 410.0, y: WORK.y + WORK.h - 40.0, w: 1100.0, h: 72.0 }, room);
        let (_, p) = settle_at(&[display], window, room).unwrap();
        assert_eq!((p.anchor, p.gap), (Edge::Bottom, 0.0));
        let r = p.rect(72.0, WORK);
        assert_eq!(r.y + r.h, WORK.y + WORK.h);
    }

    #[test]
    fn display_key_names_monitor_resolution_and_scale() {
        assert_eq!(display_key(Some(r"\\.\DISPLAY1"), 2560, 1440, 1.5), r"\\.\DISPLAY1|2560x1440|150%");
        assert_eq!(display_key(None, 1920, 1080, 1.0), "display|1920x1080|100%");
        assert_ne!(display_key(Some("A"), 1920, 1080, 1.0), display_key(Some("A"), 1920, 1080, 1.25));
    }

    #[test]
    fn display_at_finds_the_display_under_a_point_or_the_nearest() {
        let list = [display("left", 0.0), display("right", 1920.0)];
        assert_eq!(display_at(&list, 100.0, 100.0), Some(0));
        assert_eq!(display_at(&list, 2000.0, 100.0), Some(1));
        assert_eq!(display_at(&list, 5000.0, 100.0), Some(1));
        assert_eq!(display_at(&[], 0.0, 0.0), None);
    }

    #[test]
    fn choose_prefers_last_then_primary_then_any_saved() {
        let list = [display("a", 0.0), display("b", 1920.0)];
        let top = Placement { anchor: Edge::Top, gap: 16.0, center: 0.5, width: 800.0 };
        let mut store = Store::default();
        // Nothing saved: the primary, default spot.
        assert_eq!(store.choose(&list, 1), (1, Placement::default_for(list[1].work_logical())));
        // Something saved on a connected display beats the default.
        store.displays.insert("a".into(), top);
        assert_eq!(store.choose(&list, 1), (0, top));
        // The last display used wins when it is connected...
        let bottom = Placement { anchor: Edge::Bottom, ..top };
        store.displays.insert("b".into(), bottom);
        store.last = Some("a".into());
        assert_eq!(store.choose(&list, 1), (0, top));
        // ...and when it isn't, the primary's own.
        store.last = Some("gone".into());
        assert_eq!(store.choose(&list, 1), (1, bottom));
    }

    #[test]
    fn store_round_trips_and_tolerates_missing_fields() {
        let mut store = Store { last: Some("a".into()), ..Store::default() };
        store.displays.insert("a".into(), Placement::default_for(WORK));
        let json = serde_json::to_string(&store).unwrap();
        assert!(json.contains(r#""anchor":"bottom""#));
        assert_eq!(serde_json::from_str::<Store>(&json).unwrap(), store);
        assert_eq!(serde_json::from_str::<Store>("{}").unwrap(), Store::default());
    }
}
