use crate::models::{Session, Turn};
use crate::parsers::SourceAdapter;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct AntigravitySource {
    antigravity_title_map: std::sync::RwLock<HashMap<String, String>>,
    last_pb_file_modified: std::sync::RwLock<i64>,
}

impl Default for AntigravitySource {
    fn default() -> Self {
        Self {
            antigravity_title_map: std::sync::RwLock::new(HashMap::new()),
            last_pb_file_modified: std::sync::RwLock::new(0),
        }
    }
}

impl AntigravitySource {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_base_dir(&self) -> PathBuf {
        let home = crate::parsers::get_home_dir();
        home.join(".gemini/antigravity/brain")
    }

    pub(crate) fn get_session_title(&self, session_id: &str) -> String {
        let home = crate::parsers::get_home_dir();
        let pb_file = home.join(".gemini/antigravity/agyhub_summaries_proto.pb");
        let current_modified = if pb_file.exists() && pb_file.is_file() {
            pb_file.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        } else {
            0
        };

        let last_mod = { *self.last_pb_file_modified.read().unwrap() };
        if last_mod == 0 || current_modified > last_mod {
            let map = self.build_antigravity_title_map();
            {
                let mut map_guard = self.antigravity_title_map.write().unwrap();
                *map_guard = map;
                let mut mod_guard = self.last_pb_file_modified.write().unwrap();
                *mod_guard = current_modified;
            }
        }

        let map = self.antigravity_title_map.read().unwrap();
        map.get(session_id).cloned().unwrap_or_else(|| "Antigravity Session".to_string())
    }

    fn build_antigravity_title_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let home = crate::parsers::get_home_dir();
        let pb_file = home.join(".gemini/antigravity/agyhub_summaries_proto.pb");
        if pb_file.exists() && pb_file.is_file() {
            if let Ok(bytes) = fs::read(&pb_file) {
                let mut offset = 0;
                while offset < bytes.len() {
                    let tag_res = read_varint(&bytes, &mut offset);
                    let tag = match tag_res {
                        Ok(t) => t,
                        Err(_) => break,
                    };
                    let wire_type = (tag & 0x07) as u8;
                    let field_number = (tag >> 3) as u32;

                    if field_number == 1 && wire_type == 2 {
                        let entry_len_res = read_varint(&bytes, &mut offset);
                        let entry_len = match entry_len_res {
                            Ok(l) => l as usize,
                            Err(_) => break,
                        };
                        let entry_end = offset + entry_len;
                        if entry_end > bytes.len() {
                            break;
                        }

                        let mut uuid: Option<String> = None;
                        let mut title: Option<String> = None;

                        while offset < entry_end {
                            let entry_tag_res = read_varint(&bytes, &mut offset);
                            let entry_tag = match entry_tag_res {
                                Ok(t) => t,
                                Err(_) => {
                                    break;
                                }
                            };
                            let entry_wire_type = (entry_tag & 0x07) as u8;
                            let entry_field_number = (entry_tag >> 3) as u32;

                            if entry_field_number == 1 && entry_wire_type == 2 {
                                let uuid_len_res = read_varint(&bytes, &mut offset);
                                let uuid_len = match uuid_len_res {
                                    Ok(l) => l as usize,
                                    Err(_) => {
                                        break;
                                    }
                                };
                                if offset + uuid_len <= entry_end {
                                    if let Ok(u) = String::from_utf8(bytes[offset..offset+uuid_len].to_vec()) {
                                        uuid = Some(u);
                                    }
                                    offset += uuid_len;
                                } else {
                                    offset = entry_end;
                                }
                            } else if entry_field_number == 2 && entry_wire_type == 2 {
                                let info_len_res = read_varint(&bytes, &mut offset);
                                let info_len = match info_len_res {
                                    Ok(l) => l as usize,
                                    Err(_) => {
                                        break;
                                    }
                                };
                                let info_end = offset + info_len;
                                if info_end <= entry_end {
                                    while offset < info_end {
                                        let info_tag_res = read_varint(&bytes, &mut offset);
                                        let info_tag = match info_tag_res {
                                            Ok(t) => t,
                                            Err(_) => {
                                                offset = info_end;
                                                break;
                                            }
                                        };
                                        let info_wire_type = (info_tag & 0x07) as u8;
                                        let info_field_number = (info_tag >> 3) as u32;

                                        if info_field_number == 1 && info_wire_type == 2 {
                                            let str_len_res = read_varint(&bytes, &mut offset);
                                            let str_len = match str_len_res {
                                                Ok(l) => l as usize,
                                                Err(_) => {
                                                    offset = info_end;
                                                    break;
                                                }
                                            };
                                            if offset + str_len <= info_end {
                                                if let Ok(s) = String::from_utf8(bytes[offset..offset+str_len].to_vec()) {
                                                    if title.is_none() && !s.starts_with('\n') && !s.starts_with("file://") {
                                                        title = Some(s);
                                                    }
                                                }
                                                offset += str_len;
                                            } else {
                                                offset = info_end;
                                            }
                                        } else {
                                            skip_field(&bytes, &mut offset, info_wire_type, info_end);
                                        }
                                    }
                                } else {
                                    offset = entry_end;
                                }
                            } else {
                                skip_field(&bytes, &mut offset, entry_wire_type, entry_end);
                            }
                        }

                        if let (Some(u), Some(t)) = (uuid, title) {
                            map.insert(u, t);
                        }
                        offset = entry_end;
                    } else {
                        skip_field(&bytes, &mut offset, wire_type, bytes.len());
                    }
                }
            }
        }
        map
    }
}

