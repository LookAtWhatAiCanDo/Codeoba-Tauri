use crate::models::{Session, Turn};
use crate::parsers::SourceAdapter;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct CopilotSource;

impl CopilotSource {
    pub fn new() -> Self {
        Self
    }

    fn get_base_dir(&self) -> PathBuf {
        let home = crate::parsers::get_home_dir();
        home.join(".copilot/session-state")
    }
}

fn clean(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"<truncated (\d+) bytes>\s*").unwrap());
    let cleaned = re.replace_all(text, |caps: &regex::Captures| {
        let bytes = &caps[1];
        format!("\n\n[⚠️ SYSTEM LIMIT: Truncated {} bytes of log output here]\n\n", bytes)
    });
    cleaned.trim().to_string()
}

fn escape_tool_tags(text: &str) -> String {
    text.replace("[[[TOOL", "\\[\\[\\[TOOL")
        .replace("[[[/TOOL", "\\[\\[\\[/TOOL")
}

struct ToolStartInfo {
    tool_name: String,
    arguments: String,
    timestamp: i64,
}

struct ParsedEvent {
    is_user: bool,
    text: String,
    timestamp: i64,
    model: Option<String>,
}

impl SourceAdapter for CopilotSource {
    fn id(&self) -> &str {
        "copilot"
    }

    fn display_name(&self) -> &str {
        "GitHub Copilot"
    }

    fn is_available(&self) -> bool {
        self.get_base_dir().exists()
    }

    fn get_default_log_paths(&self) -> Vec<String> {
        vec![self.get_base_dir().to_string_lossy().to_string()]
    }

