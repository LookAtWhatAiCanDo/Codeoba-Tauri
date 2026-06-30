use crate::models::{Session, Turn};
use crate::parsers::SourceAdapter;
use chrono::TimeZone;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct AiderSource {
    active_aider_paths: std::sync::RwLock<Vec<String>>,
}

impl Default for AiderSource {
    fn default() -> Self {
        Self {
            active_aider_paths: std::sync::RwLock::new(Vec::new()),
        }
    }
}

impl AiderSource {
    pub fn new() -> Self {
        Self::default()
    }
}

fn get_base_dirs() -> Vec<PathBuf> {
    #[cfg(test)]
    {
        let home = crate::parsers::get_home_dir();
        let mut dirs = Vec::new();
        let dev = home.join("Dev");
        if dev.exists() && dev.is_dir() {
            dirs.push(dev);
        }
        let github = home.join("GitHub");
        if github.exists() && github.is_dir() {
            dirs.push(github);
        }
        dirs
    }
    #[cfg(not(test))]
    {
        Vec::new()
    }
}

fn find_aider_files(dir: &Path, depth: usize, max_depth: usize, paths: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name == "node_modules" || name == ".git" || name == "build" || name == ".gradle"
                    || name == ".idea" || name == "target" || name == "bin" || name == "out"
                    || name == "dist" || name == "vendor" {
                    continue;
                }
                find_aider_files(&path, depth + 1, max_depth, paths);
            } else if path.is_file() {
                if path.file_name().and_then(|s| s.to_str()) == Some(".aider.chat.history.md") {
                    paths.push(path);
                }
            }
        }
    }
}

impl SourceAdapter for AiderSource {
    fn id(&self) -> &str {
        "aider"
    }

    fn display_name(&self) -> &str {
        "Aider"
    }

    fn is_available(&self) -> bool {
        self.is_app_installed()
    }

    fn get_default_log_paths(&self) -> Vec<String> {
        get_base_dirs().iter().map(|p| p.to_string_lossy().to_string()).collect()
    }

