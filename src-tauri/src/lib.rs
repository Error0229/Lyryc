use log::{error, info};
use tauri::Manager;
use tokio::sync::Mutex;

// Module declarations
mod cleaning;
mod commands;
mod furigana;
mod llm;
mod logging;
mod lyrics;
mod lyrics_cache;
mod placement;
mod prealign;
mod sources;
mod timing;
mod translation;
mod tray;
mod types;
mod updater;
mod websocket;
mod websocket_commands;
mod window_management;
mod ytmusic;

// Import types and functions from modules
use commands::{
    clean_track_name_command, clean_track_name_with_config, get_current_track,
    get_supported_languages, set_current_track, translate_lyrics_command,
};
use furigana::generate_furigana_command;
use llm::commands::{
    llm_get_settings, llm_import_legacy_keys, llm_list_models, llm_list_presets, llm_remove_provider,
    llm_set_route, llm_test, llm_upsert_provider,
};
use lyrics::{fetch_lrclib_raw, fetch_lyrics, fetch_lyrics_timed, lookahead_configure, mark_instrumental};
use lyrics_cache::{cache_clear, cache_size};
use prealign::{prealign_configure, prealign_remove_files, prealign_status};
use updater::{update_check, update_install, update_status, UpdaterState};
use types::{TrackState, WebSocketState};
use websocket_commands::{
    control_playback, debug_websocket_server, get_websocket_clients_count, get_websocket_status,
    init_extension_connection, send_playback_command,
};
use placement::{strip_drag, strip_hits, strip_reset, strip_set_frame, strip_set_locked, strip_set_width, strip_state};
use window_management::{
    close_settings_window, minimize_to_tray, open_settings_window, quit_app,
    restore_from_tray, toggle_window_visibility,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load environment variables from .env file
    if let Err(e) = dotenv::dotenv() {
        eprintln!("Could not load .env file: {}. Continuing without it.", e);
    }

    logging::init();

    info!("Starting Lyryc application...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // First: the window is hidden until placed, and the overlay asks for this state as it loads.
            placement::init(app.handle());
            app.manage(llm::store::init(app.path().app_config_dir().ok()));
            prealign::init(app.handle());
            updater::start(app.handle());

            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                let lock = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD);
                let visibility = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if event.state() != ShortcutState::Pressed {
                                return;
                            }
                            if shortcut == &lock {
                                placement::toggle_locked(app);
                                return;
                            }
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = toggle_window_visibility(app).await {
                                    error!("Global shortcut: {}", e);
                                }
                            });
                        })
                        .build(),
                )?;
                app.global_shortcut().register(lock)?;
                app.global_shortcut().register(visibility)?;
                info!("Global shortcuts registered: Ctrl+Shift+D (lock / move), Ctrl+Shift+M (show/hide)");

                tray::init(app)?;
            }
            Ok(())
        })
        .manage(TrackState::new(Mutex::new(None)))
        .manage(WebSocketState::new(Mutex::new(None)))
        .manage(UpdaterState::default())
        .on_window_event(placement::on_window_event)
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            set_current_track,
            clean_track_name_command,
            clean_track_name_with_config,
            translate_lyrics_command,
            get_supported_languages,
            fetch_lyrics,
            fetch_lyrics_timed,
            lookahead_configure,
            mark_instrumental,
            fetch_lrclib_raw,
            init_extension_connection,
            get_websocket_status,
            get_websocket_clients_count,
            debug_websocket_server,
            control_playback,
            send_playback_command,
            strip_state,
            strip_set_frame,
            strip_drag,
            strip_set_width,
            strip_set_locked,
            strip_hits,
            strip_reset,
            tray::tray_menu_state,
            tray::tray_menu_act,
            open_settings_window,
            close_settings_window,
            minimize_to_tray,
            restore_from_tray,
            toggle_window_visibility,
            quit_app,
            generate_furigana_command,
            logging::log_frontend,
            llm_list_presets,
            llm_get_settings,
            llm_upsert_provider,
            llm_remove_provider,
            llm_set_route,
            llm_list_models,
            llm_test,
            llm_import_legacy_keys,
            prealign_configure,
            prealign_status,
            prealign_remove_files,
            cache_size,
            cache_clear,
            update_status,
            update_check,
            update_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
