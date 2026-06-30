use crate::models::{Session, Turn};
use crate::parsers::SourceAdapter;
use rusqlite::{Connection, OpenFlags};
use rusqlite::types::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct CursorSource {
    composer_to_workspace: std::sync::RwLock<HashMap<String, String>>,
    active_composer_ids: std::sync::RwLock<std::collections::HashSet<String>>,
}

impl Default for CursorSource {
    fn default() -> Self {
        Self {
            composer_to_workspace: std::sync::RwLock::new(HashMap::new()),
            active_composer_ids: std::sync::RwLock::new(std::collections::HashSet::new()),
        }
    }
}

impl CursorSource {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_base_dir(&self) -> PathBuf {
        let home = crate::parsers::get_home_dir();
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Cursor/User")
        } else if cfg!(target_os = "windows") {
            if let Ok(app_data) = std::env::var("APPDATA") {
                PathBuf::from(app_data).join("Cursor/User")
            } else {
                home.join("AppData/Roaming/Cursor/User")
            }
        } else {
            home.join(".config/Cursor/User")
        }
    }

    fn get_global_db_file(&self) -> PathBuf {
        self.get_base_dir().join("globalStorage/state.vscdb")
    }

    fn get_workspace_storage_dir(&self) -> PathBuf {
        self.get_base_dir().join("workspaceStorage")
    }

    fn query_db(&self, db_path: &Path, sql: &str) -> Vec<HashMap<String, String>> {
        let path_str = db_path.to_string_lossy();
        let uri_path = format!("file:{}?mode=ro", path_str);
        let conn = match Connection::open_with_flags(
            Path::new(&uri_path),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let mut results = Vec::new();
        if let Ok(mut rows) = stmt.query([]) {
            while let Ok(Some(row)) = rows.next() {
                let mut map = HashMap::new();
                for i in 0..col_count {
                    if let Ok(val) = row.get::<_, Value>(i) {
                        let val_str = match val {
                            Value::Null => String::new(),
                            Value::Integer(n) => n.to_string(),
                            Value::Real(r) => r.to_string(),
                            Value::Text(s) => s,
                            Value::Blob(b) => String::from_utf8_lossy(&b).to_string(),
                        };
                        map.insert(col_names[i].clone(), val_str);
                    } else {
                        map.insert(col_names[i].clone(), String::new());
                    }
                }
                results.push(map);
            }
        }
        results
    }

    fn build_workspace_map(&self) -> (HashMap<String, String>, std::collections::HashSet<String>) {
        let mut composer_to_workspace = HashMap::new();
        let mut active_composer_ids = std::collections::HashSet::new();

        let ws_dir = self.get_workspace_storage_dir();
        if !ws_dir.exists() || !ws_dir.is_dir() {
            return (composer_to_workspace, active_composer_ids);
        }

        let mut active_dirs = Vec::new();
        if let Ok(entries) = fs::read_dir(&ws_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let ws_json = path.join("workspace.json");
                    let db_file = path.join("state.vscdb");
                    if ws_json.exists() && db_file.exists() {
                        let mtime = db_file.metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        active_dirs.push((path, mtime));
                    }
                }
            }
        }

        active_dirs.sort_by(|a, b| b.1.cmp(&a.1));
        active_dirs.truncate(100);

        for (dir, _) in active_dirs {
            let ws_json_path = dir.join("workspace.json");
            let db_path = dir.join("state.vscdb");

            let ws_json_content = match fs::read_to_string(&ws_json_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let ws_obj: serde_json::Value = match serde_json::from_str(&ws_json_content) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let folder_url = match ws_obj.get("folder").and_then(|v| v.as_str()) {
                Some(url) => url,
                None => continue,
            };

            let mut folder_path = if folder_url.starts_with("file://") {
                folder_url.trim_start_matches("file://").to_string()
            } else {
                folder_url.to_string()
            };

            if folder_path.starts_with('/') && folder_path.len() > 2 && folder_path.as_bytes()[2] == b':' {
                folder_path = folder_path[1..].to_string();
            }

            let rows = self.query_db(&db_path, "SELECT value FROM ItemTable WHERE key = 'composer.composerData' LIMIT 1;");
            if let Some(row) = rows.first() {
                if let Some(val_str) = row.get("value") {
                    if let Ok(data_obj) = serde_json::from_str::<serde_json::Value>(val_str) {
                        if let Some(all_composers) = data_obj.get("allComposers").and_then(|v| v.as_array()) {
                            for ci in all_composers {
                                if let Some(comp_id) = ci.get("composerId").and_then(|v| v.as_str()) {
                                    composer_to_workspace.insert(comp_id.to_string(), folder_path.clone());
                                    active_composer_ids.insert(comp_id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        (composer_to_workspace, active_composer_ids)
    }

    fn parse_session_from_json(&self, composer_id: &str, value_str: &str) -> Option<Session> {
        let val_obj: serde_json::Value = serde_json::from_str(value_str).ok()?;
        let name = val_obj.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Cursor Session")
            .to_string();

        let created_at = val_obj.get("createdAt")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let updated_at = val_obj.get("lastUpdatedAt")
            .and_then(|v| v.as_i64())
            .unwrap_or(created_at);

        let conversation = val_obj.get("conversation")?.as_array()?;

        let mut turns = Vec::new();
        let mut idx = 0;
        let mut turn_count = 0;

        while idx < conversation.len() {
            let item = match conversation[idx].as_object() {
                Some(obj) => obj,
                None => { idx += 1; continue; }
            };
            let item_type = item.get("type").and_then(|v| v.as_i64()).unwrap_or(1);
            let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
            
            let model_name = item.get("model")
                .and_then(|v| v.as_str())
                .or_else(|| val_obj.get("model").and_then(|v| v.as_str()))
                .or_else(|| val_obj.get("modelName").and_then(|v| v.as_str()))
                .unwrap_or("Unknown")
                .to_string();

            let mut extra_data = HashMap::new();
            extra_data.insert("model".to_string(), model_name.clone());
            extra_data.insert("computeTimeMs".to_string(), "0".to_string());

            if item_type == 1 {
                let mut assistant_text = String::new();
                if idx + 1 < conversation.len() {
                    if let Some(next) = conversation[idx + 1].as_object() {
                        let next_type = next.get("type").and_then(|v| v.as_i64()).unwrap_or(1);
                        if next_type == 2 {
                            assistant_text = next.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            idx += 2;
                        } else {
                            idx += 1;
                        }
                    } else {
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
                let input_toks = crate::tokenizer::estimate_tokens(text, &model_name);
                let output_toks = crate::tokenizer::estimate_tokens(&assistant_text, &model_name);

                turns.push(Turn {
                    turn_id: format!("{}_{}", composer_id, turn_count),
                    user_message: text.to_string(),
                    assistant_message: assistant_text,
                    timestamp: created_at,
                    input_tokens: Some(input_toks),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
            } else {
                let output_toks = crate::tokenizer::estimate_tokens(text, &model_name);

                turns.push(Turn {
                    turn_id: format!("{}_{}", composer_id, turn_count),
                    user_message: String::new(),
                    assistant_message: text.to_string(),
                    timestamp: created_at,
                    input_tokens: Some(0),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
                idx += 1;
            }
        }

        if turns.is_empty() {
            return None;
        }

        let cwd = {
            let map = self.composer_to_workspace.read().expect("Failed to lock composer_to_workspace read lock");
            map.get(composer_id).cloned()
        };

        let workspace_name = crate::models::resolve_workspace_name(&cwd);
        let status = crate::models::resolve_session_status(self.id(), composer_id, &turns, &cwd);

        Some(Session {
            id: composer_id.to_string(),
            source_id: self.id().to_string(),
            file_path: format!("composerData:{}", composer_id),
            timestamp: created_at,
            updated_at,
            cwd,
            thread_name: Some(name),
            turns,
            is_archived: false,
            is_pinned: false,
            summary: None,
            snippet: None,
            workspace_name,
            status,
        })
    }
}

impl SourceAdapter for CursorSource {
    fn id(&self) -> &str {
        "cursor"
    }

    fn display_name(&self) -> &str {
        "Cursor"
    }

    fn is_available(&self) -> bool {
        self.get_global_db_file().exists()
    }

    fn get_default_log_paths(&self) -> Vec<String> {
        let mut paths = vec![self.get_global_db_file().to_string_lossy().to_string()];
        let ws_dir = self.get_workspace_storage_dir();
        if ws_dir.exists() && ws_dir.is_dir() {
            paths.push(ws_dir.to_string_lossy().to_string());
        }
        paths
    }

    fn get_watch_paths(&self) -> Vec<String> {
        self.get_default_log_paths()
    }

    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        Some(|path| {
            path.ends_with("state.vscdb")
                || path.ends_with("workspace.json")
                || path.ends_with("-wal")
                || path.ends_with("-shm")
        })
    }

    fn is_app_installed(&self) -> bool {
        if cfg!(target_os = "macos") {
            Path::new("/Applications/Cursor.app").exists()
        } else if cfg!(target_os = "windows") {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                Path::new(&local_app_data).join("Programs/cursor/Cursor.exe").exists()
            } else {
                false
            }
        } else {
            Path::new("/usr/share/cursor/cursor").exists() || Path::new("/opt/Cursor").exists()
        }
    }

    fn delete_data_paths(&self) -> bool {
        let mut success = true;
        let db_file = self.get_global_db_file();
        if db_file.exists() {
            if fs::remove_file(db_file).is_err() {
                success = false;
            }
        }
        let ws_dir = self.get_workspace_storage_dir();
        if ws_dir.exists() {
            if fs::remove_dir_all(ws_dir).is_err() {
                success = false;
            }
        }
        success
    }

    fn get_data_paths_to_delete(&self) -> Vec<String> {
        vec![
            self.get_global_db_file().to_string_lossy().to_string(),
            self.get_workspace_storage_dir().to_string_lossy().to_string(),
        ]
    }

    async fn parse_session(&self, file_path: &str) -> Option<Session> {
        let global_db = self.get_global_db_file();
        if !global_db.exists() {
            return None;
        }

        let composer_id = if file_path.starts_with("composerData:") {
            file_path.trim_start_matches("composerData:")
        } else {
            file_path
        };

        // If the workspace map is populated, respect it as an allowlist.
        {
            let known_ids = self.active_composer_ids.read().expect("Failed to lock active_composer_ids read lock");
            if !known_ids.is_empty() && !known_ids.contains(composer_id) {
                return None;
            }
        }

        let sql = format!("SELECT value FROM cursorDiskKV WHERE key = 'composerData:{}' LIMIT 1;", composer_id);
        let rows = self.query_db(&global_db, &sql);
        let value_str = rows.first()?.get("value")?;

        let key = format!("composerData:{}", composer_id);
        let hash = format!("{:x}", md5::compute(value_str.as_bytes()));
        let size = value_str.len() as i64;

        if let Some(mut cached) = crate::parsers::cache::get_cache_manager().get_cached_session_for_db(
            self.id(),
            &key,
            &hash,
            size,
        ) {
            let cwd = {
                let map = self.composer_to_workspace.read().expect("Failed to lock composer_to_workspace read lock");
                map.get(composer_id).cloned()
            };
            if cwd.is_some() {
                cached.cwd = cwd;
            }
            return Some(cached);
        }

        let session = self.parse_session_from_json(composer_id, value_str)?;
        crate::parsers::cache::get_cache_manager().put_cached_session(
            self.id(),
            &key,
            0,
            size,
            &hash,
            session.clone(),
        );
        Some(session)
    }

    async fn parse_all_sessions(&self) -> Vec<Session> {
        let global_db = self.get_global_db_file();
        if !global_db.exists() {
            return Vec::new();
        }

        crate::parsers::cache::get_cache_manager().start_scan(self.id());

        let rows = self.query_db(
            &global_db,
            "SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%';",
        );
        if rows.is_empty() {
            crate::parsers::cache::get_cache_manager().end_scan(self.id());
            return Vec::new();
        }

        let (ws_map, active_ids) = self.build_workspace_map();
        {
            let mut map_guard = self.composer_to_workspace.write().expect("Failed to lock composer_to_workspace write lock");
            *map_guard = ws_map;
            let mut ids_guard = self.active_composer_ids.write().expect("Failed to lock active_composer_ids write lock");
            *ids_guard = active_ids.clone();
        }

        let mut sessions = Vec::new();
        for row in rows {
            let key = match row.get("key") {
                Some(k) => k,
                None => continue,
            };
            let composer_id = key.trim_start_matches("composerData:");
            let value_str = match row.get("value") {
                Some(v) => v,
                None => continue,
            };

            if !active_ids.is_empty() && !active_ids.contains(composer_id) {
                continue;
            }

            let hash = format!("{:x}", md5::compute(value_str.as_bytes()));
            let size = value_str.len() as i64;
            if let Some(mut cached) = crate::parsers::cache::get_cache_manager().get_cached_session_for_db(
                self.id(),
                key,
                &hash,
                size,
            ) {
                let cwd = {
                    let map = self.composer_to_workspace.read().expect("Failed to lock composer_to_workspace read lock");
                    map.get(composer_id).cloned()
                };
                cached.cwd = cwd;
                sessions.push(cached);
                continue;
            }

            if let Some(session) = self.parse_session_from_json(composer_id, value_str) {
                crate::parsers::cache::get_cache_manager().put_cached_session(
                    self.id(),
                    key,
                    0,
                    size,
                    &hash,
                    session.clone(),
                );
                sessions.push(session);
            }
        }

        crate::parsers::cache::get_cache_manager().end_scan(self.id());

        sessions
    }
}
