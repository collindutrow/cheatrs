use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// Global state to store the captured process before showing window
static CAPTURED_PROCESS: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheatSheet {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    processes: Vec<String>,
    sections: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppConfig {
    #[serde(default)]
    last_cheatsheet_id: Option<String>,
    #[serde(default)]
    last_cheatsheet_per_process: std::collections::HashMap<String, String>,
    #[serde(default = "default_search_all_for_process")]
    search_all_for_process: bool,
}

fn default_search_all_for_process() -> bool {
    true
}

fn get_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let config_dir = std::env::var("APPDATA")
        .ok()
        .map(|appdata| PathBuf::from(appdata).join("cheatrs"));

    #[cfg(target_os = "macos")]
    let config_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("cheatrs")
    });

    #[cfg(target_os = "linux")]
    let config_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("cheatrs")
    });

    config_dir.map(|dir| {
        let _ = fs::create_dir_all(&dir);
        dir.join("config.json")
    })
}

fn load_config() -> AppConfig {
    if let Some(config_path) = get_config_path() {
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                    return config;
                }
            }
        }
    }
    AppConfig::default()
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path().ok_or("Failed to get config path")?;
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

// Get the active window's process name (Windows-specific)
#[cfg(target_os = "windows")]
fn get_active_process_name() -> Option<String> {
    use windows::Win32::System::ProcessStatus::K32GetModuleFileNameExW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == std::ptr::null_mut() {
            return None;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        )
        .ok()?;

        let mut filename = [0u16; 260];
        let len = K32GetModuleFileNameExW(Some(process_handle), None, &mut filename);

        if len == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&filename[..len as usize]);
        PathBuf::from(path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|s| s.to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn get_active_process_name() -> Option<String> {
    None
}

// Tauri commands
#[tauri::command]
fn get_current_process() -> Option<String> {
    // Return the captured process (the one active before window was shown)
    CAPTURED_PROCESS.lock().unwrap().clone()
}

#[tauri::command]
fn get_config() -> AppConfig {
    load_config()
}

#[tauri::command]
fn update_last_cheatsheet(
    sheet_id: String,
    process_name: Option<String>,
    sheet_processes: Vec<String>,
) -> Result<(), String> {
    let mut config = load_config();
    config.last_cheatsheet_id = Some(sheet_id.clone());

    // Only save per-process preference if the process is actually listed in the sheet's processes
    if let Some(process) = process_name {
        if !sheet_processes.is_empty()
            && sheet_processes
                .iter()
                .any(|p| p.to_lowercase() == process.to_lowercase())
        {
            config.last_cheatsheet_per_process.insert(process, sheet_id);
        }
    }

    save_config(&config)
}

#[tauri::command]
fn toggle_search_all_for_process() -> Result<bool, String> {
    let mut config = load_config();
    config.search_all_for_process = !config.search_all_for_process;
    save_config(&config)?;
    Ok(config.search_all_for_process)
}

#[tauri::command]
fn get_initial_sheet_id() -> Option<String> {
    let config = load_config();

    if let Some(process_name) = get_active_process_name() {
        if let Some(sheet_id) = config.last_cheatsheet_per_process.get(&process_name) {
            return Some(sheet_id.clone());
        }
    }

    config.last_cheatsheet_id
}

#[tauri::command]
fn get_sheet_for_process(process_name: String) -> Option<String> {
    let config = load_config();
    config
        .last_cheatsheet_per_process
        .get(&process_name)
        .cloned()
}

#[tauri::command]
fn set_window_size_from_screen<R: Runtime>(window: tauri::WebviewWindow<R>) -> Result<(), String> {
    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let size = monitor.size();
        let scale = monitor.scale_factor();

        let logical_width = (size.width as f64 / scale) as f64;
        let logical_height = (size.height as f64 / scale) as f64;

        let target_width = (logical_width * 0.8) as u32;
        let target_height = (logical_height * 0.8) as u32;

        window
            .set_size(tauri::Size::Physical(tauri::PhysicalSize {
                width: target_width,
                height: target_height,
            }))
            .map_err(|e| e.to_string())?;

        window.center().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn close_window(window: tauri::Window) {
    window.hide().unwrap();
}

#[tauri::command]
fn get_user_cheatsheets_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base_dir = std::env::var("APPDATA")
        .ok()
        .map(|appdata| PathBuf::from(appdata).join("cheatrs"));

    #[cfg(target_os = "macos")]
    let base_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("cheatrs")
    });

    #[cfg(target_os = "linux")]
    let base_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("cheatrs")
    });

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let base_dir: Option<PathBuf> = None;

    base_dir.map(|dir| dir.join("cheatsheets"))
}

