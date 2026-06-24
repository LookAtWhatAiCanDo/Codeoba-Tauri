use crate::models::{Session, Turn};
use crate::parsers::{SourceAdapter, is_executable_installed};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct ClaudeSource;

struct RawTurn {
    is_user: bool,
    text: String,
    timestamp: i64,
    model: Option<String>,
    is_compaction: bool,
    compaction_time_ms: i64,
}

impl SourceAdapter for ClaudeSource {
    fn id(&self) -> &str {
        "claude"
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn is_available(&self) -> bool {
        let base_dir = self.get_base_dir();
        if base_dir.exists() && base_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl")
                    {
                        return true;
                    }
                }
            }
        }
        self.is_app_installed()
    }

    fn get_default_log_paths(&self) -> Vec<String> {
        vec![self.get_base_dir().to_string_lossy().to_string()]
    }

    fn get_watch_paths(&self) -> Vec<String> {
        self.get_default_log_paths()
    }

    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        Some(|path| path.ends_with(".jsonl"))
    }

    fn is_app_installed(&self) -> bool {
        let base_dir = self.get_base_dir();
        if base_dir.exists() && base_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl")
                    {
                        return true;
                    }
                }
            }
        }
        is_executable_installed("claude")
    }

    async fn parse_session(&self, file_path: &str) -> Option<Session> {
        self.parse_session_impl(file_path).await
    }

    async fn parse_all_sessions(&self) -> Vec<Session> {
        let base_dir = self.get_base_dir();
        if !base_dir.exists() || !base_dir.is_dir() {
            return Vec::new();
        }

        let mut sessions = Vec::new();
        if let Ok(entries) = fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl")
                {
                    if let Some(session) = self.parse_session(&entry.path().to_string_lossy()).await {
                        sessions.push(session);
                    }
                }
            }
        }
        sessions
    }
}

impl ClaudeSource {
    fn get_base_dir(&self) -> PathBuf {
        let home = crate::parsers::get_home_dir();
        home.join(".claude/projects")
    }

