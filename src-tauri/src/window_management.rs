use log::info;
use tauri::Manager;

#[tauri::command]
pub async fn minimize_to_tray(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Minimizing window to system tray");

    if let Some(window) = app_handle.get_webview_window("main") {
        // Hide the window (already not in taskbar by config)
        window
            .hide()
            .map_err(|e| format!("Failed to hide window: {}", e))?;

        info!("Window hidden and minimized to tray");
        Ok("Window minimized to system tray".to_string())
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub async fn restore_from_tray(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Restoring window from system tray");

    if let Some(window) = app_handle.get_webview_window("main") {
        // Show the window (stays hidden from taskbar by config)
        window
            .show()
            .map_err(|e| format!("Failed to show window: {}", e))?;
        window
            .set_focus()
            .map_err(|e| format!("Failed to focus window: {}", e))?;

        info!("Window restored from tray");
        Ok("Window restored from system tray".to_string())
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub async fn toggle_window_visibility(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Toggling window visibility");

    if let Some(window) = app_handle.get_webview_window("main") {
        let is_visible = window
            .is_visible()
            .map_err(|e| format!("Failed to check visibility: {}", e))?;

        if is_visible {
            // Hide window (stays hidden from taskbar by config)
            window
                .hide()
                .map_err(|e| format!("Failed to hide window: {}", e))?;
            Ok("Window hidden".to_string())
        } else {
            // Show window (stays hidden from taskbar by config)
            window
                .show()
                .map_err(|e| format!("Failed to show window: {}", e))?;
            window
                .set_focus()
                .map_err(|e| format!("Failed to focus window: {}", e))?;
            Ok("Window shown".to_string())
        }
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub async fn quit_app(app_handle: tauri::AppHandle) -> Result<(), String> {
    info!("Quitting application via command");
    app_handle.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Opening settings window");

    // Check if settings window already exists
    if let Some(_) = app_handle.get_webview_window("settings") {
        info!("Settings window already exists, focusing it");
        if let Some(settings_window) = app_handle.get_webview_window("settings") {
            settings_window
                .set_focus()
                .map_err(|e| format!("Failed to focus settings window: {}", e))?;
            settings_window
                .show()
                .map_err(|e| format!("Failed to show settings window: {}", e))?;
        }
        return Ok("Settings window focused".to_string());
    }

    // Created hidden: the page shows it once it has painted in its theme
    // (SettingsApp.tsx), so it never opens on a blank white frame.
    let settings_window = tauri::WebviewWindowBuilder::new(
        &app_handle,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Lyryc Settings")
    // Room for the widest page (AI providers) without crowding; never narrower
    // than the sidebar plus a page that still wraps cleanly.
    .inner_size(960.0, 700.0)
    .min_inner_size(720.0, 500.0)
    .resizable(true)
    .center()
    // Our own title bar (settings/TitleBar.tsx). The shadow keeps the resize
    // border and, on Windows 11, the rounded corners.
    .decorations(false)
    .shadow(true)
    .always_on_top(false)
    .skip_taskbar(false)
    .visible(false)
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

    // If the page never says so (a load error), show it anyway.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2000));
        if matches!(settings_window.is_visible(), Ok(false)) {
            let _ = settings_window.show();
            let _ = settings_window.set_focus();
        }
    });

    info!("Settings window created successfully");
    Ok("Settings window opened".to_string())
}

#[tauri::command]
pub async fn close_settings_window(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Closing settings window");

    if let Some(settings_window) = app_handle.get_webview_window("settings") {
        settings_window
            .close()
            .map_err(|e| format!("Failed to close settings window: {}", e))?;
        info!("Settings window closed successfully");
        Ok("Settings window closed".to_string())
    } else {
        Err("Settings window not found".to_string())
    }
}