fn read_varint(bytes: &[u8], offset: &mut usize) -> Result<u64, String> {
    let mut result = 0u64;
    let mut shift = 0;
    while *offset < bytes.len() {
        let b = bytes[*offset] as u64;
        *offset += 1;
        result |= (b & 0x7F) << shift;
        if (b & 0x80) == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 64 {
            return Err("Varint too long".to_string());
        }
    }
    Err("Unexpected EOF reading varint".to_string())
}

fn skip_field(bytes: &[u8], offset: &mut usize, wire_type: u8, limit: usize) {
    match wire_type {
        0 => {
            let _ = read_varint(bytes, offset);
        }
        1 => {
            *offset = (*offset + 8).min(limit);
        }
        2 => {
            if let Ok(len) = read_varint(bytes, offset) {
                *offset = (*offset + len as usize).min(limit);
            } else {
                *offset = limit;
            }
        }
        5 => {
            *offset = (*offset + 4).min(limit);
        }
        _ => {
            *offset = limit;
        }
    }
}

fn clean(text: &str) -> String {
    let re = regex::Regex::new(r"<truncated (\d+) bytes>\s*").unwrap();
    let cleaned = re.replace_all(text, |caps: &regex::Captures| {
        let bytes = &caps[1];
        format!("\n\n[⚠️ SYSTEM LIMIT: Truncated {} bytes of log output here]\n\n", bytes)
    });
    cleaned.trim().to_string()
}

fn remove_surrounding_quotes(s: &str) -> &str {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn escape_tool_tags(text: &str) -> String {
    text.replace("[[[TOOL", "\\[\\[\\[TOOL")
        .replace("[[[/TOOL", "\\[\\[\\[/TOOL")
}

fn format_tool_entry(tool_type: &str, content: &str, tool_calls: Option<&serde_json::Value>, timestamp: i64) -> String {
    let label = match tool_type {
        "VIEW_FILE" => "📄 View File",
        "RUN_COMMAND" => "⚡ Run Command",
        "CODE_ACTION" => "✏️ Code Edit",
        "GREP_SEARCH" => "🔍 Search",
        "LIST_DIRECTORY" => "📂 List Directory",
        "SEARCH_WEB" => "🌐 Web Search",
        "GENERIC" => "🔧 Tool",
        "SYSTEM_MESSAGE" => "⚙️ System Message",
        "ERROR_MESSAGE" => "❌ Error",
        _ => tool_type,
    };

    let mut header_parts = Vec::new();
    if let Some(serde_json::Value::Array(arr)) = tool_calls {
        for tc in arr {
            if let Some(tc_obj) = tc.as_object() {
                let name = tc_obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(args) = tc_obj.get("args").and_then(|v| v.as_object()) {
                    let summary = match name {
                        "view_file" => args.get("AbsolutePath").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s))),
                        "run_command" => args.get("CommandLine").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s))),
                        "grep_search" => {
                            let query = args.get("Query").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s)));
                            let path = args.get("SearchPath").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s)));
                            query.map(|q| {
                                if let Some(p) = path {
                                    format!("Query: {} in {}", q, p)
                                } else {
                                    format!("Query: {}", q)
                                }
                            })
                        }
                        "list_dir" => args.get("DirectoryPath").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s))),
                        "replace_file_content" | "multi_replace_file_content" | "write_to_file" => {
                            args.get("TargetFile").and_then(|v| v.as_str()).map(|s| clean(remove_surrounding_quotes(s)))
                        }
                        _ => None,
                    };
                    if let Some(sum) = summary {
                        header_parts.push(sum);
                    }
                }
            }
        }
    }

    let header = if !header_parts.is_empty() {
        format!("{}: {}", label, header_parts.join(", "))
    } else {
        label.to_string()
    };
    let header_escaped = escape_tool_tags(&header);
    let cleaned_content = escape_tool_tags(&clean(content));

    format!("[[[TOOL:{}|{}|{}]]]\n{}\n[[[/TOOL]]]", tool_type, header_escaped, timestamp, cleaned_content)
}

