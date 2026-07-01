use crate::parsers::get_sources_list;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::collections::{HashMap, HashSet};
use tauri::{Emitter, Manager};

pub struct WatcherState {
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    pub last_generations: Mutex<HashMap<String, u64>>,
    pub watched_inodes: Mutex<HashMap<PathBuf, u64>>,
    pub detected_sources: Mutex<HashSet<String>>,
}

fn is_directory_not_empty(path: &Path) -> bool {
    if let Ok(mut entries) = std::fs::read_dir(path) {
        entries.next().is_some()
    } else {
        false
    }
}

pub fn check_and_restore_watched_paths<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    let sources = get_sources_list();
    let state = app_handle.state::<WatcherState>();
    let decisions = crate::parsers::source_decisions::load_source_decisions();
    
    let mut guard = match state.watcher.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    
    let watcher = match &mut *guard {
        Some(w) => w,
        None => return,
    };

    let idx_state = app_handle.state::<crate::search::SearchIndexState>();

    // 1. Passive addition detection for "ask" sources
    for source in &sources {
        let decision = decisions.get(source.id()).map(|s| s.as_str()).unwrap_or("ask");
        if decision == "ask" {
            let mut detected = false;
            for path in source.get_watch_paths() {
                let p = Path::new(&path);
                let watch_target = if p.extension().is_some() {
                    p.parent().map(|parent| parent.to_path_buf())
                } else {
                    Some(p.to_path_buf())
                };
                if let Some(target) = watch_target {
                    if target.exists() && is_directory_not_empty(&target) {
                        detected = true;
                        break;
                    }
                }
            }

            if detected {
                let mut detected_guard = match state.detected_sources.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                if !detected_guard.contains(source.id()) {
                    crate::log_info!("Passively detected installation of source: {}", source.id());
                    detected_guard.insert(source.id().to_string());
                    let _ = app_handle.emit("source-detected", source.id());
                }
            }
        }
    }

    // 2. Collect all expected watch targets for allowed sources
    let mut targets = Vec::new();
    for source in &sources {
        let decision = decisions.get(source.id()).map(|s| s.as_str()).unwrap_or("ask");
        if decision != "allow" {
            continue;
        }
        for path in source.get_watch_paths() {
            let p = Path::new(&path);
            let watch_target = if p.extension().is_some() {
                p.parent().map(|parent| parent.to_path_buf())
            } else {
                Some(p.to_path_buf())
            };
            if let Some(target) = watch_target {
                targets.push((source.id().to_string(), target));
            }
        }
    }

    // Deduplicate watch targets (shortest path wins)
    targets.sort_by_key(|(_, p)| p.as_os_str().len());
    let mut unique_targets = Vec::new();
    for (src_id, p) in targets {
        if !unique_targets.iter().any(|(_, u): &(String, PathBuf)| p.starts_with(u)) {
            unique_targets.push((src_id, p));
        }
    }

    // Check existing watches and rebind or unwatch if deleted
    for (source_id, target) in unique_targets {
        let exists = target.exists();
        let mut current_ino = 0;
        if exists {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if let Ok(meta) = target.metadata() {
                    current_ino = meta.ino();
                }
            }
            #[cfg(not(unix))]
            {
                if let Ok(meta) = target.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(duration) = modified.duration_since(std::time::SystemTime::UNIX_EPOCH) {
                            current_ino = duration.as_nanos() as u64;
                        }
                    }
                }
            }
        }

        let stored_ino = if let Ok(inodes_guard) = state.watched_inodes.lock() {
            inodes_guard.get(&target).copied()
        } else {
            None
        };

        if !exists {
            // Target was deleted. Clean index, remove watch, but DO NOT CREATE IT!
            let has_sessions = if let Ok(s_guard) = idx_state.sessions.read() {
                s_guard.values().any(|sess| sess.source_id == source_id)
            } else {
                false
            };

            if stored_ino.is_some() || has_sessions {
                crate::log_info!("Monitored directory was deleted: {:?}. Cleaning index for source: {}", target, source_id);
                let _ = watcher.unwatch(&target);
                if let Ok(mut inodes_guard) = state.watched_inodes.lock() {
                    inodes_guard.remove(&target);
                }

                // Remove sessions from index
                let mut removed_session_ids = Vec::new();
                if let Ok(s_guard) = idx_state.sessions.read() {
                    for (id, sess) in s_guard.iter() {
                        if sess.source_id == source_id {
                            removed_session_ids.push(id.clone());
                        }
                    }
                }
                if !removed_session_ids.is_empty() {
                    crate::log_info!("Removing {} sessions from index due to deleted directory for source: {}", removed_session_ids.len(), source_id);
                    if let Ok(mut s_guard) = idx_state.sessions.write() {
                        for id in &removed_session_ids {
                            s_guard.remove(id);
                        }
                    }
                    if let Ok(mut e_guard) = idx_state.embeddings.write() {
                        for id in &removed_session_ids {
                            e_guard.remove(id);
                        }
                    }
                    for id in &removed_session_ids {
                        let _ = app_handle.emit("session-deleted", id);
                    }
                }
            }
        } else if stored_ino != Some(current_ino) {
            // Inode mismatch or not registered yet, start/restore watch
            crate::log_info!("Monitored directory state changed (stored={:?}, current={:?}) for target: {:?}. Watching directory...", stored_ino, current_ino, target);
            let _ = watcher.unwatch(&target);
            let _ = watcher.watch(&target, RecursiveMode::Recursive);
            if let Ok(mut inodes_guard) = state.watched_inodes.lock() {
                inodes_guard.insert(target.clone(), current_ino);
            }

            // If it was already watched (stored_ino.is_some()) but the inode changed,
            // we must clear the old sessions for this source from the index before reloading.
            if stored_ino.is_some() {
                let mut removed_session_ids = Vec::new();
                if let Ok(s_guard) = idx_state.sessions.read() {
                    for (id, sess) in s_guard.iter() {
                        if sess.source_id == source_id {
                            removed_session_ids.push(id.clone());
                        }
                    }
                }
                if !removed_session_ids.is_empty() {
                    crate::log_info!("Removing {} sessions from index due to inode change for source: {}", removed_session_ids.len(), source_id);
                    if let Ok(mut s_guard) = idx_state.sessions.write() {
                        for id in &removed_session_ids {
                            s_guard.remove(id);
                        }
                    }
                    if let Ok(mut e_guard) = idx_state.embeddings.write() {
                        for id in &removed_session_ids {
                            e_guard.remove(id);
                        }
                    }
                    for id in &removed_session_ids {
                        let _ = app_handle.emit("session-deleted", id);
                    }
                }
            }

            // Trigger async reload
            let app_handle_clone = app_handle.clone();
            let source_id_clone = source_id.clone();
            tauri::async_runtime::spawn(async move {
                let sources = get_sources_list();
                if let Some(src) = sources.iter().find(|s| s.id() == source_id_clone) {
                    let sessions = src.parse_all_sessions().await;
                    let idx_state = app_handle_clone.state::<crate::search::SearchIndexState>();
                    for sess in sessions {
                        let _ = idx_state.update_session(sess.clone()).await;
                        let _ = app_handle_clone.emit("session-updated", &sess);
                    }
                }
            });
        }
    }
}