#[tauri::command]
fn load_cheatsheets(app: tauri::AppHandle) -> Result<Vec<CheatSheet>, String> {
    let mut sheets = Vec::new();
    let mut dirs_to_search = Vec::new();

    // 1. Dev mode: Check project cheatsheets directory
    let project_dir = std::env::current_dir().ok().and_then(|p| {
        // Try to find the project root by looking for cheatsheets directory
        let mut current = p.clone();
        loop {
            let candidate = current.join("cheatsheets");
            if candidate.exists() {
                return Some(candidate);
            }
            if !current.pop() {
                break;
            }
        }
        None
    });

    if let Some(dev_dir) = project_dir {
        eprintln!(
            "Checking dev/project directory: {:?} (exists: {})",
            dev_dir,
            dev_dir.exists()
        );
        if dev_dir.exists() {
            dirs_to_search.push(dev_dir);
        }
    }

    // 2. Production: Add bundled cheatsheets directory (in resources)
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled_dir = resource_dir.join("cheatsheets");
        eprintln!(
            "Checking bundled directory: {:?} (exists: {})",
            bundled_dir,
            bundled_dir.exists()
        );
        if bundled_dir.exists() {
            dirs_to_search.push(bundled_dir);
        }
    }

    // 3. User data directory - use simpler path without full identifier
    #[cfg(target_os = "windows")]
    let user_dir = std::env::var("APPDATA")
        .ok()
        .map(|appdata| PathBuf::from(appdata).join("cheatrs").join("cheatsheets"));

    #[cfg(target_os = "macos")]
    let user_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("cheatrs")
            .join("cheatsheets")
    });

    #[cfg(target_os = "linux")]
    let user_dir = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("cheatrs")
            .join("cheatsheets")
    });

    if let Some(user_dir) = user_dir {
        eprintln!("Checking user data directory: {:?}", user_dir);
        // Create the directory if it doesn't exist
        let _ = fs::create_dir_all(&user_dir);
        if user_dir.exists() {
            eprintln!("User data directory exists, adding to search list");
            dirs_to_search.push(user_dir);
        }
    }

    eprintln!(
        "Searching {} directories for JSON files",
        dirs_to_search.len()
    );

    // Search all directories for JSON files
    for dir in dirs_to_search {
        eprintln!("Searching directory: {:?}", dir);
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                eprintln!("Found file: {:?}", path);
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    eprintln!("Loading JSON file: {:?}", path);
                    match load_cheatsheet_from_file(&path) {
                        Ok(sheet) => {
                            eprintln!(
                                "Successfully loaded sheet: {} (id: {})",
                                sheet.name, sheet.id
                            );
                            sheets.push(sheet);
                        }
                        Err(e) => eprintln!("Failed to load {:?}: {}", path, e),
                    }
                }
            }
        } else {
            eprintln!("Failed to read directory: {:?}", dir);
        }
    }

    eprintln!("Total sheets loaded: {}", sheets.len());
    Ok(sheets)
}

fn load_cheatsheet_from_file(path: &PathBuf) -> Result<CheatSheet, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let sheet: CheatSheet =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(sheet)
}

use tauri::{
    AppHandle, Emitter, Listener, Manager, Runtime,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;
            #[cfg(desktop)]
            setup_global_shortcut(app)?;
            #[cfg(target_os = "windows")]
            configure_windows_styles(app)?;
            setup_blur_handler(app)?;
            Ok(())
        })
        // Commands
        .invoke_handler(tauri::generate_handler![
            close_window,
            load_cheatsheets,
            get_current_process,
            get_config,
            update_last_cheatsheet,
            toggle_search_all_for_process,
            get_initial_sheet_id,
            get_sheet_for_process,
            set_window_size_from_screen
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Build tray icon and menu: Toggle, Reload, Open Cheatsheets, Quit.
fn setup_tray<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    // Tray menu items
    let toggle_item = MenuItemBuilder::new("Toggle").id("toggle").build(app)?;
    let reload_item = MenuItemBuilder::new("Reload").id("reload").build(app)?;
    let open_cheatsheets_item = MenuItemBuilder::new("Open Cheatsheets Folder")
        .id("open_cheatsheets")
        .build(app)?;
    let quit_item = MenuItemBuilder::new("Quit").id("quit").build(app)?;

    let tray_menu = MenuBuilder::new(app)
        .items(&[
            &toggle_item,
            &reload_item,
            &open_cheatsheets_item,
            &quit_item,
        ])
        .build()?;

    // Load tray icon from src-tauri/icons/tray.png; fall back to default app icon if missing.
    let icon = load_tray_icon(app);

    let mut tray_builder = TrayIconBuilder::new().tooltip("Cheatrs");
    if let Some(icon) = icon {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder
        .menu(&tray_menu)
        .on_menu_event(|app_handle, event| match event.id().as_ref() {
            "toggle" => toggle_main_window_visibility(app_handle),
            "reload" => reload_main_window(app_handle),
            "open_cheatsheets" => open_cheatsheets_folder(app_handle),
            "quit" => app_handle.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Load embedded tray icon.
fn load_tray_icon<R: Runtime>(_app: &tauri::App<R>) -> Option<tauri::image::Image<'static>> {
    use tauri::image::Image;

    const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray.png");

    match Image::from_bytes(TRAY_ICON_BYTES) {
        Ok(img) => Some(img),
        Err(e) => {
            eprintln!("tray icon: failed to load embedded icon: {}", e);
            None
        }
    }
}

/// Register global hotkey (Windows, macOS, Linux) using the plugin.
#[cfg(desktop)]
fn setup_global_shortcut<R: tauri::Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};

    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcuts(["super+slash"])
        .map_err(|e| {
            tauri::Error::PluginInitialization("global-shortcut".to_string(), e.to_string())
        })?
        .with_handler(|app_handle, shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if shortcut.matches(Modifiers::SUPER, Code::Slash) {
                    toggle_main_window_visibility(app_handle);
                }
            }
        })
        .build();

    app.handle().plugin(plugin)?;

    Ok(())
}

