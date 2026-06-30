use crate::keyring;
use crate::models::Session;
use crate::parsers::get_sources_list;
use crate::search::{SearchFilter, SearchResult, SearchIndexState};
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMetadata {
    pub id: String,
    pub display_name: String,
    pub is_available: bool,
    pub is_app_installed: bool,
}

#[tauri::command]
pub fn get_sources() -> Vec<SourceMetadata> {
    let sources = get_sources_list();
    sources
        .iter()
        .map(|s| SourceMetadata {
            id: s.id().to_string(),
            display_name: s.display_name().to_string(),
            is_available: s.is_available(),
            is_app_installed: s.is_app_installed(),
        })
        .collect()
}

#[tauri::command]
pub async fn get_all_sessions<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> Result<Vec<Session>, String> {
    let state = app_handle.state::<SearchIndexState>();
    let guard = state.sessions.read().map_err(|e| e.to_string())?;
    
    let mut all_sessions: Vec<Session> = guard.values().map(|s| {
        let mut lightweight = s.to_lightweight();
        if lightweight.workspace_name.is_none() && lightweight.cwd.is_some() {
            lightweight.workspace_name = crate::models::resolve_workspace_name(&lightweight.cwd);
        }
        if lightweight.status.is_none() {
            lightweight.status = crate::models::resolve_session_status(&lightweight.source_id, &lightweight.id, &lightweight.turns, &lightweight.cwd);
        }
        lightweight
    }).collect();
    
    // Sort sessions by updated_at descending
    all_sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(all_sessions)
}

#[tauri::command]
pub async fn get_session<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    source_id: String,
    file_path: String,
) -> Result<Option<Session>, String> {
    let start_time = std::time::Instant::now();
    crate::log_info!("[IPC] get_session: Started for source_id='{}', file_path='{}'", source_id, file_path);

    let state = app_handle.state::<SearchIndexState>();
    let in_memory_cached = {
        let guard = state.sessions.read().map_err(|e| e.to_string())?;
        guard.values().find(|s| s.source_id == source_id && s.file_path == file_path).cloned()
    };

    if let Some(mut session) = in_memory_cached {
        if session.workspace_name.is_none() && session.cwd.is_some() {
            session.workspace_name = crate::models::resolve_workspace_name(&session.cwd);
        }
        if session.status.is_none() {
            session.status = crate::models::resolve_session_status(&session.source_id, &session.id, &session.turns, &session.cwd);
        }
        let elapsed = start_time.elapsed();
        crate::log_info!(
            "[IPC] get_session: Completed in {:?} (loaded from SearchIndexState cache, turns: {})",
            elapsed,
            session.turns.len()
        );
        return Ok(Some(session));
    }

    // Not found in in-memory SearchIndexState. Fall back to parsing the file (which uses CacheManager cache checks internally)
    crate::log_info!("[IPC] get_session: Cache miss in SearchIndexState. Falling back to parsing file...");
    let sources = get_sources_list();
    let source = sources.iter().find(|s| s.id() == source_id);
    match source {
        Some(s) => {
            let session_opt = s.parse_session(&file_path).await;
            let elapsed = start_time.elapsed();
            match &session_opt {
                Some(session) => {
                    crate::log_info!(
                        "[IPC] get_session: Completed in {:?} (parsed via source adapter, turns: {})",
                        elapsed,
                        session.turns.len()
                    );
                }
                None => {
                    crate::log_info!("[IPC] get_session: Completed in {:?} (failed to parse file)", elapsed);
                }
            }
            Ok(session_opt.map(|mut session| {
                if session.workspace_name.is_none() && session.cwd.is_some() {
                    session.workspace_name = crate::models::resolve_workspace_name(&session.cwd);
                }
                if session.status.is_none() {
                    session.status = crate::models::resolve_session_status(&session.source_id, &session.id, &session.turns, &session.cwd);
                }
                session
            }))
        }
        None => {
            let elapsed = start_time.elapsed();
            crate::log_error!("[IPC] get_session: Completed with error in {:?}: Source adapter '{}' not found", elapsed, source_id);
            Err(format!("Source adapter '{}' not found", source_id))
        }
    }
}

#[tauri::command]
pub fn delete_source_data(source_id: String) -> Result<bool, String> {
    let sources = get_sources_list();
    let source = sources.iter().find(|s| s.id() == source_id);
    match source {
        Some(s) => Ok(s.delete_data_paths()),
        None => Err(format!("Source adapter '{}' not found", source_id)),
    }
}