pub fn start_watcher<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>) -> Result<(), String> {
    let sources = get_sources_list();
    let decisions = crate::parsers::source_decisions::load_source_decisions();
    let mut targets = Vec::new();

    for source in &sources {
        let decision = decisions.get(source.id()).map(|s| s.as_str()).unwrap_or("ask");
        if decision != "allow" {
            continue;
        }

        for path in source.get_watch_paths() {
            let p = Path::new(&path);
            let watch_target = if p.extension().is_some() {
                p.parent().map(|parent| parent.to_path_buf())
            } else {
                Some(p.to_path_buf())
            };
            if let Some(target) = watch_target {
                if target.exists() {
                    targets.push(target);
                }
            }
        }
    }

    // Deduplicate watch targets (shortest path wins, subdirectories are ignored to prevent overlapping FSEvents conflicts)
    targets.sort_by_key(|p| p.as_os_str().len());
    let mut unique_targets = Vec::new();
    for p in targets {
        if !unique_targets.iter().any(|u| p.starts_with(u)) {
            unique_targets.push(p);
        }
    }

    let handle_clone = app_handle.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        match res {
            Ok(event) => {
                crate::log_info!("[watcher_event] Event: {:?}", event);

                // Filter for file writes, creations, or deletions
                if is_relevant_event(&event.kind) {
                    for path in event.paths {
                        crate::log_info!("[watcher_event] Processing relevant path: {:?}", path);
                        handle_file_change(&handle_clone, &path);
                    }
                }
            }
            Err(e) => {
                crate::log_error!("Watcher error: {:?}", e);
            }
        }
    })
    .map_err(|e| e.to_string())?;

    let state = app_handle.state::<WatcherState>();
    
    // Clear watched inodes and start new watches
    if let Ok(mut inodes_guard) = state.watched_inodes.lock() {
        inodes_guard.clear();
    }

    for path in &unique_targets {
        if path.exists() {
            let _ = watcher.watch(path, RecursiveMode::Recursive);
            
            // Get new inode and store it
            let mut new_ino = 0;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if let Ok(meta) = path.metadata() {
                    new_ino = meta.ino();
                }
            }
            #[cfg(not(unix))]
            {
                if let Ok(meta) = path.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(duration) = modified.duration_since(std::time::SystemTime::UNIX_EPOCH) {
                            new_ino = duration.as_nanos() as u64;
                        }
                    }
                }
            }
            if let Ok(mut inodes_guard) = state.watched_inodes.lock() {
                inodes_guard.insert(path.clone(), new_ino);
            }
        }
    }

    // Save the watcher in Tauri state so it doesn't get dropped
    if let Ok(mut guard) = state.watcher.lock() {
        *guard = Some(watcher);
    }

    // Spawn a background loop to verify monitored paths exist and to passively detect new folders
    let handle_periodic = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            check_and_restore_watched_paths(&handle_periodic);
        }
    });

    Ok(())
}

