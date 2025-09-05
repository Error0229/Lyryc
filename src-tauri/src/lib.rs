use log::{error, info};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

// Module declarations
mod commands;
mod lyrics;
mod track_cleaning;
mod types;
mod websocket;
mod websocket_commands;
mod window_management;

// Import types and functions from modules
use commands::{clean_track_name_command, get_current_track, set_current_track};
use lyrics::{fetch_lrclib_raw, fetch_lyrics};
use types::{ClickThroughState, TrackState, WebSocketState};
use websocket_commands::{
    control_playback, debug_websocket_server, get_websocket_clients_count, get_websocket_status,
    init_extension_connection, send_playback_command,
};
use window_management::{
    enable_drag_mode, initialize_window_sizing, minimize_to_tray, quit_app, restore_from_tray,
    toggle_window_visibility,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("Starting Lyryc application...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                // Define shortcuts
                let ctrl_shift_d =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
                let ctrl_shift_m =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);

                // Get state for the handlers
                let click_through_state = app.state::<ClickThroughState>().inner().clone();

                // Initialize global shortcut plugin with handler
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if event.state() == ShortcutState::Pressed {
                                if shortcut == &ctrl_shift_d {
                                    info!("Global shortcut Ctrl+Shift+D triggered - toggling click-through");
                                    let app_handle = app.clone();
                                    let click_through_state = click_through_state.clone();

                                    tauri::async_runtime::spawn(async move {
                                        // Get window
                                        if let Some(window) = app_handle.get_webview_window("main") {
                                            // Toggle click-through state
                                            let current_state = {
                                                let mut state = click_through_state.lock().await;
                                                *state = !*state; // Toggle the state
                                                *state
                                            };

                                            // Apply the new state to the window
                                            if let Err(e) = window.set_ignore_cursor_events(current_state) {
                                                error!("Failed to set click-through to {}: {}", current_state, e);
                                                return;
                                            }

                                            // Emit event to frontend with new state
                                            let event_name = if current_state { "click-through-enabled" } else { "click-through-disabled" };
                                            if let Err(e) = app_handle.emit(event_name, current_state) {
                                                error!("Failed to emit {} event: {}", event_name, e);
                                                return;
                                            }

                                            info!("Click-through toggled to: {} ({})",
                                                  if current_state { "enabled" } else { "disabled" },
                                                  if current_state { "click-through active" } else { "draggable/interactive" });
                                        } else {
                                            error!("Main window not found for global shortcut handler");
                                        }
                                    });
                                } else if shortcut == &ctrl_shift_m {
                                    info!("Global shortcut Ctrl+Shift+M triggered - toggling window visibility");
                                    let app_handle = app.clone();

                                    tauri::async_runtime::spawn(async move {
                                        if let Err(e) = toggle_window_visibility(app_handle).await {
                                            error!("Failed to toggle window visibility: {}", e);
                                        }
                                    });
                                }
                            }
                        })
                        .build(),
                )?;

                // Register the shortcuts
                app.global_shortcut().register(ctrl_shift_d)?;
                app.global_shortcut().register(ctrl_shift_m)?;

                info!("Global shortcuts registered: Ctrl+Shift+D (toggle click-through), Ctrl+Shift+M (minimize/restore)");

                // Setup system tray
                let _tray = TrayIconBuilder::with_id("lyryc-tray")
                    .tooltip("Lyryc - Clean Lyric Viewer")
                    .icon(app.default_window_icon().unwrap().clone())
                    .on_tray_icon_event(|tray, event| match event {
                        TrayIconEvent::Click { .. } => {
                            info!("Tray icon clicked - toggling window visibility");
                            let app_handle = tray.app_handle().clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = toggle_window_visibility(app_handle).await {
                                    error!("Failed to toggle window from tray click: {}", e);
                                }
                            });
                        }
                        _ => {}
                    })
                    .build(app)?;

                info!("System tray initialized in backend");
            }
            Ok(())
        })
        .manage(TrackState::new(Mutex::new(None)))
        .manage(WebSocketState::new(Mutex::new(None)))
        .manage(ClickThroughState::new(Mutex::new(true))) // Start with click-through enabled
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            set_current_track,
            clean_track_name_command,
            fetch_lyrics,
            fetch_lrclib_raw,
            init_extension_connection,
            get_websocket_status,
            get_websocket_clients_count,
            debug_websocket_server,
            control_playback,
            send_playback_command,
            initialize_window_sizing,
            minimize_to_tray,
            restore_from_tray,
            toggle_window_visibility,
            quit_app,
            enable_drag_mode
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