    fn get_watch_paths(&self) -> Vec<String> {
        self.get_default_log_paths()
    }

    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        Some(|path| path.ends_with("events.jsonl") || path.ends_with("workspace.yaml"))
    }

    fn is_app_installed(&self) -> bool {
        if cfg!(target_os = "macos") {
            Path::new("/Applications/GitHub Copilot.app").exists()
        } else {
            let home = crate::parsers::get_home_dir();
            home.join(".copilot").exists()
        }
    }

    fn delete_data_paths(&self) -> bool {
        let dir = self.get_base_dir();
        if dir.exists() {
            fs::remove_dir_all(dir).is_ok()
        } else {
            true
        }
    }

    fn get_data_paths_to_delete(&self) -> Vec<String> {
        self.get_default_log_paths()
    }

    async fn parse_session(&self, file_path: &str) -> Option<Session> {
        let path = Path::new(file_path);
        let parent_dir = path.parent()?;
        let workspace_yaml_file = parent_dir.join("workspace.yaml");

        if !workspace_yaml_file.exists() || !workspace_yaml_file.is_file() {
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

        let mut session_id = parent_dir.file_name()?.to_string_lossy().to_string();
        let mut thread_name = "GitHub Copilot Session".to_string();
        let mut cwd: Option<String> = None;
        let mut created_time = last_modified;
        let mut updated_time = last_modified;

        if let Ok(yaml_content) = fs::read_to_string(&workspace_yaml_file) {
            for line in yaml_content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let mut parts = trimmed.splitn(2, ':');
                let key = parts.next().unwrap_or("").trim();
                let val = parts.next().unwrap_or("").trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                match key {
                    "id" => session_id = val.to_string(),
                    "name" => thread_name = val.to_string(),
                    "cwd" => cwd = Some(val.to_string()),
                    "created_at" => {
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(val) {
                            created_time = dt.timestamp_millis();
                        }
                    }
                    "updated_at" => {
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(val) {
                            updated_time = dt.timestamp_millis();
                        }
                    }
                    _ => {}
                }
            }
        }

        let events_content = fs::read_to_string(path).ok()?;
        let mut events_list = Vec::new();
        let mut active_tool_calls = HashMap::new();

        for line in events_content.lines() {
            let trimmed_line = line.trim();
            if trimmed_line.is_empty() {
                continue;
            }
            if let Ok(element) = serde_json::from_str::<serde_json::Value>(trimmed_line) {
                let obj = match element.as_object() {
                    Some(o) => o,
                    None => continue,
                };
                let event_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let timestamp_str = obj.get("timestamp").and_then(|v| v.as_str());
                let timestamp = timestamp_str
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(last_modified);

                let data = match obj.get("data").and_then(|v| v.as_object()) {
                    Some(d) => d,
                    None => continue,
                };

                match event_type {
                    "user.message" => {
                        let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let clean_content = clean(content);
                        if !clean_content.is_empty() {
                            events_list.push(ParsedEvent {
                                is_user: true,
                                text: clean_content,
                                timestamp,
                                model: None,
                            });
                        }
                    }
                    "assistant.message" => {
                        let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let reasoning_text = data.get("reasoningText").and_then(|v| v.as_str()).unwrap_or("");
                        let model = data.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());

                        let mut text_builder = String::new();
                        if !reasoning_text.trim().is_empty() {
                            text_builder.push_str("> [!NOTE]\n> **Reasoning:**\n> ");
                            text_builder.push_str(&reasoning_text.trim().replace('\n', "\n> "));
                            text_builder.push_str("\n\n");
                        }
                        if !content.trim().is_empty() && content != "..." {
                            text_builder.push_str(&escape_tool_tags(&clean(content)));
                        }

                        let text = text_builder.trim().to_string();
                        if !text.is_empty() {
                            events_list.push(ParsedEvent {
                                is_user: false,
                                text,
                                timestamp,
                                model,
                            });
                        }
                    }
                    "tool.execution_start" => {
                        let tool_call_id = match data.get("toolCallId").and_then(|v| v.as_str()) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };
                        let tool_name = data.get("toolName").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let arguments_str = data.get("arguments").map(|v| v.to_string()).unwrap_or_default();
                        active_tool_calls.insert(tool_call_id, ToolStartInfo {
                            tool_name,
                            arguments: arguments_str,
                            timestamp,
                        });
                    }
                    "tool.execution_complete" => {
                        let tool_call_id = match data.get("toolCallId").and_then(|v| v.as_str()) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };
                        let success = data.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                        let start_info = match active_tool_calls.remove(&tool_call_id) {
                            Some(info) => info,
                            None => continue,
                        };

                        let result_obj = data.get("result").and_then(|v| v.as_object());
                        let detailed_content = result_obj.and_then(|r| r.get("detailedContent")).and_then(|v| v.as_str());
                        let content = result_obj.and_then(|r| r.get("content")).and_then(|v| v.as_str()).unwrap_or("");
                        let output_content = if let Some(dc) = detailed_content {
                            if !dc.trim().is_empty() { dc } else { content }
                        } else {
                            content
                        };

                        let label = match start_info.tool_name.to_lowercase().as_str() {
                            "view_file" => "📄 View File",
                            "run_command" | "bash" => "⚡ Run Command",
                            "replace_file_content" | "multi_replace_file_content" | "write_to_file" => "✏️ Code Edit",
                            "grep_search" => "🔍 Search",
                            "list_dir" => "📂 List Directory",
                            "search_web" => "🌐 Web Search",
                            _ => "🔧 Tool",
                        };

                        let tool_category = match start_info.tool_name.to_lowercase().as_str() {
                            "view_file" => "VIEW_FILE",
                            "run_command" | "bash" => "RUN_COMMAND",
                            "replace_file_content" | "multi_replace_file_content" | "write_to_file" => "CODE_ACTION",
                            "grep_search" => "GREP_SEARCH",
                            "list_dir" => "LIST_DIRECTORY",
                            "search_web" => "SEARCH_WEB",
                            _ => "GENERIC",
                        };

                        let mut summary = start_info.tool_name.clone();
                        if let Ok(args_val) = serde_json::from_str::<serde_json::Value>(&start_info.arguments) {
                            if let Some(args_obj) = args_val.as_object() {
                                let extracted = match start_info.tool_name.to_lowercase().as_str() {
                                    "view_file" => args_obj.get("AbsolutePath").and_then(|v| v.as_str()),
                                    "run_command" => args_obj.get("CommandLine").and_then(|v| v.as_str()),
                                    "bash" => args_obj.get("command").and_then(|v| v.as_str()),
                                    "grep_search" => args_obj.get("Query").and_then(|v| v.as_str()),
                                    "list_dir" => args_obj.get("DirectoryPath").and_then(|v| v.as_str()),
                                    "replace_file_content" | "multi_replace_file_content" | "write_to_file" => {
                                        args_obj.get("TargetFile").and_then(|v| v.as_str())
                                    }
                                    _ => None,
                                };
                                if let Some(ext) = extracted {
                                    summary = ext.to_string();
                                }
                            }
                        }

                        let header = format!("{}: {}", label, summary);
                        let header_escaped = escape_tool_tags(&header);
                        let cleaned_output = escape_tool_tags(&clean(output_content));

                        let formatted_output = if !success {
                            format!("[[[TOOL:ERROR_MESSAGE|❌ Error: {}|{}]]]\n{}\n[[[/TOOL]]]", summary, start_info.timestamp, cleaned_output)
                        } else {
                            format!("[[[TOOL:{}|{}|{}]]]\n{}\n[[[/TOOL]]]", tool_category, header_escaped, start_info.timestamp, cleaned_output)
                        };

                        events_list.push(ParsedEvent {
                            is_user: false,
                            text: formatted_output,
                            timestamp: start_info.timestamp,
                            model: data.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        });
                    }
                    _ => {}
                }
            }
        }

        if events_list.is_empty() {
            return None;
        }

        events_list.sort_by_key(|e| e.timestamp);

        let mut turns = Vec::new();
        let mut turn_count = 0;
        let mut idx = 0;

        while idx < events_list.len() {
            let ev = &events_list[idx];
            if ev.is_user {
                let mut assistant_parts = Vec::new();
                let mut next_idx = idx + 1;
                let mut turn_model = ev.model.clone();
                let mut active_time_ms = 0i64;
                let mut current_timestamp = ev.timestamp;

                while next_idx < events_list.len() && !events_list[next_idx].is_user {
                    let next_ev = &events_list[next_idx];
                    if !next_ev.text.is_empty() {
                        assistant_parts.push(next_ev.text.clone());
                    }
                    let gap = (next_ev.timestamp - current_timestamp).max(0);
                    active_time_ms += if gap > 120_000 { 15_000 } else { gap };
                    current_timestamp = next_ev.timestamp;
                    if next_ev.model.is_some() {
                        turn_model = next_ev.model.clone();
                    }
                    next_idx += 1;
                }

                let assistant_message = assistant_parts.join("\n\n");
                let active_model = turn_model.unwrap_or_else(|| "Unknown".to_string());
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), active_time_ms.to_string());
                extra_data.insert("model".to_string(), active_model.clone());

                let input_toks = crate::tokenizer::estimate_tokens(&ev.text, &active_model);
                let output_toks = crate::tokenizer::estimate_tokens(&assistant_message, &active_model);

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: ev.text.clone(),
                    assistant_message,
                    timestamp: ev.timestamp,
                    input_tokens: Some(input_toks),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
                idx = next_idx;
            } else {
                let active_model = ev.model.clone().unwrap_or_else(|| "Unknown".to_string());
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), "0".to_string());
                extra_data.insert("model".to_string(), active_model.clone());

                let output_toks = crate::tokenizer::estimate_tokens(&ev.text, &active_model);

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: String::new(),
                    assistant_message: ev.text.clone(),
                    timestamp: ev.timestamp,
                    input_tokens: Some(0),
                    output_tokens: Some(output_toks),
                    extra_data,
                });
                turn_count += 1;
                idx += 1;
            }
        }

        let first_time = events_list.first().map(|e| e.timestamp).unwrap_or(created_time);
        let last_time = events_list.last().map(|e| e.timestamp).unwrap_or(updated_time);

        let workspace_name = crate::models::resolve_workspace_name(&cwd);
        let status = crate::models::resolve_session_status(self.id(), &session_id, &turns, &cwd);

        let session = Session {
            id: session_id,
            source_id: self.id().to_string(),
            file_path: file_path.to_string(),
            timestamp: first_time,
            updated_at: last_time,
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
        let base_dir = self.get_base_dir();
        if !base_dir.exists() || !base_dir.is_dir() {
            return Vec::new();
        }

        crate::parsers::cache::get_cache_manager().start_scan(self.id());

        let mut sessions = Vec::new();
        let mut walk_stack = vec![base_dir];
        while let Some(current_dir) = walk_stack.pop() {
            if let Ok(entries) = fs::read_dir(current_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk_stack.push(path);
                    } else if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("events.jsonl") {
                        if let Some(session) = self.parse_session(&path.to_string_lossy()).await {
                            sessions.push(session);
                        }
                    }
                }
            }
        }

        crate::parsers::cache::get_cache_manager().end_scan(self.id());

        sessions
    }
}