fn is_relevant_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    )
}

fn handle_file_change<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>, path: &Path) {
    let path_str = path.to_string_lossy();
    let sources = get_sources_list();

    crate::log_info!("[watcher_event] handle_file_change: {}", path_str);

    for source in sources {
        let matches_filter = match source.get_watch_file_filter() {
            Some(filter_fn) => filter_fn(&path_str),
            None => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if source.id() == "aider" && ext == "md" {
                    true
                } else if source.id() == "cursor" && (ext == "vscdb" || path_str.contains("state.vscdb")) {
                    true
                } else if ext == "jsonl" {
                    true
                } else {
                    false
                }
            }
        };

        // Also detect directory modifications/creations inside source's watched paths
        let is_dir_change = path.is_dir() && source.get_watch_paths().iter().any(|p| path_str.starts_with(p));

        crate::log_info!(
            "[watcher_event] Source '{}': matches_filter={}, is_dir_change={}",
            source.id(),
            matches_filter,
            is_dir_change
        );

        if matches_filter || is_dir_change {
            let file_path = path_str.to_string();
            let app_handle_clone = app_handle.clone();
            let source_id = source.id().to_string();

            // Get next generation count for this file to debounce
            let state = app_handle.state::<WatcherState>();
            let gen = if let Ok(mut guard) = state.last_generations.lock() {
                let entry = guard.entry(file_path.clone()).or_insert(0);
                *entry += 1;
                *entry
            } else {
                0
            };

            if gen == 0 {
                return;
            }

            tauri::async_runtime::spawn(async move {
                // Sleep to debounce rapid sequential filesystem events (e.g. 500ms)
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Check if this generation is still the latest one
                let state = app_handle_clone.state::<WatcherState>();
                let is_latest = if let Ok(mut guard) = state.last_generations.lock() {
                    let latest = guard.get(&file_path) == Some(&gen);
                    if latest {
                        guard.remove(&file_path);
                    }
                    latest
                } else {
                    false
                };

                if is_latest {
                    // Re-fetch the sources list to find the matching source adapter
                    let sources = get_sources_list();
                    if let Some(src) = sources.iter().find(|s| s.id() == source_id) {
                        let is_db = file_path.ends_with(".sqlite")
                            || file_path.ends_with(".vscdb")
                            || file_path.ends_with("-wal")
                            || file_path.ends_with("-shm")
                            || file_path.ends_with(".pb")
                            || file_path.ends_with(".pbtxt")
                            || file_path.ends_with("session_index.jsonl")
                            || file_path.ends_with("workspace.yaml");

                        let path_obj = Path::new(&file_path);
                        let is_dir = path_obj.is_dir();

                        if is_db || is_dir {
                            crate::log_info!("Database file changed ({}). Reloading all sessions for {}...", file_path, src.display_name());
                            let sessions = src.parse_all_sessions().await;
                            let idx_state = app_handle_clone.state::<crate::search::SearchIndexState>();

                            // 1. Identify and remove any stale/deleted database sessions
                            let new_session_ids: std::collections::HashSet<String> = sessions.iter().map(|s| s.id.clone()).collect();
                            let mut removed_session_ids = Vec::new();

                            if let Ok(guard) = idx_state.sessions.read() {
                                for (id, existing_sess) in guard.iter() {
                                    if existing_sess.source_id == source_id && !new_session_ids.contains(id) {
                                        removed_session_ids.push(id.clone());
                                    }
                                }
                            }

                            if !removed_session_ids.is_empty() {
                                crate::log_info!("Removing {} stale/deleted database sessions from index for source: {}", removed_session_ids.len(), source_id);
                                if let Ok(mut guard) = idx_state.sessions.write() {
                                    for id in &removed_session_ids {
                                        guard.remove(id);
                                    }
                                }
                                if let Ok(mut guard) = idx_state.embeddings.write() {
                                    for id in &removed_session_ids {
                                        guard.remove(id);
                                    }
                                }
                                for id in &removed_session_ids {
                                    let _ = app_handle_clone.emit("session-deleted", id);
                                }
                            }

                            // 2. Detect modified or new sessions
                            let mut modified_sessions = Vec::new();
                            if let Ok(guard) = idx_state.sessions.read() {
                                for sess in &sessions {
                                    if let Some(existing) = guard.get(&sess.id) {
                                        if source_id == "antigravity" {
                                            crate::log_info!("Checking antigravity session {}: existing.thread_name={:?}, sess.thread_name={:?}, existing.updated_at={}, sess.updated_at={}", sess.id, existing.thread_name, sess.thread_name, existing.updated_at, sess.updated_at);
                                        }
                                        if existing.updated_at != sess.updated_at
                                            || existing.turns.len() != sess.turns.len()
                                            || existing.thread_name != sess.thread_name
                                            || existing.is_archived != sess.is_archived
                                            || existing.is_pinned != sess.is_pinned
                                            || existing.status != sess.status
                                        {
                                            modified_sessions.push(sess.clone());
                                        }
                                    } else {
                                        modified_sessions.push(sess.clone());
                                    }
                                }
                            }

                            // 3. Update index with modified sessions list and emit updates
                            for sess in modified_sessions {
                                crate::log_info!("Updating index and emitting session-updated for database change: {}", sess.id);
                                let _ = idx_state.update_session(sess.clone()).await;
                                let _ = app_handle_clone.emit("session-updated", &sess);
                            }
                        } else {
                            // Check if file exists (if not, it's deleted)
                            if !Path::new(&file_path).exists() {
                                crate::log_info!("Session file deleted: {}. Removing from index...", file_path);
                                let idx_state = app_handle_clone.state::<crate::search::SearchIndexState>();
                                let removed_session_id = {
                                    if let Ok(mut guard) = idx_state.sessions.write() {
                                        let found = guard.iter()
                                            .find(|(_, s)| s.file_path == file_path)
                                            .map(|(k, _)| k.clone());
                                        if let Some(ref id) = found {
                                            guard.remove(id);
                                        }
                                        found
                                    } else {
                                        None
                                    }
                                };
                                if let Some(session_id) = removed_session_id {
                                    if let Ok(mut guard) = idx_state.embeddings.write() {
                                        guard.remove(&session_id);
                                    }
                                    let _ = app_handle_clone.emit("session-deleted", &session_id);
                                }
                            } else if let Some(session) = src.parse_session(&file_path).await {
                                crate::log_info!("Session file updated: {}. Updating index and emitting session-updated...", file_path);
                                let idx_state = app_handle_clone.state::<crate::search::SearchIndexState>();
                                let _ = idx_state.update_session(session.clone()).await;
                                let _ = app_handle_clone.emit("session-updated", &session);
                            }
                        }
                    }
                }
            });
            break;
        }
    }
}