/// Toggle main window visibility. Use SW_HIDE semantics under the hood.
fn toggle_main_window_visibility<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide(); // completely hide (SW_HIDE-equivalent)
            }
            Ok(false) | Err(_) => {
                // Capture the active process BEFORE showing the window
                let process = get_active_process_name();
                *CAPTURED_PROCESS.lock().unwrap() = process;

                // Set window size based on screen size before showing
                let _ = set_window_size_from_screen(window.clone());
                let _ = window.show();
                let _ = window.set_focus();

                // Emit event to frontend to update the process
                let _ = window.emit("process-captured", ());
            }
        }
    }
}

/// Reload main window web content (from embedded static assets).
fn reload_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        // Simple approach: reload via JS.
        let _ = window.eval("window.location.reload()");
    }
}

/// Open the cheatsheets folder in the system file explorer.
fn open_cheatsheets_folder<R: Runtime>(
    #[cfg_attr(target_os = "windows", allow(unused_variables))] app: &AppHandle<R>,
) {
    if let Some(dir) = get_user_cheatsheets_dir() {
        // Create the directory if it doesn't exist
        let _ = fs::create_dir_all(&dir);

        // Windows: Use ShellExecuteW to respect file manager replacements
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::Shell::ShellExecuteW;
            use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
            use windows::core::{PCWSTR, w};

            if let Some(dir_str) = dir.to_str() {
                let dir_wide: Vec<u16> = dir_str.encode_utf16().chain(std::iter::once(0)).collect();

                unsafe {
                    let result = ShellExecuteW(
                        None,
                        w!("explore"),
                        PCWSTR::from_raw(dir_wide.as_ptr()),
                        None,
                        None,
                        SW_SHOWNORMAL,
                    );

                    // ShellExecute returns > 32 on success
                    let result_code = result.0 as isize;
                    if result_code <= 32 {
                        eprintln!(
                            "Failed to open folder with ShellExecute: error code {}",
                            result_code
                        );
                    }
                }
            }
        }

        // macOS and Linux: Use Tauri opener plugin
        #[cfg(not(target_os = "windows"))]
        {
            use tauri_plugin_opener::OpenerExt;

            if let Some(dir_str) = dir.to_str() {
                if let Err(e) = app.opener().open_path(dir_str, None::<&str>) {
                    eprintln!("Failed to open cheatsheets folder: {}", e);
                }
            }
        }
    }
}

/// Windows-only: patch extended styles so the window never appears in taskbar or Alt-Tab.
/// Uses WS_EX_TOOLWINDOW instead of WS_EX_APPWINDOW.
#[cfg(target_os = "windows")]
fn configure_windows_styles<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    use tauri::Manager;

    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };

    apply_toolwindow_style(&window);

    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_toolwindow_style<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetWindowLongW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    // `hwnd()` is provided by Tauri's Windows extension traits on desktop.
    // Enable the "windows" feature on tauri if needed.
    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Remove appwindow, add toolwindow so it does not appear in taskbar/Alt-Tab.
        let ex_style = (ex_style & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0;

        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style as i32);

        // Apply the style changes.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

/// Setup blur event handler to hide the window when it loses focus.
fn setup_blur_handler<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.clone().listen("tauri://blur", move |_| {
            // Hide the window when it loses focus
            if let Some(app_window) = window.app_handle().get_webview_window("main") {
                let _ = app_window.hide();
            }
        });
    }
    Ok(())
}