#[tauri::command]
pub fn get_credential(key: String) -> Option<String> {
    keyring::get_secret(&key)
}

#[tauri::command]
pub fn save_credential(key: String, value: Option<String>) {
    keyring::put_secret(&key, value.as_deref());
}

#[tauri::command]
pub fn is_keyring_disabled() -> bool {
    keyring::is_keyring_disabled()
}

#[tauri::command]
pub fn set_keyring_disabled(disabled: bool) {
    keyring::set_keyring_disabled(disabled);
}

#[tauri::command]
pub fn is_premium_active() -> bool {
    crate::premium::is_premium_active()
}

#[tauri::command]
pub async fn search_sessions<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    query: String,
    filter: SearchFilter,
    use_semantic: bool,
    similarity_threshold: Option<f64>,
) -> Result<Vec<SearchResult>, String> {
    let state = app_handle.state::<SearchIndexState>();
    
    let sessions: Vec<Session> = {
        let guard = state.sessions.read().map_err(|e| e.to_string())?;
        guard.values().cloned().collect()
    };

    let mut results = if use_semantic {
        let model_path = crate::search::downloader::get_model_file();
        let vocab_path = crate::search::downloader::get_vocab_file();

        if !model_path.exists() || !vocab_path.exists() {
            return Err("Semantic search is unavailable: ONNX model/vocab not found under ~/.codeoba/models/. Please download the model or use lexical search.".to_string());
        }
        let onnx_embedder = crate::search::semantic::OnnxSemanticEmbedder::new(&model_path, &vocab_path)?;
        let query_vector = onnx_embedder.get_embeddings(&query)?;

        let embeddings_guard = state.embeddings.read().map_err(|e| e.to_string())?;
        let threshold = similarity_threshold.unwrap_or(0.35) as f32;
        crate::search::semantic::semantic_search(
            &sessions,
            &embeddings_guard,
            &query_vector,
            threshold,
            &filter,
        )
    } else {
        crate::search::lexical::lexical_search(&sessions, &query, &filter)
    };

    for res in &mut results {
        res.session = res.session.to_lightweight();
    }
    Ok(results)
}

#[tauri::command]
pub async fn rebuild_index<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    bypass_cache: Option<bool>,
) -> Result<(), String> {
    if bypass_cache == Some(true) {
        crate::log_info!("[IPC] rebuild_index: Bypassing and clearing cache!");
        crate::parsers::cache::get_cache_manager().clear_all_caches();
    }
    let state = app_handle.state::<SearchIndexState>();
    state.rebuild(true, Some(app_handle.clone())).await
}

#[tauri::command]
pub fn log_from_frontend(level: String, message: String) {
    let formatted = format!("[FE-{}] {}", level.to_uppercase(), message);
    if level == "error" {
        crate::log_error!("{}", formatted);
    } else if level == "warn" {
        crate::log_warn!("{}", formatted);
    } else {
        crate::log_info!("{}", formatted);
    }
}

#[tauri::command]
pub fn check_reset_window() -> bool {
    std::env::args().any(|arg| arg == "--reset-window" || arg == "--reset")
}

#[tauri::command]
pub fn get_indexing_progress<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
) -> Result<Option<crate::search::IndexingProgress>, String> {
    let state = app_handle.state::<crate::search::SearchIndexState>();
    let guard = state.last_progress.read().map_err(|e| e.to_string())?;
    Ok(guard.clone())
}

#[tauri::command]
pub fn is_updater_active<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> bool {
    let config = app_handle.config();
    if let Some(updater_config) = config.plugins.0.get("updater") {
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
            crate::validate_updater_config(pubkey, &endpoints)
        } else {
            false
        }
    } else {
        false
    }
}

