pub mod models;
pub mod logging;
pub mod parsers;
pub mod keyring;
pub mod tokenizer;
pub mod commands;
pub mod watcher;
pub mod search;
pub mod premium;

#[cfg(test)]
pub static HOME_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

pub fn validate_updater_config(pubkey: &str, endpoints: &[String]) -> bool {
    let normalized_pubkey = pubkey.trim().replace('\n', "").replace('\r', "");
    
    // Official Dev/Staging Keys (add rotated keys here)
    let dev_pubkeys = [
        // Active dev key used for local development & main branch CI builds (Added June 2026)
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU4RkNDQUJEOEUwOEM4NjgKUldSb3lBaU92Y3I4NkMyMnRFa1FSWkE4QXZqODFWMS8wODhIbE41Z0U1TWRBL1pJcWRyeVlURnAK",
    ];
    // Official Prod Keys (add rotated keys here)
    let prod_pubkeys = [
        // Active production key (Added June 28, 2026 for release v0.1.6)
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdGNDQwODNBQ0MzQTQ0OEUKUldTT1JEck1PZ2hFZit2RU8xVkE0ei93Q3pzT1JhYjMwR0JFMzZOajJGcThDY21kdm0yTVdlaVAK",
    ];

    let is_dev_pubkey = dev_pubkeys.iter().any(|k| k.trim().replace('\n', "").replace('\r', "") == normalized_pubkey);
    let is_prod_pubkey = prod_pubkeys.iter().any(|k| k.trim().replace('\n', "").replace('\r', "") == normalized_pubkey);

    for endpoint in endpoints {
        let endpoint_lower = endpoint.to_lowercase();
        // Dev/Staging pair verification (includes dev server and local addresses/emulators)
        if endpoint_lower.starts_with("https://dev.codeoba.com/api/update")
            || endpoint_lower.starts_with("http://localhost:")
            || endpoint_lower.starts_with("http://127.0.0.1:")
        {
            if is_dev_pubkey {
                return true;
            }
        }
        // Prod pair verification
        if endpoint_lower.starts_with("https://codeoba.com/api/update") {
            if is_prod_pubkey {
                return true;
            }
        }
    }
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Delete the window state file preemptively before the Tauri builder or plugins initialize
    if std::env::args().any(|arg| arg == "--reset-window" || arg == "--reset") {
        if let Some(mut path) = dirs::data_dir() {
            path = path.join("com.whataicando.codeoba").join(".window-state.json");
            if path.exists() {
                let _ = std::fs::remove_file(&path);
                crate::log_info!("Pre-emptively deleted window state file: {:?}", path);
            }
        }
    }

    let context = tauri::generate_context!();
    
    // Check if the updater is active from configuration and passes validation
    let updater_active = if let Some(updater_config) = context.config().plugins.0.get("updater") {
        let active = updater_config.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
        if active {
            let pubkey = updater_config.get("pubkey").and_then(|v| v.as_str()).unwrap_or("");
            let mut endpoints = Vec::new();
            if let Some(endpoints_val) = updater_config.get("endpoints") {
                if let Some(arr) = endpoints_val.as_array() {
                    for val in arr {
                        if let Some(s) = val.as_str() {
                            endpoints.push(s.to_string());
                        }
                    }
                }
            }
            validate_updater_config(pubkey, &endpoints)
        } else {
            false
        }
    } else {
        false
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            crate::log_info!("Second instance launched with args: {:?} at {}", argv, cwd);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build());

    if updater_active {
        crate::log_info!("Updater is active in configuration and passed verification. Registering updater and process plugins...");
        builder = builder
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_updater::Builder::new().build());
    } else {
        crate::log_info!("Updater is disabled or failed config verification. Skipping updater and process plugin registration.");
    }

    builder
        .manage(watcher::WatcherState {
            watcher: std::sync::Mutex::new(None),
            last_generations: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
        .manage(search::SearchIndexState::new())
        .setup(|app| {
            // Ensure encryption key is created synchronously on startup to prevent background collisions
            let _ = crate::keyring::get_or_create_cache_key();
            
            let handle = app.handle().clone();
            let _ = watcher::start_watcher(handle.clone());

            // Load cached sessions in background thread on startup
            let handle_clone = handle.clone();
            std::thread::spawn(move || {
                tauri::async_runtime::block_on(async move {
                    let state = handle_clone.state::<search::SearchIndexState>();
                    
                    // Load cached sessions in the background
                    state.load_cached_sessions();
                    
                    let progress = search::IndexingProgress {
                        step: "complete".to_string(),
                        progress: 1.0,
                        current_source: "Cache".to_string(),
                    };
                    if let Ok(mut guard) = state.last_progress.write() {
                        *guard = Some(progress.clone());
                    }
                    use tauri::Emitter;
                    let _ = handle_clone.emit("indexing-progress", progress);
                });
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::get_sources,
            commands::get_all_sessions,
            commands::get_session,
            commands::delete_source_data,
            commands::get_credential,
            commands::save_credential,
            commands::search_sessions,
            commands::rebuild_index,
            commands::log_from_frontend,
            commands::check_reset_window,
            commands::get_indexing_progress,
            commands::is_updater_active,
            commands::get_resolved_updater_endpoints,
            commands::get_semantic_model_status,
            commands::download_semantic_model,
            commands::delete_semantic_model,
            commands::resolve_and_read_file,
            commands::save_file_permission,
            commands::get_all_permissions,
            commands::delete_permission,
            commands::clear_all_permissions,
            commands::open_file_externally,
            commands::start_local_auth_server,
            commands::stop_local_auth_server
        ])
        .run(context)
        .expect("error while running tauri application");
}
