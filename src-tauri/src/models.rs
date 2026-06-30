use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceChartPoint {
    pub label: String,
    pub value: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub key_actions: Vec<String>,
    pub errors: Vec<String>,
    pub performance_charts: Vec<PerformanceChartPoint>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub turn_id: String,
    pub user_message: String,
    pub assistant_message: String,
    pub timestamp: i64,
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub extra_data: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub source_id: String,
    pub file_path: String,
    pub timestamp: i64,
    pub updated_at: i64,
    pub cwd: Option<String>,
    pub thread_name: Option<String>,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_pinned: bool,
    pub summary: Option<SessionSummary>,
    #[serde(default)]
    pub snippet: Option<String>,
    #[serde(default)]
    pub workspace_name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

impl Session {
    pub fn to_lightweight(&self) -> Self {
        let snippet = self.snippet.clone().or_else(|| {
            self.turns.last().map(|turn| {
                let msg = if !turn.user_message.is_empty() {
                    &turn.user_message
                } else {
                    &turn.assistant_message
                };
                let mut snippet_text = msg.chars().take(100).collect::<String>().replace('\n', " ");
                if msg.chars().count() > 100 {
                    snippet_text.truncate(100);
                    snippet_text.push_str("...");
                }
                snippet_text
            })
        });

        Self {
            id: self.id.clone(),
            source_id: self.source_id.clone(),
            file_path: self.file_path.clone(),
            timestamp: self.timestamp,
            updated_at: self.updated_at,
            cwd: self.cwd.clone(),
            thread_name: self.thread_name.clone(),
            turns: self.turns.iter().map(|t| Turn {
                turn_id: t.turn_id.clone(),
                user_message: String::new(),
                assistant_message: String::new(),
                timestamp: t.timestamp,
                input_tokens: t.input_tokens,
                output_tokens: t.output_tokens,
                extra_data: t.extra_data.clone(),
            }).collect(),
            is_archived: self.is_archived,
            is_pinned: self.is_pinned,
            summary: None,
            snippet,
            workspace_name: self.workspace_name.clone(),
            status: self.status.clone(),
        }
    }
}

pub fn resolve_session_status(source_id: &str, session_id: &str, turns: &[Turn], cwd: &Option<String>) -> Option<String> {
    if turns.is_empty() {
        return Some("discussion".to_string());
    }

    let last_turn = &turns[turns.len() - 1];

    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    
    // Support both second-based and millisecond-based timestamps
    let mut last_timestamp = last_turn.timestamp;
    if last_timestamp < 20_000_000_000 {
        last_timestamp *= 1000;
    }

    let age_ms = now - last_timestamp;
    let is_recent = age_ms >= 0 && age_ms < 3600_000; // 1 hour threshold for recent/active session

    // Check if the last turn's assistant message is empty and the session is recent.
    // If it's empty and recent, it means it's currently running/executing.
    let is_currently_running = last_turn.assistant_message.trim().is_empty() && is_recent;

    if is_currently_running {
        return Some("executing".to_string());
    }

    if source_id == "antigravity" {
        let home = crate::parsers::get_home_dir();
        let brain_dir = home.join(format!(".gemini/antigravity/brain/{}", session_id));
        
        // 1. Check for awaiting approval / pending plan
        let plan_metadata = brain_dir.join("implementation_plan.md.metadata.json");
        if plan_metadata.exists() && plan_metadata.is_file() {
            if let Ok(content) = std::fs::read_to_string(&plan_metadata) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if val.get("requestFeedback").and_then(|v| v.as_bool()) == Some(true) {
                        return Some("awaiting_review".to_string());
                    }
                }
            }
        }
        
        // 2. Check if task.md exists
        let task_file = brain_dir.join("task.md");
        if task_file.exists() && task_file.is_file() {
            if let Ok(content) = std::fs::read_to_string(&task_file) {
                let mut has_any_tasks = false;
                let mut has_uncompleted_tasks = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("- [ ]") || trimmed.starts_with("- [/]") {
                        has_any_tasks = true;
                        has_uncompleted_tasks = true;
                    } else if trimmed.starts_with("- [x]") {
                        has_any_tasks = true;
                    }
                }
                
                if has_any_tasks {
                    if has_uncompleted_tasks {
                        return Some("executing".to_string());
                    } else {
                        return Some("completed".to_string());
                    }
                }
            }
        }
        
        // 3. Check walkthrough.md
        let walkthrough_file = brain_dir.join("walkthrough.md");
        if walkthrough_file.exists() && walkthrough_file.is_file() {
            return Some("completed".to_string());
        }
    }

    // Default fallback: if cwd is defined, treat it as a tool/workspace execution session
    if cwd.is_some() {
        if is_recent {
            Some("awaiting_review".to_string())
        } else {
            Some("completed".to_string())
        }
    } else {
        Some("discussion".to_string())
    }
}

pub fn resolve_workspace_name(cwd: &Option<String>) -> Option<String> {
    let cwd_str = cwd.as_deref()?;
    let path = std::path::Path::new(cwd_str);
    let mut current = path.to_path_buf();
    let mut git_root: Option<std::path::PathBuf> = None;
    
    while let Some(parent) = current.parent() {
        if current.join(".git").exists() {
            git_root = Some(current.clone());
        }
        current = parent.to_path_buf();
    }
    if current.join(".git").exists() {
        git_root = Some(current);
    }
    
    if let Some(root) = git_root {
        if let Ok(rel) = path.strip_prefix(&root) {
            let components: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            if !components.is_empty() {
                let first = &components[0];
                let standard_folders: std::collections::HashSet<&str> = [
                    "src", "lib", "bin", "app", "tests", "docs", "config", 
                    ".github", "target", "dist", "node_modules", "build", 
                    "public", "assets", "functions", "src-tauri"
                ].iter().cloned().collect();
                
                if !standard_folders.contains(first.as_str()) {
                    return Some(first.clone());
                }
            }
        }
        return root.file_name().map(|n| n.to_string_lossy().into_owned());
    }
    
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