    async fn parse_session_impl(&self, file_path: &str) -> Option<Session> {
        let path = Path::new(file_path);
        let file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let last_modified = metadata.modified().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let reader = BufReader::new(file);
        let mut raw_turns = Vec::new();
        
        let mut session_id = path.file_stem()?.to_string_lossy().to_string();
        let mut cwd: Option<String> = None;
        let mut slug: Option<String> = None;

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(element) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(obj) = element.as_object() {
                    let line_type = match obj.get("type").and_then(|v| v.as_str()) {
                        Some(t) => t,
                        None => continue,
                    };
                    
                    let timestamp = obj.get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(0);

                    if let Some(sid) = obj.get("sessionId").and_then(|v| v.as_str()) {
                        session_id = sid.to_string();
                    }
                    if let Some(c) = obj.get("cwd").and_then(|v| v.as_str()) {
                        cwd = Some(c.to_string());
                    }
                    if let Some(sl) = obj.get("slug").and_then(|v| v.as_str()) {
                        slug = Some(sl.to_string());
                    }

                    if line_type == "user" {
                        if let Some(msg_obj) = obj.get("message").and_then(|v| v.as_object()) {
                            let content = msg_obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            raw_turns.push(RawTurn {
                                is_user: true,
                                text: content.to_string(),
                                timestamp,
                                model: None,
                                is_compaction: false,
                                compaction_time_ms: 0,
                            });
                        }
                    } else if line_type == "assistant" {
                        if let Some(msg_obj) = obj.get("message").and_then(|v| v.as_object()) {
                            let model_name = msg_obj.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
                            let mut text = String::new();
                            if let Some(content_array) = msg_obj.get("content").and_then(|v| v.as_array()) {
                                for item in content_array {
                                    if let Some(item_obj) = item.as_object() {
                                        if item_obj.get("type").and_then(|v| v.as_str()) == Some("text") {
                                            if let Some(t) = item_obj.get("text").and_then(|v| v.as_str()) {
                                                text.push_str(t);
                                                text.push('\n');
                                            }
                                        }
                                    }
                                }
                            }
                            let text_trimmed = text.trim().to_string();
                            if !text_trimmed.is_empty() {
                                raw_turns.push(RawTurn {
                                    is_user: false,
                                    text: text_trimmed,
                                    timestamp,
                                    model: model_name,
                                    is_compaction: false,
                                    compaction_time_ms: 0,
                                });
                            }
                        }
                    } else if line_type == "system" {
                        if obj.get("subtype").and_then(|v| v.as_str()) == Some("compact_boundary") {
                            let duration_ms = obj.get("compactMetadata")
                                .and_then(|v| v.as_object())
                                .and_then(|m| m.get("durationMs"))
                                .and_then(|d| d.as_i64() .or_else(|| d.as_str().and_then(|s| s.parse().ok())))
                                .unwrap_or(0);
                            
                            raw_turns.push(RawTurn {
                                is_user: false,
                                text: String::new(),
                                timestamp,
                                model: None,
                                is_compaction: true,
                                compaction_time_ms: duration_ms,
                            });
                        }
                    }
                }
            }
        }

        if raw_turns.is_empty() {
            return None;
        }

        // Pair raw turns into Turns
        let mut turns = Vec::new();
        let mut current_idx = 0;
        let mut turn_count = 0;

        while current_idx < raw_turns.len() {
            let user_raw = &raw_turns[current_idx];
            if user_raw.is_user {
                let mut model_name: Option<String> = None;
                let mut has_compaction = false;
                let mut compaction_time_ms = 0;
                
                let mut next_idx = current_idx + 1;
                let mut assistant_parts = Vec::new();
                let mut last_timestamp = user_raw.timestamp;

                while next_idx < raw_turns.len() && !raw_turns[next_idx].is_user {
                    let next_raw = &raw_turns[next_idx];
                    if next_raw.is_compaction {
                        has_compaction = true;
                        compaction_time_ms += next_raw.compaction_time_ms;
                    } else if !next_raw.text.is_empty() {
                        assistant_parts.push(next_raw.text.clone());
                    }
                    last_timestamp = next_raw.timestamp;
                    if next_raw.model.is_some() {
                        model_name = next_raw.model.clone();
                    }
                    next_idx += 1;
                }

                let assistant_text = assistant_parts.join("\n\n");
                let compute_time_ms = (last_timestamp - user_raw.timestamp).max(0);
                
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), compute_time_ms.to_string());
                extra_data.insert("model".to_string(), model_name.unwrap_or_else(|| "Unknown".to_string()));
                if has_compaction {
                    extra_data.insert("isCompaction".to_string(), "true".to_string());
                    extra_data.insert("compactionTimeMs".to_string(), compaction_time_ms.to_string());
                }

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: user_raw.text.clone(),
                    assistant_message: assistant_text,
                    timestamp: user_raw.timestamp,
                    extra_data,
                });
                turn_count += 1;
                current_idx = next_idx;
            } else {
                // Assistant only / orphan turn
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), "0".to_string());
                extra_data.insert("model".to_string(), user_raw.model.clone().unwrap_or_else(|| "Unknown".to_string()));
                if user_raw.is_compaction {
                    extra_data.insert("isCompaction".to_string(), "true".to_string());
                    extra_data.insert("compactionTimeMs".to_string(), user_raw.compaction_time_ms.to_string());
                }

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: String::new(),
                    assistant_message: user_raw.text.clone(),
                    timestamp: user_raw.timestamp,
                    extra_data,
                });
                turn_count += 1;
                current_idx += 1;
            }
        }

        let first_time = raw_turns.first().map(|t| t.timestamp).unwrap_or(last_modified);
        let last_time = raw_turns.last().map(|t| t.timestamp).unwrap_or(last_modified);

        let clean_thread_name = if let Some(ref s) = slug {
            let home = crate::parsers::get_home_dir();
            let plan_file = home.join(format!(".claude/plans/{}.md", s));
            if plan_file.exists() && plan_file.is_file() {
                if let Ok(file) = File::open(&plan_file) {
                    let mut reader = BufReader::new(file);
                    let mut first_line = String::new();
                    if reader.read_line(&mut first_line).is_ok() && !first_line.trim().is_empty() {
                        let trimmed = first_line.trim();
                        if trimmed.starts_with('#') {
                            let raw_title = trimmed.trim_start_matches('#').trim();
                            if raw_title.to_lowercase().starts_with("plan:") {
                                raw_title[5..].trim().to_string()
                            } else if raw_title.to_lowercase().starts_with("goal:") {
                                raw_title[5..].trim().to_string()
                            } else {
                                raw_title.to_string()
                            }
                        } else {
                            self.format_slug(s)
                        }
                    } else {
                        self.format_slug(s)
                    }
                } else {
                    self.format_slug(s)
                }
            } else {
                self.format_slug(s)
            }
        } else {
            "Claude Session".to_string()
        };

        Some(Session {
            id: session_id,
            source_id: self.id().to_string(),
            file_path: file_path.to_string(),
            timestamp: first_time,
            updated_at: last_time,
            cwd,
            thread_name: Some(clean_thread_name),
            turns,
            is_archived: false,
            is_pinned: false,
            summary: None,
        })
    }

    fn format_slug(&self, slug: &str) -> String {
        let replaced = slug.replace("-", " ").to_lowercase();
        let mut chars = replaced.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}