    fn get_watch_paths(&self) -> Vec<String> {
        self.active_aider_paths.read().expect("Failed to lock active_aider_paths read lock").clone()
    }

    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        Some(|path| path.ends_with(".aider.chat.history.md"))
    }

    fn is_app_installed(&self) -> bool {
        let base_dirs = get_base_dirs();
        if !base_dirs.is_empty() {
            return true;
        }
        if !self.active_aider_paths.read().expect("Failed to lock active_aider_paths read lock").is_empty() {
            return true;
        }
        crate::parsers::is_executable_installed("aider")
    }

    fn delete_data_paths(&self) -> bool {
        let mut success = true;
        let paths = self.active_aider_paths.read().expect("Failed to lock active_aider_paths read lock").clone();
        for path_str in paths {
            let file = Path::new(&path_str).join(".aider.chat.history.md");
            if file.exists() {
                if fs::remove_file(file).is_err() {
                    success = false;
                }
            }
        }
        let mut paths_guard = self.active_aider_paths.write().expect("Failed to lock active_aider_paths write lock");
        paths_guard.clear();
        success
    }

    fn get_data_paths_to_delete(&self) -> Vec<String> {
        let paths = self.active_aider_paths.read().expect("Failed to lock active_aider_paths read lock").clone();
        paths.iter().map(|p| Path::new(p).join(".aider.chat.history.md").to_string_lossy().to_string()).collect()
    }

    async fn parse_session(&self, file_path: &str) -> Option<Session> {
        let path = Path::new(file_path);
        if !path.exists() || !path.is_file() {
            return None;
        }

        let metadata = path.metadata().ok()?;
        let last_modified = metadata.modified().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let size = metadata.len() as i64;

        if let Some(cached) = crate::parsers::cache::get_cache_manager().get_cached_session_for_file(
            self.id(),
            file_path,
            last_modified,
            size,
        ) {
            return Some(cached);
        }

        let text = fs::read_to_string(path).ok()?;
        let mut created_time = last_modified;
        let updated_time = created_time;

        let start_re = regex::Regex::new(r"(?i)Aider chat started at (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})").unwrap();
        if let Some(caps) = start_re.captures(&text) {
            let time_str = &caps[1];
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S") {
                if let Some(local_dt) = chrono::Local.from_local_datetime(&dt).single() {
                    created_time = local_dt.timestamp_millis();
                } else {
                    created_time = dt.and_utc().timestamp_millis();
                }
            }
        }

        let pattern = regex::Regex::new(r"(?i)(?:\n|^)#### (User|Assistant|Aider|Bot):").unwrap();
        let matches: Vec<regex::Match> = pattern.find_iter(&text).collect();

        struct RawTurn {
            is_user: bool,
            text: String,
            timestamp: i64,
        }
        let mut raw_turns = Vec::new();

        for i in 0..matches.len() {
            let m = matches[i];
            let matched_str = m.as_str().trim();
            let role = matched_str.split("#### ")
                .nth(1)
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();

            let start_content = m.end();
            let end_content = if i + 1 < matches.len() {
                matches[i + 1].start()
            } else {
                text.len()
            };

            let content = text[start_content..end_content].trim().to_string();
            if content.is_empty() {
                continue;
            }

            let is_user = role == "user";
            let is_assistant = role == "assistant" || role == "aider" || role == "bot";

            if is_user {
                raw_turns.push(RawTurn {
                    is_user: true,
                    text: content,
                    timestamp: created_time,
                });
            } else if is_assistant {
                raw_turns.push(RawTurn {
                    is_user: false,
                    text: content,
                    timestamp: created_time,
                });
            }
        }

        if raw_turns.is_empty() {
            return None;
        }

        let mut turns = Vec::new();
        let mut turn_count = 0;
        let mut current_idx = 0;

        let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let path_bytes = abs_path.to_string_lossy().as_bytes().to_vec();
        let session_uuid = uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, &path_bytes);
        let session_id = session_uuid.to_string();

        while current_idx < raw_turns.len() {
            let user_raw = &raw_turns[current_idx];
            if user_raw.is_user {
                let mut assistant_text = String::new();
                let mut compute_time_ms = 0i64;
                if current_idx + 1 < raw_turns.len() && !raw_turns[current_idx + 1].is_user {
                    let assistant_raw = &raw_turns[current_idx + 1];
                    assistant_text = assistant_raw.text.clone();
                    compute_time_ms = (assistant_raw.timestamp - user_raw.timestamp).max(0);
                    current_idx += 2;
                } else {
                    current_idx += 1;
                }
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), compute_time_ms.to_string());
                extra_data.insert("model".to_string(), "Unknown".to_string());

                let input_toks = crate::tokenizer::estimate_tokens(&user_raw.text, "Unknown");
                let output_toks = crate::tokenizer::estimate_tokens(&assistant_text, "Unknown");

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: user_raw.text.clone(),
                    assistant_message: assistant_text,
                    timestamp: user_raw.timestamp,
                    input_tokens: Some(input_toks),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
            } else {
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), "0".to_string());
                extra_data.insert("model".to_string(), "Unknown".to_string());

                let output_toks = crate::tokenizer::estimate_tokens(&user_raw.text, "Unknown");

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: String::new(),
                    assistant_message: user_raw.text.clone(),
                    timestamp: user_raw.timestamp,
                    input_tokens: Some(0),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
                current_idx += 1;
            }
        }

        let cwd = path.parent().map(|p| p.to_string_lossy().to_string());
        let project_name = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("Project");
        let thread_name = format!("{} (Aider)", project_name);

        let workspace_name = crate::models::resolve_workspace_name(&cwd);
        let status = crate::models::resolve_session_status(self.id(), &session_id, &turns, &cwd);

        let session = Session {
            id: session_id,
            source_id: self.id().to_string(),
            file_path: file_path.to_string(),
            timestamp: created_time,
            updated_at: updated_time,
            cwd,
            thread_name: Some(thread_name),
            turns,
            is_archived: false,
            is_pinned: false,
            summary: None,
            snippet: None,
            workspace_name,
            status,
        };

        crate::parsers::cache::get_cache_manager().put_cached_session(
            self.id(),
            file_path,
            last_modified,
            size,
            "",
            session.clone(),
        );

        Some(session)
    }

    async fn parse_all_sessions(&self) -> Vec<Session> {
        let base_dirs = get_base_dirs();
        let mut sessions = Vec::new();
        let mut active_dirs = std::collections::HashSet::new();
        
        let mut files = Vec::new();
        for dir in base_dirs {
            find_aider_files(&dir, 1, 5, &mut files);
        }

        crate::parsers::cache::get_cache_manager().start_scan(self.id());

        for path in files {
            if let Some(session) = self.parse_session(&path.to_string_lossy()).await {
                sessions.push(session);
                if let Some(parent) = path.parent() {
                    active_dirs.insert(parent.to_string_lossy().to_string());
                }
            }
        }

        crate::parsers::cache::get_cache_manager().end_scan(self.id());

        let mut paths_guard = self.active_aider_paths.write().expect("Failed to lock active_aider_paths write lock");
        *paths_guard = active_dirs.into_iter().collect();

        sessions
    }
}
