#![allow(async_fn_in_trait)]

use crate::models::Session;
use std::env;
use std::path::Path;
use std::process::Command;

pub mod claude;
pub mod cursor;
pub mod antigravity;
pub mod aider;
pub mod copilot;
pub mod codex;
pub mod cache;
pub mod resolver;
pub mod permissions;

#[cfg(test)]
mod tests;

pub trait SourceAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn get_default_log_paths(&self) -> Vec<String>;
    fn get_watch_paths(&self) -> Vec<String>;
    fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        None
    }
    async fn parse_session(&self, file_path: &str) -> Option<Session>;
    async fn parse_all_sessions(&self) -> Vec<Session>;
    fn is_app_installed(&self) -> bool {
        true
    }
    fn delete_data_paths(&self) -> bool {
        false
    }
    fn get_data_paths_to_delete(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Helper function to check if a binary command executable is installed on the host machine.
pub fn is_executable_installed(binary_name: &str) -> bool {
    let home = env::var("HOME").unwrap_or_default();
    
    // Check common macOS/Linux directories
    let common_paths = vec![
        format!("/opt/homebrew/bin/{}", binary_name),
        format!("/usr/local/bin/{}", binary_name),
        format!("/usr/bin/{}", binary_name),
        format!("{}/.local/bin/{}", home, binary_name),
        format!("{}/.npm-global/bin/{}", home, binary_name),
    ];
    
    for path in common_paths {
        if Path::new(&path).exists() {
            return true;
        }
    }
    
    // Check environment PATH directories
    if let Some(path_env) = env::var_os("PATH") {
        let paths = env::split_paths(&path_env);
        let extensions = if cfg!(windows) {
            vec!["", ".exe", ".cmd", ".bat"]
        } else {
            vec![""]
        };
        
        for dir in paths {
            for ext in &extensions {
                let bin_path = dir.join(format!("{}{}", binary_name, ext));
                if bin_path.exists() {
                    return true;
                }
            }
        }
    }
    
    // Fallback search using system tools
    let finder = if cfg!(windows) { "where" } else { "which" };
    let mut cmd = Command::new(finder);
    cmd.arg(binary_name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(status) = cmd.status() {
        if status.success() {
            return true;
        }
    }
    
    false
}

/// Returns the user's home directory. Checks the `HOME` (or `USERPROFILE` on Windows) environment
/// variables first for mocking support, falling back to `dirs::home_dir()`.
pub fn get_home_dir() -> std::path::PathBuf {
    if let Ok(mock_home) = env::var("CODEOBA_MOCK_HOME") {
        return std::path::PathBuf::from(mock_home);
    }
    if let Ok(home) = env::var("HOME") {
        return std::path::PathBuf::from(home);
    }
    if let Ok(userprofile) = env::var("USERPROFILE") {
        return std::path::PathBuf::from(userprofile);
    }
    dirs::home_dir().unwrap_or_default()
}

pub enum Source {
    Claude(claude::ClaudeSource),
    Cursor(cursor::CursorSource),
    Antigravity(antigravity::AntigravitySource),
    Aider(aider::AiderSource),
    Copilot(copilot::CopilotSource),
    Codex(codex::CodexSource),
}

impl Source {
    pub fn id(&self) -> &str {
        match self {
            Source::Claude(s) => s.id(),
            Source::Cursor(s) => s.id(),
            Source::Antigravity(s) => s.id(),
            Source::Aider(s) => s.id(),
            Source::Copilot(s) => s.id(),
            Source::Codex(s) => s.id(),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Source::Claude(s) => s.display_name(),
            Source::Cursor(s) => s.display_name(),
            Source::Antigravity(s) => s.display_name(),
            Source::Aider(s) => s.display_name(),
            Source::Copilot(s) => s.display_name(),
            Source::Codex(s) => s.display_name(),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            Source::Claude(s) => s.is_available(),
            Source::Cursor(s) => s.is_available(),
            Source::Antigravity(s) => s.is_available(),
            Source::Aider(s) => s.is_available(),
            Source::Copilot(s) => s.is_available(),
            Source::Codex(s) => s.is_available(),
        }
    }

    pub fn get_watch_paths(&self) -> Vec<String> {
        match self {
            Source::Claude(s) => s.get_watch_paths(),
            Source::Cursor(s) => s.get_watch_paths(),
            Source::Antigravity(s) => s.get_watch_paths(),
            Source::Aider(s) => s.get_watch_paths(),
            Source::Copilot(s) => s.get_watch_paths(),
            Source::Codex(s) => s.get_watch_paths(),
        }
    }

    pub fn get_watch_file_filter(&self) -> Option<fn(&str) -> bool> {
        match self {
            Source::Claude(s) => s.get_watch_file_filter(),
            Source::Cursor(s) => s.get_watch_file_filter(),
            Source::Antigravity(s) => s.get_watch_file_filter(),
            Source::Aider(s) => s.get_watch_file_filter(),
            Source::Copilot(s) => s.get_watch_file_filter(),
            Source::Codex(s) => s.get_watch_file_filter(),
        }
    }

    pub async fn parse_session(&self, file_path: &str) -> Option<Session> {
        match self {
            Source::Claude(s) => s.parse_session(file_path).await,
            Source::Cursor(s) => s.parse_session(file_path).await,
            Source::Antigravity(s) => s.parse_session(file_path).await,
            Source::Aider(s) => s.parse_session(file_path).await,
            Source::Copilot(s) => s.parse_session(file_path).await,
            Source::Codex(s) => s.parse_session(file_path).await,
        }
    }

    pub async fn parse_all_sessions(&self) -> Vec<Session> {
        match self {
            Source::Claude(s) => s.parse_all_sessions().await,
            Source::Cursor(s) => s.parse_all_sessions().await,
            Source::Antigravity(s) => s.parse_all_sessions().await,
            Source::Aider(s) => s.parse_all_sessions().await,
            Source::Copilot(s) => s.parse_all_sessions().await,
            Source::Codex(s) => s.parse_all_sessions().await,
        }
    }

    pub fn is_app_installed(&self) -> bool {
        match self {
            Source::Claude(s) => s.is_app_installed(),
            Source::Cursor(s) => s.is_app_installed(),
            Source::Antigravity(s) => s.is_app_installed(),
            Source::Aider(s) => s.is_app_installed(),
            Source::Copilot(s) => s.is_app_installed(),
            Source::Codex(s) => s.is_app_installed(),
        }
    }

    pub fn delete_data_paths(&self) -> bool {
        match self {
            Source::Claude(s) => s.delete_data_paths(),
            Source::Cursor(s) => s.delete_data_paths(),
            Source::Antigravity(s) => s.delete_data_paths(),
            Source::Aider(s) => s.delete_data_paths(),
            Source::Copilot(s) => s.delete_data_paths(),
            Source::Codex(s) => s.delete_data_paths(),
        }
    }
}

pub fn get_sources_list() -> Vec<Source> {
    vec![
        Source::Claude(claude::ClaudeSource),
        Source::Cursor(cursor::CursorSource::new()),
        Source::Antigravity(antigravity::AntigravitySource::new()),
        Source::Aider(aider::AiderSource::new()),
        Source::Copilot(copilot::CopilotSource::new()),
        Source::Codex(codex::CodexSource::new()),
    ]
}



