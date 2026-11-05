//! The tray icon. Left click shows or hides the lyrics; double click opens
//! Settings; right click opens our own menu (tray.html, a small window styled
//! like the rest of the app) instead of the native one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use log::error;
use serde::Serialize;
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::placement;
use crate::window_management::{open_settings_window, toggle_window_visibility};

const MENU: &str = "tray-menu";

/// Bumped by every left click and double click: a single click acts only if
/// no second click followed within the double-click time.
static CLICKS: AtomicU64 = AtomicU64::new(0);

pub fn init(app: &App) -> tauri::Result<()> {
    create_menu(app.handle())?;
    TrayIconBuilder::with_id("lyryc-tray")
        .icon(tray_icon(app)?)
        .tooltip("Lyryc")
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // Down and Up both arrive as Click: act once, on release.
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => single_click(app),
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    CLICKS.fetch_add(1, Ordering::SeqCst); // not a show/hide after all
                    act(app, "settings");
                }
                TrayIconEvent::Click { button: MouseButton::Right, button_state: MouseButtonState::Up, position, .. } => {
                    open_menu(app, position)
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// Show or hide the lyrics, unless this click turns out to be the first of a
/// double click (which opens Settings): toggling at once would flash the strip.
fn single_click(app: &AppHandle) {
    let mine = CLICKS.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(double_click_time()).await;
        if CLICKS.load(Ordering::SeqCst) == mine {
            act(&app, "visible");
        }
    });
}

#[cfg(windows)]
fn double_click_time() -> Duration {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
    Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() }).clamp(150, 600))
}

#[cfg(not(windows))]
fn double_click_time() -> Duration {
    Duration::from_millis(400)
}

/// The tray draws at 16 px times the display scale. Hand Windows that exact
/// size: scaling one large image down to 16-24 px blurs the glyph.
const TRAY_ICONS: [(f64, &[u8]); 6] = [
    (16.0, include_bytes!("../icons/tray/16.png")),
    (20.0, include_bytes!("../icons/tray/20.png")),
    (24.0, include_bytes!("../icons/tray/24.png")),
    (32.0, include_bytes!("../icons/tray/32.png")),
    (40.0, include_bytes!("../icons/tray/40.png")),
    (48.0, include_bytes!("../icons/tray/48.png")),
];

fn tray_icon(app: &App) -> tauri::Result<Image<'static>> {
    let scale = app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor());
    let want = 16.0 * scale;
    let (_, png) = TRAY_ICONS.iter().find(|(px, _)| *px >= want).unwrap_or(&TRAY_ICONS[5]);
    Image::from_bytes(png)
}

// ---------------------------------------------------------------- the menu

/// Created hidden at startup, so a right click opens it at once, already
/// painted. The page sizes the window to its items. It never takes focus:
/// Windows folds the tray's overflow flyout the moment another window becomes
/// active, which left the menu floating over nothing. A click outside it (or
/// Esc) closes it instead, see `watch_outside`.
fn create_menu(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, MENU, WebviewUrl::App("tray.html".into()))
        .title("Lyryc menu")
        .inner_size(240.0, 260.0)
        .decorations(false)
        // The system shadow gives it Windows 11's rounded corners and menu shadow.
        .shadow(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .build()?;
    never_activate(&w);
    let hide = w.clone();
    w.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            hide_menu(&hide);
        }
    });
    Ok(())
}

fn menu(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MENU)
}

/// Open the menu at the cursor, like a native one: above it and to its right,
/// flipped to stay inside the work area of the display it is on.
fn open_menu(app: &AppHandle, at: PhysicalPosition<f64>) {
    let Some(w) = menu(app) else { return };
    let Ok(size) = w.outer_size() else { return };
    let (mw, mh) = (size.width as f64, size.height as f64);
    let (mut x, mut y) = (at.x, at.y - mh);
    if let Ok(Some(m)) = app.monitor_from_point(at.x, at.y) {
        let a = m.work_area();
        let (left, top) = (a.position.x as f64, a.position.y as f64);
        let (right, bottom) = (left + a.size.width as f64, top + a.size.height as f64);
        if x + mw > right {
            x = at.x - mw;
        }
        if y < top {
            y = at.y;
        }
        x = x.clamp(left, (right - mw).max(left));
        y = y.clamp(top, (bottom - mh).max(top));
    }
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    let _ = w.emit_to(MENU, "tray-menu-open", state(app));
    show_inactive(&w);
    watch_outside(w);
}

