use log::info;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size};

#[tauri::command]
pub async fn initialize_window_sizing(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Initializing window sizing to fit screen");

    if let Some(window) = app_handle.get_webview_window("main") {
        // Get primary monitor
        let monitor = window
            .primary_monitor()
            .map_err(|e| format!("Failed to get primary monitor: {}", e))?;

        if let Some(monitor) = monitor {
            let size = monitor.size();
            let screen_width = size.width as f64;
            let _screen_height = size.height as f64;

            // Set window to full screen width and minimal height (40px)
            let window_width = screen_width;
            let window_height = 40.0; // Ultra compact height

            // Position at top-left corner
            window
                .set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 }))
                .map_err(|e| format!("Failed to set position: {}", e))?;

            // Set size to full width and minimal height
            window
                .set_size(Size::Physical(PhysicalSize {
                    width: window_width as u32,
                    height: window_height as u32,
                }))
                .map_err(|e| format!("Failed to set size: {}", e))?;

            info!(
                "Window resized to {}x{} at position (0,0)",
                window_width, window_height
            );
            Ok(format!(
                "Window sized to {}x{} at top of screen",
                window_width, window_height
            ))
        } else {
            Err("No primary monitor found".to_string())
        }
    } else {
        Err("Main window not found".to_string())
    }
}

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
pub async fn adjust_window_height(
    app_handle: tauri::AppHandle,
    content_height: f64,
    has_translation: bool,
    has_furigana: bool,
) -> Result<String, String> {
    info!(
        "Adjusting window height - content: {}px, translation: {}, furigana: {}",
        content_height, has_translation, has_furigana
    );

    if let Some(window) = app_handle.get_webview_window("main") {
        let current_size = window
            .inner_size()
            .map_err(|e| format!("Failed to get current size: {}", e))?;

        // Calculate required height with minimal margins
        let mut required_height = content_height;

        // Add minimal base padding for the container
        required_height += 16.0; // 8px top + 8px bottom padding

        // Add minimal extra space for bilingual content
        if has_translation {
            required_height += 8.0; // Minimal spacing between lines
        }

        // Add minimal extra space for furigana rendering
        if has_furigana {
            required_height += 12.0; // Minimal space for furigana above/below
        }

        // Ensure minimum height (reduced)
        let min_height = 40.0;
        required_height = required_height.max(min_height);

        // Ensure maximum height (to prevent too large windows)
        let max_height = 300.0;
        required_height = required_height.min(max_height);

        // Only resize if the height difference is significant (avoid constant micro-adjustments)
        let current_height = current_size.height as f64;
        let height_diff = (required_height - current_height).abs();

        if height_diff > 5.0 {
            let new_size = PhysicalSize {
                width: current_size.width,
                height: required_height as u32,
            };

            window
                .set_size(Size::Physical(new_size))
                .map_err(|e| format!("Failed to resize window: {}", e))?;

            info!(
                "Window height adjusted from {}px to {}px (diff: {:.1}px)",
                current_height, required_height, height_diff
            );
        }

        Ok(format!("Window height set to {:.0}px", required_height))
    } else {
        Err("Main window not found".to_string())
    }
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

    // Create new settings window
    let _settings_window = tauri::WebviewWindowBuilder::new(
        &app_handle,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Lyryc Settings")
    .inner_size(800.0, 600.0)
    .min_inner_size(600.0, 400.0)
    .resizable(true)
    .center()
    .decorations(true)
    .always_on_top(false)
    .skip_taskbar(false)
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

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

#[tauri::command]
pub async fn enable_drag_mode(app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Enabling drag mode - disabling click-through");

    if let Some(window) = app_handle.get_webview_window("main") {
        window
            .set_ignore_cursor_events(false)
            .map_err(|e| format!("Failed to disable click-through: {}", e))?;

        // Emit event to frontend
        app_handle
            .emit("drag-mode-enabled", true)
            .map_err(|e| format!("Failed to emit event: {}", e))?;

        // Auto-disable after 5 seconds
        let app_handle_clone = app_handle.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if let Some(window) = app_handle_clone.get_webview_window("main") {
                let _ = window.set_ignore_cursor_events(true);
                let _ = app_handle_clone.emit("drag-mode-disabled", false);
                info!("Auto-disabled drag mode after 5 seconds");
            }
        });

        Ok("Drag mode enabled for 5 seconds".to_string())
    } else {
        Err("Main window not found".to_string())
    }
}