struct Event {
    is_user: bool,
    text: String,
    timestamp: i64,
    model: Option<String>,
    is_compaction: bool,
    compaction_time_ms: i64,
}

impl SourceAdapter for AntigravitySource {
    fn id(&self) -> &str {
        "antigravity"
    }

    fn display_name(&self) -> &str {
        "Google Antigravity"
    }

    fn is_available(&self) -> bool {
        self.get_base_dir().exists()
    }

    fn get_default_log_paths(&self) -> Vec<String> {
        vec![self.get_base_dir().to_string_lossy().to_string()]
    }

    fn get_watch_paths(&self) -> Vec<String> {
        let home = crate::parsers::get_home_dir();
        vec![home.join(".gemini/antigravity").to_string_lossy().to_string()]
    }

    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        Some(|path| {
            path.ends_with("transcript.jsonl")
                || path.ends_with("agyhub_summaries_proto.pb")
                || (path.contains("annotations") && path.ends_with(".pbtxt"))
        })
    }

    fn is_app_installed(&self) -> bool {
        if cfg!(target_os = "macos") {
            Path::new("/Applications/Antigravity.app").exists() || Path::new("/Applications/Gemini.app").exists()
        } else {
            let home = crate::parsers::get_home_dir();
            home.join(".gemini/antigravity").exists()
        }
    }

    async fn parse_session(&self, file_path: &str) -> Option<Session> {
        let path = Path::new(file_path);
        let content_str = fs::read_to_string(path).ok()?;
        let metadata = path.metadata().ok()?;
        let last_modified = metadata.modified().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let session_id = path.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or(""))
            .to_string();

        let mut events = Vec::new();
        let mut cwd: Option<String> = None;
        let mut current_model: Option<String> = None;

        let user_req_re = regex::Regex::new(
            r"(?i)^\s*<USER_REQUEST>([\s\S]*?)</USER_REQUEST>\s*(?:<ADDITIONAL_METADATA>|<USER_SETTINGS_CHANGE>|$)"
        ).unwrap();

        let sys_msg_re = regex::Regex::new(
            r"(?i)^\s*<SYSTEM_MESSAGE>([\s\S]*?)</SYSTEM_MESSAGE>\s*$"
        ).unwrap();

        for line in content_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(element) = serde_json::from_str::<serde_json::Value>(line) {
                let obj = match element.as_object() {
                    Some(o) => o,
                    None => continue,
                };
                let step_type = match obj.get("type").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => continue,
                };
                let source = obj.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let created_at_str = obj.get("created_at").and_then(|v| v.as_str());
                let timestamp = created_at_str
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0);

                let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let tool_calls = obj.get("tool_calls");

                // Track model selection changes
                if let Some(user_settings) = obj.get("user_settings_change").and_then(|v| v.as_object()) {
                    if let Some(model_sel) = user_settings.get("Model Selection").and_then(|v| v.as_str()) {
                        current_model = Some(model_sel.to_string());
                    }
                }
                if content.contains("<USER_SETTINGS_CHANGE>") {
                    let settings_content = content.split("<USER_SETTINGS_CHANGE>")
                        .nth(1)
                        .unwrap_or("")
                        .split("</USER_SETTINGS_CHANGE>")
                        .next()
                        .unwrap_or("");
                    if let Some(line_with_model) = settings_content.lines().find(|l| l.contains("`Model Selection`")) {
                        let after_to = line_with_model.split(" to ").nth(1).unwrap_or("");
                        let model_name = after_to.split(". ").next().unwrap_or("").trim().trim_end_matches('.');
                        if !model_name.is_empty() {
                            current_model = Some(model_name.to_string());
                        }
                    }
                }

                // Extract Cwd
                if let Some(serde_json::Value::Array(arr)) = tool_calls {
                    for tc in arr {
                        if let Some(tc_obj) = tc.as_object() {
                            if let Some(args) = tc_obj.get("args").and_then(|v| v.as_object()) {
                                if let Some(arg_cwd) = args.get("Cwd").or_else(|| args.get("cwd")).and_then(|v| v.as_str()) {
                                    cwd = Some(arg_cwd.trim_matches('"').to_string());
                                }
                            }
                        }
                    }
                }

                if step_type == "USER_INPUT" && source == "USER_EXPLICIT" {
                    let mut clean_content = content.trim().to_string();
                    if let Some(caps) = user_req_re.captures(content) {
                        clean_content = caps[1].trim().to_string();
                    }
                    clean_content = clean(&clean_content);
                    if !clean_content.is_empty() {
                        events.push(Event {
                            is_user: true,
                            text: clean_content,
                            timestamp,
                            model: current_model.clone(),
                            is_compaction: false,
                            compaction_time_ms: 0,
                        });
                    }
                } else if (step_type == "PLANNER_RESPONSE" || step_type == "ASK_QUESTION") && source == "MODEL" {
                    let clean_content = escape_tool_tags(&clean(content));
                    if !clean_content.is_empty() {
                        events.push(Event {
                            is_user: false,
                            text: clean_content,
                            timestamp,
                            model: current_model.clone(),
                            is_compaction: false,
                            compaction_time_ms: 0,
                        });
                    }
                } else if matches!(step_type, "VIEW_FILE" | "RUN_COMMAND" | "CODE_ACTION" | "GREP_SEARCH" | "LIST_DIRECTORY" | "SEARCH_WEB" | "GENERIC") && source == "MODEL" {
                    let formatted = format_tool_entry(step_type, content, tool_calls, timestamp);
                    if !formatted.trim().is_empty() {
                        events.push(Event {
                            is_user: false,
                            text: formatted,
                            timestamp,
                            model: current_model.clone(),
                            is_compaction: false,
                            compaction_time_ms: 0,
                        });
                    }
                } else if step_type == "SYSTEM_MESSAGE" && source == "SYSTEM" {
                    let mut clean_content = content.trim().to_string();
                    if let Some(caps) = sys_msg_re.captures(&clean_content) {
                        clean_content = caps[1].trim().to_string();
                    } else {
                        let intro = "The following is a <SYSTEM_MESSAGE> not actually sent by the user. It is provided by the system as important information to pay attention to.";
                        if clean_content.starts_with(intro) {
                            clean_content = clean_content[intro.len()..].trim().to_string();
                        }
                    }
                    let formatted = format_tool_entry(step_type, &clean_content, tool_calls, timestamp);
                    if !formatted.trim().is_empty() {
                        events.push(Event {
                            is_user: false,
                            text: formatted,
                            timestamp,
                            model: current_model.clone(),
                            is_compaction: false,
                            compaction_time_ms: 0,
                        });
                    }
                } else if step_type == "CHECKPOINT" {
                    let preceding_event = events.last();
                    let duration = if let Some(pe) = preceding_event {
                        if pe.is_user {
                            (timestamp - pe.timestamp).max(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    events.push(Event {
                        is_user: false,
                        text: String::new(),
                        timestamp,
                        model: current_model.clone(),
                        is_compaction: true,
                        compaction_time_ms: duration,
                    });
                } else if step_type == "ERROR_MESSAGE" && source == "SYSTEM" {
                    let formatted = format_tool_entry(step_type, content, tool_calls, timestamp);
                    if !formatted.trim().is_empty() {
                        events.push(Event {
                            is_user: false,
                            text: formatted,
                            timestamp,
                            model: current_model.clone(),
                            is_compaction: false,
                            compaction_time_ms: 0,
                        });
                    }
                }
            }
        }

        if events.is_empty() {
            return None;
        }

        let mut turns = Vec::new();
        let mut turn_count = 0;
        let mut idx = 0;
        while idx < events.len() {
            let ev = &events[idx];
            if ev.is_user {
                let mut assistant_parts = Vec::new();
                let mut next_idx = idx + 1;
                let mut turn_model = ev.model.clone();
                let mut active_time_ms = 0i64;
                let mut current_timestamp = ev.timestamp;
                let mut has_compaction = false;
                let mut compaction_time_ms = 0i64;

                while next_idx < events.len() && !events[next_idx].is_user {
                    let next_ev = &events[next_idx];
                    if next_ev.is_compaction {
                        has_compaction = true;
                        compaction_time_ms += next_ev.compaction_time_ms;
                    } else {
                        if !next_ev.text.is_empty() {
                            assistant_parts.push(next_ev.text.clone());
                        }
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
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), active_time_ms.to_string());
                let final_model = turn_model.unwrap_or_else(|| "Unknown".to_string());
                extra_data.insert("model".to_string(), final_model);
                if has_compaction {
                    extra_data.insert("isCompaction".to_string(), "true".to_string());
                    extra_data.insert("compactionTimeMs".to_string(), compaction_time_ms.to_string());
                }

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: ev.text.clone(),
                    assistant_message,
                    timestamp: ev.timestamp,
                    extra_data,
                });
                turn_count += 1;
                idx = next_idx;
            } else {
                let mut extra_data = HashMap::new();
                extra_data.insert("computeTimeMs".to_string(), "0".to_string());
                let final_model = ev.model.clone().unwrap_or_else(|| "Unknown".to_string());
                extra_data.insert("model".to_string(), final_model);
                if ev.is_compaction {
                    extra_data.insert("isCompaction".to_string(), "true".to_string());
                    extra_data.insert("compactionTimeMs".to_string(), ev.compaction_time_ms.to_string());
                }

                turns.push(Turn {
                    turn_id: format!("{}_{}", session_id, turn_count),
                    user_message: String::new(),
                    assistant_message: ev.text.clone(),
                    timestamp: ev.timestamp,
                    extra_data,
                });
                turn_count += 1;
                idx += 1;
            }
        }

        let first_time = events.first().map(|e| e.timestamp).unwrap_or(last_modified);
        let last_time = events.last().map(|e| e.timestamp).unwrap_or(last_modified);

        let home = crate::parsers::get_home_dir();
        let annotation_file = home.join(format!(".gemini/antigravity/annotations/{}.pbtxt", session_id));
        let is_archived = if annotation_file.exists() && annotation_file.is_file() {
            if let Ok(text) = fs::read_to_string(&annotation_file) {
                let normalized: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                normalized.contains("archived:true")
            } else {
                false
            }
        } else {
            false
        };

        Some(Session {
            id: session_id.clone(),
            source_id: self.id().to_string(),
            file_path: file_path.to_string(),
            timestamp: first_time,
            updated_at: last_time,
            cwd,
            thread_name: Some(self.get_session_title(&session_id)),
            turns,
            is_archived,
            is_pinned: false,
            summary: None,
        })
    }

    async fn parse_all_sessions(&self) -> Vec<Session> {
        let base_dir = self.get_base_dir();
        if !base_dir.exists() || !base_dir.is_dir() {
            return Vec::new();
        }

        let home = crate::parsers::get_home_dir();
        let pb_file = home.join(".gemini/antigravity/agyhub_summaries_proto.pb");
        let current_modified = if pb_file.exists() && pb_file.is_file() {
            pb_file.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        } else {
            0
        };
        {
            let mut mod_guard = self.last_pb_file_modified.write().unwrap();
            *mod_guard = current_modified;
            let mut map_guard = self.antigravity_title_map.write().unwrap();
            *map_guard = self.build_antigravity_title_map();
        }

        let mut sessions = Vec::new();
        let mut walk_stack = vec![base_dir];
        while let Some(current_dir) = walk_stack.pop() {
            if let Ok(entries) = fs::read_dir(current_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk_stack.push(path);
                    } else if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some("transcript.jsonl") {
                        if let Some(session) = self.parse_session(&path.to_string_lossy()).await {
                            sessions.push(session);
                        }
                    }
                }
            }
        }
        sessions
    }
}