/// Mark the menu as a window that clicks never activate (WS_EX_NOACTIVATE).
#[cfg(windows)]
fn never_activate(w: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
    let Ok(hwnd) = w.hwnd() else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd.0 as _, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE as isize);
    }
}

#[cfg(not(windows))]
fn never_activate(_: &WebviewWindow) {}

/// Show without activating, so the window that had focus (or the tray's
/// overflow flyout) keeps it.
#[cfg(windows)]
fn show_inactive(w: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW};
    let Ok(hwnd) = w.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let _ = w.run_on_main_thread(move || unsafe {
        SetWindowPos(hwnd as _, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    });
}

/// Hide the menu the way it was shown, directly: Tauri's own hide only acts
/// on a window it showed itself, and does nothing here.
#[cfg(windows)]
fn hide_menu(w: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    let Ok(hwnd) = w.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let _ = w.run_on_main_thread(move || unsafe {
        ShowWindow(hwnd as _, SW_HIDE);
    });
}

#[cfg(not(windows))]
fn hide_menu(w: &WebviewWindow) {
    let _ = w.hide();
}

#[cfg(not(windows))]
fn show_inactive(w: &WebviewWindow) {
    let _ = w.show();
    let _ = w.set_focus();
}

/// While the menu shows, a press outside it or Esc closes it: a window that
/// never takes focus never hears a blur.
#[cfg(windows)]
fn watch_outside(w: WebviewWindow) {
    use std::sync::atomic::AtomicBool;
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        // The right button that opened the menu may still be down: only a new press counts.
        let mut was_down = true;
        while w.is_visible().unwrap_or(false) {
            let (down, esc, inside) = input_now(&w);
            if esc || (down && !was_down && !inside) {
                hide_menu(&w);
                break;
            }
            was_down = down;
            std::thread::sleep(Duration::from_millis(30));
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn watch_outside(_: WebviewWindow) {}

/// (a mouse button is down, Esc is down, the cursor is over the menu).
#[cfg(windows)]
fn input_now(w: &WebviewWindow) -> (bool, bool, bool) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
    let pressed = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 };
    let down = pressed(VK_LBUTTON) || pressed(VK_RBUTTON) || pressed(VK_MBUTTON);
    let inside = w.hwnd().is_ok_and(|hwnd| unsafe {
        let (mut r, mut p) = (std::mem::zeroed::<RECT>(), POINT { x: 0, y: 0 });
        GetWindowRect(hwnd.0 as _, &mut r) != 0
            && GetCursorPos(&mut p) != 0
            && (r.left..r.right).contains(&p.x)
            && (r.top..r.bottom).contains(&p.y)
    });
    (down, pressed(VK_ESCAPE), inside)
}

/// What the menu's items say: show or hide, lock or move.
#[derive(Clone, Copy, Serialize)]
pub struct MenuState {
    visible: bool,
    locked: bool,
}

fn state(app: &AppHandle) -> MenuState {
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
    MenuState { visible, locked: placement::locked(app) }
}

#[tauri::command]
pub fn tray_menu_state(app: AppHandle) -> MenuState {
    state(&app)
}

/// A menu item was chosen (or "close": Esc). The menu hides first.
#[tauri::command]
pub fn tray_menu_act(app: AppHandle, id: String) {
    if let Some(w) = menu(&app) {
        hide_menu(&w);
    }
    act(&app, &id);
}

/// Run a menu item by id.
fn act(app: &AppHandle, id: &str) {
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let done = match id.as_str() {
            "visible" => toggle_window_visibility(app.clone()).await.map(drop),
            "locked" => {
                placement::toggle_locked(&app);
                Ok(())
            }
            "reset" => {
                placement::reset(&app);
                Ok(())
            }
            "settings" => open_settings_window(app.clone()).await.map(drop),
            "quit" => {
                app.exit(0);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = done {
            error!("Tray '{id}': {e}");
        }
    });
}