#[cfg(test)]
mod watcher_tests {
    use super::*;
    use crate::models::{Session, Turn};
    use crate::search::SearchIndexState;
    use crate::parsers::SourceAdapter;

    #[test]
    fn test_restore_watched_paths_removes_sessions() {
        let _lock = crate::HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_home = tempfile::tempdir().unwrap();
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        std::env::set_var("CODEOBA_MOCK_HOME", temp_home.path().to_string_lossy().to_string());

        // Write mock source decisions so codex/antigravity are allowed in the test
        let codeoba_dir = temp_home.path().join(".codeoba");
        std::fs::create_dir_all(&codeoba_dir).unwrap();
        std::fs::write(
            codeoba_dir.join("source_decisions.json"),
            r#"{"codex": "allow", "antigravity": "allow"}"#
        ).unwrap();

        // Initialize state
        let app_handle = tauri::test::mock_app().handle().clone();
        
        // Setup WatcherState in app state
        let (tx, _rx) = std::sync::mpsc::channel();
        let watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); }).unwrap();
        app_handle.manage(WatcherState {
            watcher: Mutex::new(Some(watcher)),
            last_generations: Mutex::new(std::collections::HashMap::new()),
            watched_inodes: Mutex::new(std::collections::HashMap::new()),
            detected_sources: Mutex::new(std::collections::HashSet::new()),
        });

        let idx_state = SearchIndexState::new();
        
        // Add a mock Codex session
        let session = Session {
            id: "codex-test".to_string(),
            source_id: "codex".to_string(),
            file_path: "some_path".to_string(),
            timestamp: 0,
            updated_at: 0,
            cwd: None,
            thread_name: Some("Codex Title".to_string()),
            turns: vec![Turn {
                turn_id: "t1".to_string(),
                user_message: "User query".to_string(),
                assistant_message: "Reply".to_string(),
                timestamp: 0,
                input_tokens: None,
                output_tokens: None,
                extra_data: std::collections::HashMap::new(),
            }],
            is_archived: false,
            is_pinned: false,
            summary: None,
            snippet: None,
            workspace_name: None,
            status: None,
        };
        
        tauri::async_runtime::block_on(async {
            idx_state.update_session(session).await.unwrap();
        });

        // Verify session is in index
        {
            let guard = idx_state.sessions.read().unwrap();
            assert!(guard.contains_key("codex-test"));
        }

        // Manage idx_state in app state
        app_handle.manage(idx_state);

        // Codex target path is HOME/sessions (from get_watch_paths)
        // Wait, for Codex, the watch target parent resolved is temp_home/.codex
        let codex_dir = temp_home.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        assert!(codex_dir.exists());

        // Deleting the directory
        std::fs::remove_dir_all(&codex_dir).unwrap();
        assert!(!codex_dir.exists());

        // Run check_and_restore_watched_paths
        check_and_restore_watched_paths(&app_handle);

        // Check if directory was recreated (should NOT be recreated anymore)
        assert!(!codex_dir.exists());

        // Check if Codex session was removed from search index!
        let idx = app_handle.state::<SearchIndexState>();
        let guard = idx.sessions.read().unwrap();
        assert!(!guard.contains_key("codex-test"), "Codex session was NOT removed!");

        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CODEOBA_MOCK_HOME");
    }

    #[test]
    fn test_restore_watched_paths_removes_sessions_on_inode_change() {
        let _lock = crate::HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_home = tempfile::tempdir().unwrap();
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        std::env::set_var("CODEOBA_MOCK_HOME", temp_home.path().to_string_lossy().to_string());

        // Write mock source decisions so codex/antigravity are allowed in the test
        let codeoba_dir = temp_home.path().join(".codeoba");
        std::fs::create_dir_all(&codeoba_dir).unwrap();
        std::fs::write(
            codeoba_dir.join("source_decisions.json"),
            r#"{"codex": "allow", "antigravity": "allow"}"#
        ).unwrap();

        // Initialize state
        let app_handle = tauri::test::mock_app().handle().clone();
        
        // Setup WatcherState in app state
        let (tx, _rx) = std::sync::mpsc::channel();
        let watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); }).unwrap();
        
        let codex_dir = temp_home.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        assert!(codex_dir.exists());

        // Initialize watched inodes map with a dummy inode that is guaranteed not to match the real one
        let mut watched_inodes = std::collections::HashMap::new();
        watched_inodes.insert(codex_dir.clone(), 999999);

        app_handle.manage(WatcherState {
            watcher: Mutex::new(Some(watcher)),
            last_generations: Mutex::new(std::collections::HashMap::new()),
            watched_inodes: Mutex::new(watched_inodes),
            detected_sources: Mutex::new(std::collections::HashSet::new()),
        });

        let idx_state = SearchIndexState::new();
        
        // Add a mock Codex session
        let session = Session {
            id: "codex-test-inode".to_string(),
            source_id: "codex".to_string(),
            file_path: "some_path".to_string(),
            timestamp: 0,
            updated_at: 0,
            cwd: None,
            thread_name: Some("Codex Title".to_string()),
            turns: vec![Turn {
                turn_id: "t1".to_string(),
                user_message: "User query".to_string(),
                assistant_message: "Reply".to_string(),
                timestamp: 0,
                input_tokens: None,
                output_tokens: None,
                extra_data: std::collections::HashMap::new(),
            }],
            is_archived: false,
            is_pinned: false,
            summary: None,
            snippet: None,
            workspace_name: None,
            status: None,
        };
        
        tauri::async_runtime::block_on(async {
            idx_state.update_session(session).await.unwrap();
        });

        // Manage idx_state in app state
        app_handle.manage(idx_state);

        // Run check_and_restore_watched_paths
        check_and_restore_watched_paths(&app_handle);

        // Check if Codex session was removed from search index due to inode mismatch detection!
        let idx = app_handle.state::<SearchIndexState>();
        let guard = idx.sessions.read().unwrap();
        assert!(!guard.contains_key("codex-test-inode"), "Codex session was NOT removed on inode change!");

        // Check if the stored inode was updated to the actual inode
        let state = app_handle.state::<WatcherState>();
        let inodes_guard = state.watched_inodes.lock().unwrap();
        let stored_ino = inodes_guard.get(&codex_dir).copied().unwrap_or(0);
        assert_ne!(stored_ino, 999999, "Stored inode was not updated to the new one!");
        assert_ne!(stored_ino, 0);

        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CODEOBA_MOCK_HOME");
    }

    fn helper_encode_varint(value: u64) -> Vec<u8> {
        let mut list = Vec::new();
        let mut temp = value;
        loop {
            if (temp & !0x7F) == 0 {
                list.push(temp as u8);
                break;
            } else {
                list.push(((temp & 0x7F) | 0x80) as u8);
                temp >>= 7;
            }
        }
        list
    }

    fn helper_encode_length_delimited(field_number: u32, bytes: &[u8]) -> Vec<u8> {
        let tag = (field_number << 3) | 2;
        let mut result = helper_encode_varint(tag as u64);
        result.extend(helper_encode_varint(bytes.len() as u64));
        result.extend_from_slice(bytes);
        result
    }

    #[test]
    fn test_antigravity_rename_watcher_sync() {
        let _lock = crate::HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let temp_home = tempfile::tempdir().unwrap();
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_home.path());
        std::env::set_var("CODEOBA_MOCK_HOME", temp_home.path().to_string_lossy().to_string());

        // Write mock source decisions so codex/antigravity are allowed in the test
        let codeoba_dir = temp_home.path().join(".codeoba");
        std::fs::create_dir_all(&codeoba_dir).unwrap();
        std::fs::write(
            codeoba_dir.join("source_decisions.json"),
            r#"{"codex": "allow", "antigravity": "allow"}"#
        ).unwrap();

        // Initialize state
        let app_handle = tauri::test::mock_app().handle().clone();

        // Setup WatcherState in app state
        let (tx, _rx) = std::sync::mpsc::channel();
        let watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); }).unwrap();
        app_handle.manage(WatcherState {
            watcher: Mutex::new(Some(watcher)),
            last_generations: Mutex::new(std::collections::HashMap::new()),
            watched_inodes: Mutex::new(std::collections::HashMap::new()),
            detected_sources: Mutex::new(std::collections::HashSet::new()),
        });

        let idx_state = SearchIndexState::new();

        // 1. Create a mock Antigravity transcript
        let gemini_dir = temp_home.path().join(".gemini/antigravity");
        let brain_dir = gemini_dir.join("brain");
        let session_dir = brain_dir.join("session-antigravity-123/.system_generated/logs");
        std::fs::create_dir_all(&session_dir).unwrap();
        let transcript_file = session_dir.join("transcript.jsonl");
        std::fs::write(
            &transcript_file,
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:00:00Z","content":"<USER_REQUEST>Hello</USER_REQUEST>"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-05-20T02:00:01Z","content":"Hi"}"#
        ).unwrap();

        // 2. Create a mock pb file with initial title
        let pb_file = gemini_dir.join("agyhub_summaries_proto.pb");
        let uuid_bytes = "session-antigravity-123".as_bytes();
        let uuid_field = helper_encode_length_delimited(1, uuid_bytes);
        let title_bytes = "Exploring Physics".as_bytes();
        let title_field = helper_encode_length_delimited(1, title_bytes);
        let info_field = helper_encode_length_delimited(2, &title_field);
        let entry_field = helper_encode_length_delimited(1, &[uuid_field.clone(), info_field].concat());
        std::fs::write(&pb_file, &entry_field).unwrap();

        // 3. Load sessions initially via source to populate index
        let src = crate::parsers::antigravity::AntigravitySource::new();
        let sessions = tauri::async_runtime::block_on(async {
            src.parse_all_sessions().await
        });
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].thread_name.as_deref(), Some("Exploring Physics"));

        // Put initial session into search index
        tauri::async_runtime::block_on(async {
            idx_state.update_session(sessions[0].clone()).await.unwrap();
        });

        app_handle.manage(idx_state);

        // 4. Update the title in summaries pb to "New Physics Title"
        let title_bytes_new = "New Physics Title".as_bytes();
        let title_field_new = helper_encode_length_delimited(1, title_bytes_new);
        let info_field_new = helper_encode_length_delimited(2, &title_field_new);
        let entry_field_new = helper_encode_length_delimited(1, &[uuid_field.clone(), info_field_new].concat());
        std::fs::write(&pb_file, &entry_field_new).unwrap();

        // 5. Trigger handle_file_change to simulate file watch event
        // Set generation count to 1 so the debounce logic allows it
        {
            let state = app_handle.state::<WatcherState>();
            let mut guard = state.last_generations.lock().unwrap();
            guard.insert(pb_file.to_string_lossy().to_string(), 1);
        }

        handle_file_change(&app_handle, &pb_file);

        // 6. Give the async reload handler a moment to execute
        std::thread::sleep(std::time::Duration::from_millis(1500));

        // 7. Check if the session title in the index was updated!
        let idx = app_handle.state::<SearchIndexState>();
        let guard = idx.sessions.read().unwrap();
        let session_in_idx = guard.get("session-antigravity-123").unwrap();
        assert_eq!(session_in_idx.thread_name.as_deref(), Some("New Physics Title"));

        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CODEOBA_MOCK_HOME");
    }
}
