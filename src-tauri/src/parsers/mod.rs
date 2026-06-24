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
    if let Ok(status) = Command::new(finder)
        .arg(binary_name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status() {
        if status.success() {
            return true;
        }
    }
    
    false
}

/// Returns the user's home directory. Checks the `HOME` (or `USERPROFILE` on Windows) environment
/// variables first for mocking support, falling back to `dirs::home_dir()`.
pub fn get_home_dir() -> std::path::PathBuf {
    if let Ok(home) = env::var("HOME") {
        return std::path::PathBuf::from(home);
    }
    if let Ok(userprofile) = env::var("USERPROFILE") {
        return std::path::PathBuf::from(userprofile);
    }
    dirs::home_dir().unwrap_or_default()
}