#[tauri::command]
pub fn get_resolved_updater_endpoints<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> Vec<String> {
    let config = app_handle.config();
    let current_version = config.version.clone().unwrap_or_else(|| "0.1.0".to_string());
    
    // Resolve target and arch
    let target = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86"
    };

    if let Some(updater_config) = config.plugins.0.get("updater") {
        if let Some(endpoints) = updater_config.get("endpoints") {
            if let Some(arr) = endpoints.as_array() {
                return arr.iter()
                    .filter_map(|val| val.as_str())
                    .map(|s| {
                        s.replace("{{current_version}}", &current_version)
                         .replace("{{target}}", target)
                         .replace("{{arch}}", arch)
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

#[tauri::command]
pub fn get_semantic_model_status() -> bool {
    crate::search::downloader::is_model_downloaded()
}

#[tauri::command]
pub async fn download_semantic_model<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> Result<(), String> {
    crate::search::downloader::download_model(app_handle).await
}

#[tauri::command]
pub fn delete_semantic_model() {
    crate::search::downloader::delete_model_files();
}

use crate::parsers::resolver::{resolve_local_file_link, LocalFileResolution};
use crate::parsers::permissions;

#[derive(serde::Serialize)]
pub struct FileReadResponse {
    status: String, // "allowed" | "confirmation_required" | "denied" | "rejected"
    content: Option<String>,
    #[serde(rename = "canonicalPath")]
    canonical_path: Option<String>,
    reason: Option<String>,
}

#[tauri::command]
pub fn resolve_and_read_file(
    raw_path: String,
    session_cwd: Option<String>,
) -> Result<FileReadResponse, String> {
    let base_dir = session_cwd.as_ref().map(std::path::Path::new);
    let trusted_root = base_dir;

    let resolution = resolve_local_file_link(&raw_path, base_dir, trusted_root);
    
    match resolution {
        LocalFileResolution::Allowed(path) => {
            read_resolved_file(path)
        }
        LocalFileResolution::ConfirmationRequired(path, reason) => {
            let path_str = path.to_string_lossy().to_string();
            match permissions::check_permission(&path_str, "preview") {
                Some(ref dec) if dec == "allow" => read_resolved_file(path),
                Some(ref dec) if dec == "deny" => Ok(FileReadResponse {
                    status: "denied".to_string(),
                    content: None,
                    canonical_path: Some(path_str),
                    reason: Some("Permission denied by saved preferences.".to_string()),
                }),
                _ => Ok(FileReadResponse {
                    status: "confirmation_required".to_string(),
                    content: None,
                    canonical_path: Some(path_str),
                    reason: Some(reason),
                }),
            }
        }
        LocalFileResolution::Rejected(reason) => {
            Ok(FileReadResponse {
                status: "rejected".to_string(),
                content: None,
                canonical_path: None,
                reason: Some(reason),
            })
        }
    }
}

fn read_resolved_file(path: std::path::PathBuf) -> Result<FileReadResponse, String> {
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Failed to read metadata: {}", e))?;
    if metadata.len() > 5_242_881 {
        return Err("File exceeds maximum preview limit of 5MB".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    let content = String::from_utf8_lossy(&bytes).into_owned();
    Ok(FileReadResponse {
        status: "allowed".to_string(),
        content: Some(content),
        canonical_path: Some(path.to_string_lossy().to_string()),
        reason: None,
    })
}

#[tauri::command]
pub fn save_file_permission(canonical_path: String, action: String, decision: String) {
    permissions::add_permission(&canonical_path, &action, &decision);
}

#[tauri::command]
pub fn get_all_permissions() -> Vec<permissions::PermissionEntry> {
    permissions::load_permissions()
}

#[tauri::command]
pub fn delete_permission(canonical_path: String, action: Option<String>) {
    permissions::delete_permission(&canonical_path, action.as_deref());
}

#[tauri::command]
pub fn clear_all_permissions() {
    permissions::clear_all_permissions();
}

#[tauri::command]
pub fn open_file_externally(raw_path: String, session_cwd: Option<String>) -> Result<(), String> {
    let base_dir = session_cwd.as_ref().map(std::path::Path::new);
    let trusted_root = base_dir;

    let resolution = resolve_local_file_link(&raw_path, base_dir, trusted_root);
    
    let path = match resolution {
        LocalFileResolution::Allowed(path) => path,
        LocalFileResolution::ConfirmationRequired(path, reason) => {
            let path_str = path.to_string_lossy().to_string();
            match permissions::check_permission(&path_str, "external_open") {
                Some(ref dec) if dec == "allow" => path,
                Some(ref dec) if dec == "deny" => return Err("Permission denied by saved preferences.".to_string()),
                _ => return Err(format!("Confirmation required: {}", reason)),
            }
        }
        LocalFileResolution::Rejected(reason) => return Err(reason),
    };

    let path_str = path.to_string_lossy().to_string();
    
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(&["/c", "start", "", &path_str])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path_str)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path_str)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn start_local_auth_server<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> Result<u16, String> {
    crate::premium::loopback::start_server(app_handle)
}

#[tauri::command]
pub fn stop_local_auth_server() {
    crate::premium::loopback::stop_server();
}

