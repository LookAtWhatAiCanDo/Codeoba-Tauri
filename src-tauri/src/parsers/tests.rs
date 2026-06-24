use crate::parsers::claude::ClaudeSource;
use crate::parsers::cursor::CursorSource;
use crate::parsers::antigravity::AntigravitySource;
use crate::parsers::aider::AiderSource;
use crate::parsers::copilot::CopilotSource;
use crate::parsers::codex::CodexSource;
use crate::parsers::SourceAdapter;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
enum TempMessagePart {
    Text(String),
    Tool {
        tool_type: String,
        header: String,
        content: String,
        timestamp: i64,
    },
}

fn temp_is_escaped(text: &str, index: usize) -> bool {
    let mut count = 0;
    let mut i = index as i64 - 1;
    let bytes = text.as_bytes();
    while i >= 0 && bytes[i as usize] == b'\\' {
        count += 1;
        i -= 1;
    }
    count % 2 != 0
}

fn temp_unescape_tool_tags(text: &str) -> String {
    text.replace("\\[\\[\\[TOOL", "[[[TOOL")
        .replace("\\[\\[\\[/TOOL", "[[[/TOOL")
}

fn temp_parse_assistant_message(message: &str) -> Vec<TempMessagePart> {
    let mut parts = Vec::new();
    let mut current_index = 0;
    while current_index < message.len() {
        let mut start_idx = message[current_index..].find("[[[TOOL:");
        if let Some(idx) = start_idx {
            let actual_idx = current_index + idx;
            if temp_is_escaped(message, actual_idx) {
                let mut search_from = actual_idx + 8;
                loop {
                    if let Some(next_idx) = message[search_from..].find("[[[TOOL:") {
                        let actual_next = search_from + next_idx;
                        if !temp_is_escaped(message, actual_next) {
                            start_idx = Some(actual_next - current_index);
                            break;
                        }
                        search_from = actual_next + 8;
                    } else {
                        start_idx = None;
                        break;
                    }
                }
            }
        }

        let start_idx = match start_idx {
            Some(idx) => current_index + idx,
            None => {
                let remaining = &message[current_index..];
                if !remaining.is_empty() {
                    parts.push(TempMessagePart::Text(temp_unescape_tool_tags(remaining)));
                }
                break;
            }
        };

        if start_idx > current_index {
            let preceding = &message[current_index..start_idx];
            if !preceding.is_empty() {
                parts.push(TempMessagePart::Text(temp_unescape_tool_tags(preceding)));
            }
        }

        let header_end_idx = match message[start_idx..].find("]]]") {
            Some(idx) => start_idx + idx,
            None => {
                parts.push(TempMessagePart::Text(temp_unescape_tool_tags(&message[start_idx..])));
                break;
            }
        };

        let header_content = &message[start_idx + 8..header_end_idx];
        let parts_of_header: Vec<&str> = header_content.split('|').collect();
        let tool_type = parts_of_header.first().copied().unwrap_or("");
        let header = parts_of_header.get(1).copied().unwrap_or("");
        let timestamp = parts_of_header.get(2).and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);

        let mut end_idx = None;
        let mut search_from = header_end_idx + 3;
        while search_from <= message.len() {
            if let Some(idx) = message[search_from..].find("[[[/TOOL]]]") {
                let actual_end = search_from + idx;
                if !temp_is_escaped(message, actual_end) {
                    end_idx = Some(actual_end);
                    break;
                }
                search_from = actual_end + 11;
            } else {
                break;
            }
        }

        let end_idx = match end_idx {
            Some(idx) => idx,
            None => {
                let tag_start = &message[start_idx..start_idx + 8];
                parts.push(TempMessagePart::Text(temp_unescape_tool_tags(tag_start)));
                current_index = start_idx + 8;
                continue;
            }
        };

        let content = &message[header_end_idx + 3..end_idx];
        parts.push(TempMessagePart::Tool {
            tool_type: temp_unescape_tool_tags(tool_type),
            header: temp_unescape_tool_tags(header),
            content: temp_unescape_tool_tags(content),
            timestamp,
        });
        current_index = end_idx + 11;
    }
    parts
}

async fn with_mock_home<F, Fut>(f: F)
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let temp_dir = tempfile::tempdir().unwrap();
    let original_home = std::env::var_os("HOME");
    let original_userprofile = std::env::var_os("USERPROFILE");
    let original_appdata = std::env::var_os("APPDATA");
    let original_localappdata = std::env::var_os("LOCALAPPDATA");

    std::env::set_var("HOME", temp_dir.path());
    std::env::set_var("USERPROFILE", temp_dir.path());
    std::env::set_var("APPDATA", temp_dir.path().join("AppData/Roaming"));
    std::env::set_var("LOCALAPPDATA", temp_dir.path().join("AppData/Local"));

    f(temp_dir.path().to_path_buf()).await;

    if let Some(h) = original_home {
        std::env::set_var("HOME", h);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(up) = original_userprofile {
        std::env::set_var("USERPROFILE", up);
    } else {
        std::env::remove_var("USERPROFILE");
    }
    if let Some(ad) = original_appdata {
        std::env::set_var("APPDATA", ad);
    } else {
        std::env::remove_var("APPDATA");
    }
    if let Some(la) = original_localappdata {
        std::env::set_var("LOCALAPPDATA", la);
    } else {
        std::env::remove_var("LOCALAPPDATA");
    }
}

fn encode_varint(value: u64) -> Vec<u8> {
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

fn encode_length_delimited(field_number: u32, bytes: &[u8]) -> Vec<u8> {
    let tag = (field_number << 3) | 2;
    let mut result = encode_varint(tag as u64);
    result.extend(encode_varint(bytes.len() as u64));
    result.extend_from_slice(bytes);
    result
}

#[test]
fn test_claude_source_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"type":"user","timestamp":"2026-05-20T02:00:00Z","message":{"role":"user","content":"Hello Claude"},"sessionId":"session123","cwd":"/path/to/project","slug":"test-session"}
{"type":"assistant","timestamp":"2026-05-20T02:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Hello User"}]}}
"#,
        ).unwrap();

        let source = ClaudeSource;
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.id, "session123");
        assert_eq!(session.cwd, Some("/path/to/project".to_string()));
        assert_eq!(session.thread_name, Some("Test session".to_string()));
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Hello Claude");
        assert_eq!(session.turns[0].assistant_message, "Hello User");
    });
}

#[test]
fn test_claude_compaction_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"type":"user","timestamp":"2026-05-20T02:00:00Z","message":{"role":"user","content":"Hello Claude"},"sessionId":"sessionCompact","cwd":"/path/to/project","slug":"test-session"}
{"parentUuid":null,"logicalParentUuid":"123","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"timestamp":"2026-05-20T02:00:30Z","uuid":"abc","level":"info","compactMetadata":{"trigger":"auto","preTokens":1000,"postTokens":100,"durationMs":5000},"sessionId":"sessionCompact"}
{"type":"assistant","timestamp":"2026-05-20T02:01:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Hello User"}]}}
"#,
        ).unwrap();

        let source = ClaudeSource;
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.id, "sessionCompact");
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Hello Claude");
        assert_eq!(session.turns[0].assistant_message, "Hello User");
        assert_eq!(session.turns[0].extra_data.get("isCompaction").map(|s| s.as_str()), Some("true"));
        assert_eq!(session.turns[0].extra_data.get("compactionTimeMs").map(|s| s.as_str()), Some("5000"));
    });
}

#[test]
fn test_antigravity_source_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:00:00Z","content":"<USER_REQUEST>Hello Antigravity</USER_REQUEST>"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-05-20T02:01:00Z","content":"Hello back"}
{"step_index":2,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:02:00Z","content":"<USER_REQUEST>Another query</USER_REQUEST><USER_SETTINGS_CHANGE>\nThe user changed setting `Model Selection` from Gemini 3.5 Flash (High) to Claude Sonnet 4.6 (Thinking).\n</USER_SETTINGS_CHANGE>"}
{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-05-20T02:03:00Z","content":"Sure"}
{"step_index":4,"source":"MODEL","type":"RUN_COMMAND","status":"DONE","created_at":"2026-05-20T02:04:00Z","content":"Running ls","tool_calls":[{"name":"run_command","args":{"CommandLine":"\"ls -la\"","Cwd":"\"/Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba2\""}}]}
"#,
        ).unwrap();

        let source = AntigravitySource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].user_message, "Hello Antigravity");
        assert_eq!(session.turns[0].assistant_message, "Hello back");
        assert_eq!(session.turns[0].extra_data.get("model").map(|s| s.as_str()), Some("Unknown"));
        assert_eq!(session.turns[0].extra_data.get("computeTimeMs").map(|s| s.as_str()), Some("60000"));

        assert_eq!(session.turns[1].user_message, "Another query");
        assert!(session.turns[1].assistant_message.contains("Sure"));
        assert!(session.turns[1].assistant_message.contains("[[[TOOL:RUN_COMMAND|⚡ Run Command: ls -la"));
        assert_eq!(session.turns[1].extra_data.get("model").map(|s| s.as_str()), Some("Claude Sonnet 4.6 (Thinking)"));
        assert_eq!(session.turns[1].extra_data.get("computeTimeMs").map(|s| s.as_str()), Some("120000"));
        assert_eq!(session.cwd, Some("/Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba2".to_string()));
    });
}

#[test]
fn test_antigravity_system_and_error_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:00:00Z","content":"<USER_REQUEST>Start</USER_REQUEST>"}
{"step_index":1,"source":"SYSTEM","type":"SYSTEM_MESSAGE","status":"DONE","created_at":"2026-05-20T02:01:00Z","content":"<SYSTEM_MESSAGE>Compilation complete</SYSTEM_MESSAGE>"}
{"step_index":2,"source":"SYSTEM","type":"ERROR_MESSAGE","status":"DONE","created_at":"2026-05-20T02:02:00Z","content":"Command failed with status 1"}
{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-05-20T02:03:00Z","content":"Done"}
"#,
        ).unwrap();

        let source = AntigravitySource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Start");
        assert!(session.turns[0].assistant_message.contains("[[[TOOL:SYSTEM_MESSAGE|⚙️ System Message"));
        assert!(session.turns[0].assistant_message.contains("Compilation complete"));
        assert!(session.turns[0].assistant_message.contains("[[[TOOL:ERROR_MESSAGE|❌ Error"));
        assert!(session.turns[0].assistant_message.contains("Command failed with status 1"));
        assert!(session.turns[0].assistant_message.contains("Done"));
    });
}

#[test]
fn test_codex_source_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::Builder::new().prefix("rollout-").suffix(".jsonl").tempfile().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"timestamp":"2026-05-20T02:00:00Z","type":"session_meta","payload":{"id":"codex123","timestamp":"2026-05-20T02:00:00Z","cwd":"/path/to/codex"}}
{"timestamp":"2026-05-20T02:01:00Z","type":"response_item","payload":{"role":"user","content":[{"text":"Hi Codex"}]}}
{"timestamp":"2026-05-20T02:02:00Z","type":"response_item","payload":{"role":"assistant","content":[{"text":"Hi human"}]}}
"#,
        ).unwrap();

        let source = CodexSource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.id, "codex123");
        assert_eq!(session.cwd, Some("/path/to/codex".to_string()));
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Hi Codex");
        assert_eq!(session.turns[0].assistant_message, "Hi human");
    });
}

#[test]
fn test_aider_source_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"# Aider chat started at 2026-05-20 12:00:00

#### User:
Explain recursion.

#### Assistant:
Recursion is when a function calls itself.
"#,
        ).unwrap();

        let source = AiderSource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Explain recursion.");
        assert_eq!(session.turns[0].assistant_message, "Recursion is when a function calls itself.");
    });
}

#[test]
fn test_copilot_source_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_yaml = temp_dir.path().join("workspace.yaml");
        let events_jsonl = temp_dir.path().join("events.jsonl");

        fs::write(
            &workspace_yaml,
            r#"id: copilot-session-123
name: Code review audit
cwd: /path/to/project
branch: main
repository: LookAtWhatAiCanDo/Codeoba
created_at: 2026-06-10T14:10:14.691Z
updated_at: 2026-06-10T21:10:21.486Z
"#,
        ).unwrap();

        fs::write(
            &events_jsonl,
            r#"{"type":"user.message","timestamp":"2026-06-10T21:10:16.036Z","data":{"content":"review and audit this code"}}
{"type":"tool.execution_start","timestamp":"2026-06-10T21:10:21.480Z","data":{"toolCallId":"call_1","toolName":"run_command","arguments":{"CommandLine":"ls -la"}}}
{"type":"tool.execution_complete","timestamp":"2026-06-10T21:10:21.483Z","data":{"toolCallId":"call_1","success":true,"result":{"content":"Intent logged","detailedContent":"Reviewing codebase"}}}
{"type":"assistant.message","timestamp":"2026-06-10T21:10:21.479Z","data":{"content":"Reviewing the current diff now...","reasoningText":"Let me start by checking files.","model":"gpt-4o"}}
"#,
        ).unwrap();

        let source = CopilotSource::new();
        let session = source.parse_session(&events_jsonl.to_string_lossy()).await.unwrap();

        assert_eq!(session.id, "copilot-session-123");
        assert_eq!(session.cwd, Some("/path/to/project".to_string()));
        assert_eq!(session.thread_name, Some("Code review audit".to_string()));
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "review and audit this code");

        let assistant_text = &session.turns[0].assistant_message;
        assert!(assistant_text.contains("> [!NOTE]"));
        assert!(assistant_text.contains("**Reasoning:**"));
        assert!(assistant_text.contains("Let me start by checking files."));
        assert!(assistant_text.contains("Reviewing the current diff now..."));
        assert!(assistant_text.contains("[[[TOOL:RUN_COMMAND|⚡ Run Command: ls -la"));
        assert!(assistant_text.contains("Reviewing codebase"));

        assert_eq!(session.turns[0].extra_data.get("model").map(|s| s.as_str()), Some("gpt-4o"));
    });
}

#[test]
fn test_antigravity_protobuf_wire_format_title_resolution() {
    tauri::async_runtime::block_on(async {
        with_mock_home(|mock_home| async move {
            let gemini_dir = mock_home.join(".gemini/antigravity");
            fs::create_dir_all(&gemini_dir).unwrap();

            let uuid_bytes = "session-12345".as_bytes();
            let uuid_field = encode_length_delimited(1, uuid_bytes);

            let title_bytes = "Exploring Quantum Physics".as_bytes();
            let title_field = encode_length_delimited(1, title_bytes);
            let info_field = encode_length_delimited(2, &title_field);
            let entry_field = encode_length_delimited(1, &[uuid_field, info_field].concat());

            let pb_file = gemini_dir.join("agyhub_summaries_proto.pb");
            fs::write(&pb_file, &entry_field).unwrap();

            let source = AntigravitySource::new();
            let title = source.get_session_title("session-12345");
            assert_eq!(title, "Exploring Quantum Physics");
        }).await;
    });
}

#[test]
fn test_antigravity_archived_parsing() {
    tauri::async_runtime::block_on(async {
        with_mock_home(|mock_home| async move {
            let gemini_dir = mock_home.join(".gemini/antigravity");
            let brain_dir = gemini_dir.join("brain");
            let session_dir = brain_dir.join("session-archived/.system_generated/logs");
            let annotations_dir = gemini_dir.join("annotations");

            fs::create_dir_all(&session_dir).unwrap();
            fs::create_dir_all(&annotations_dir).unwrap();

            let transcript_file = session_dir.join("transcript.jsonl");
            fs::write(
                &transcript_file,
                r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:00:00Z","content":"<USER_REQUEST>Archived test</USER_REQUEST>"}
"#,
            ).unwrap();

            let source = AntigravitySource::new();

            let session1 = source.parse_session(&transcript_file.to_string_lossy()).await.unwrap();
            assert_eq!(session1.is_archived, false);

            let annotation_file = annotations_dir.join("session-archived.pbtxt");
            fs::write(&annotation_file, "archived:true last_user_view_time:{seconds:1234 nanos:567}").unwrap();

            let session2 = source.parse_session(&transcript_file.to_string_lossy()).await.unwrap();
            assert_eq!(session2.is_archived, true);

            fs::write(&annotation_file, "archived:false last_user_view_time:{seconds:1234 nanos:567}").unwrap();

            let session3 = source.parse_session(&transcript_file.to_string_lossy()).await.unwrap();
            assert_eq!(session3.is_archived, false);
        }).await;
    });
}

#[test]
fn test_codex_archived_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_dir = tempfile::tempdir().unwrap();
        let sessions_dir = temp_dir.path().join("sessions");
        let archived_sessions_dir = temp_dir.path().join("archived_sessions");

        fs::create_dir_all(&sessions_dir).unwrap();
        fs::create_dir_all(&archived_sessions_dir).unwrap();

        let active_file = sessions_dir.join("rollout-codex123.jsonl");
        fs::write(
            &active_file,
            r#"{"timestamp":"2026-05-20T02:00:00Z","type":"session_meta","payload":{"id":"codex123","timestamp":"2026-05-20T02:00:00Z","cwd":"/path/to/codex"}}
{"timestamp":"2026-05-20T02:01:00Z","type":"response_item","payload":{"role":"user","content":[{"text":"Hi Codex"}]}}
{"timestamp":"2026-05-20T02:02:00Z","type":"response_item","payload":{"role":"assistant","content":[{"text":"Hi human"}]}}
"#,
        ).unwrap();

        let archived_file = archived_sessions_dir.join("rollout-codex456.jsonl");
        fs::write(
            &archived_file,
            r#"{"timestamp":"2026-05-20T02:00:00Z","type":"session_meta","payload":{"id":"codex456","timestamp":"2026-05-20T02:00:00Z","cwd":"/path/to/codex"}}
{"timestamp":"2026-05-20T02:01:00Z","type":"response_item","payload":{"role":"user","content":[{"text":"Hi Codex"}]}}
{"timestamp":"2026-05-20T02:02:00Z","type":"response_item","payload":{"role":"assistant","content":[{"text":"Hi human"}]}}
"#,
        ).unwrap();

        let source = CodexSource::new();

        let active_session = source.parse_session(&active_file.to_string_lossy()).await.unwrap();
        assert_eq!(active_session.is_archived, false);

        let archived_session = source.parse_session(&archived_file.to_string_lossy()).await.unwrap();
        assert_eq!(archived_session.is_archived, true);
    });
}

#[test]
fn test_antigravity_tool_tags_edge_cases() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-05-20T02:00:00Z","content":"<USER_REQUEST>Search for [[[TOOL:</USER_REQUEST>"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-05-20T02:01:00Z","content":"I will search for `[[[TOOL:` now."}
{"step_index":2,"source":"MODEL","type":"GREP_SEARCH","status":"DONE","created_at":"2026-05-20T02:02:00Z","content":"Found: return \"[[[TOOL:\"","tool_calls":[{"name":"grep_search","args":{"Query":"\"[[[TOOL:\""}}]}
"#,
        ).unwrap();

        let source = AntigravitySource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].user_message, "Search for [[[TOOL:");

        let assistant_text = &session.turns[0].assistant_message;
        assert!(assistant_text.contains("I will search for `\\[\\[\\[TOOL:` now."));
        assert!(assistant_text.contains("[[[TOOL:GREP_SEARCH|🔍 Search: Query: \\[\\[\\[TOOL:|1779242520000]]]"));
        assert!(assistant_text.contains("Found: return \"\\[\\[\\[TOOL:\""));
    });
}

#[test]
fn test_aider_generic_level_4_headings() {
    tauri::async_runtime::block_on(async {
        let temp_file = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        fs::write(
            &temp_path,
            r#"# Aider chat started at 2026-05-20 12:00:00

#### User:
Please fix the bug.
#### Steps to reproduce:
1. Open the file.
2. Run the code.

#### Assistant:
I will fix it.
#### Summary:
Done.
"#,
        ).unwrap();

        let source = AiderSource::new();
        let session = source.parse_session(&temp_path).await.unwrap();

        assert_eq!(session.turns.len(), 1);
        assert_eq!(
            session.turns[0].user_message,
            "Please fix the bug.\n#### Steps to reproduce:\n1. Open the file.\n2. Run the code."
        );
        assert_eq!(
            session.turns[0].assistant_message,
            "I will fix it.\n#### Summary:\nDone."
        );
    });
}

#[test]
fn test_cursor_windows_path_stripping() {
    let paths = vec![
        ("file:///C:/Users/pv/Dev/Project", "C:/Users/pv/Dev/Project"),
        ("file:///D:/Work", "D:/Work"),
        ("file:///etc/hosts", "/etc/hosts"),
        ("/Users/pv/Dev", "/Users/pv/Dev"),
    ];
    for (input, expected) in paths {
        let mut folder_path = if input.starts_with("file://") {
            input.trim_start_matches("file://").to_string()
        } else {
            input.to_string()
        };
        if folder_path.starts_with('/') && folder_path.len() > 2 && folder_path.as_bytes()[2] == b':' {
            folder_path = folder_path[1..].to_string();
        }
        assert_eq!(folder_path, expected);
    }
}

#[test]
fn test_cursor_sqlite_parsing() {
    tauri::async_runtime::block_on(async {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("state.vscdb");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);",
            [],
        ).unwrap();

        let value_str = r#"{"name":"Feature development","createdAt":1779242400000,"lastUpdatedAt":1779242460000,"conversation":[{"type":1,"text":"Create login screen","model":"gpt-4o"},{"type":2,"text":"Okay, creating..."}]}"#;
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES ('composerData:session123', ?1);",
            [value_str],
        ).unwrap();

        let ws_dir = temp_dir.path().join("workspaceStorage");
        let ws_sub_dir = ws_dir.join("workspace-abc");
        fs::create_dir_all(&ws_sub_dir).unwrap();

        let ws_json = ws_sub_dir.join("workspace.json");
        fs::write(&ws_json, r#"{"folder":"file:///Users/pv/Dev/Project"}"#).unwrap();

        let ws_db = ws_sub_dir.join("state.vscdb");
        let ws_conn = Connection::open(&ws_db).unwrap();
        ws_conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);",
            [],
        ).unwrap();
        ws_conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('composer.composerData', '{\"allComposers\": [{\"composerId\": \"session123\"}]}');",
            [],
        ).unwrap();

        with_mock_home(|mock_home| async move {
            let cursor_dir = mock_home.join("Library/Application Support/Cursor/User");
            if cfg!(target_os = "windows") {
                let win_cursor_dir = mock_home.join("AppData/Roaming/Cursor/User");
                fs::create_dir_all(win_cursor_dir.join("globalStorage")).unwrap();
                fs::copy(&db_path, win_cursor_dir.join("globalStorage/state.vscdb")).unwrap();
                
                let ws_target_dir = win_cursor_dir.join("workspaceStorage");
                fs::create_dir_all(ws_target_dir.join("workspace-abc")).unwrap();
                fs::copy(&ws_json, ws_target_dir.join("workspace-abc/workspace.json")).unwrap();
                fs::copy(&ws_db, ws_target_dir.join("workspace-abc/state.vscdb")).unwrap();
            } else {
                fs::create_dir_all(cursor_dir.join("globalStorage")).unwrap();
                fs::copy(&db_path, cursor_dir.join("globalStorage/state.vscdb")).unwrap();
                
                let ws_target_dir = cursor_dir.join("workspaceStorage");
                fs::create_dir_all(ws_target_dir.join("workspace-abc")).unwrap();
                fs::copy(&ws_json, ws_target_dir.join("workspace-abc/workspace.json")).unwrap();
                fs::copy(&ws_db, ws_target_dir.join("workspace-abc/state.vscdb")).unwrap();
            }

            let source = CursorSource::new();
            let sessions = source.parse_all_sessions().await;

            assert_eq!(sessions.len(), 1);
            let s = &sessions[0];
            assert_eq!(s.id, "session123");
            assert_eq!(s.thread_name, Some("Feature development".to_string()));
            assert_eq!(s.turns.len(), 1);
            assert_eq!(s.turns[0].user_message, "Create login screen");
            assert_eq!(s.turns[0].assistant_message, "Okay, creating...");
        }).await;
    });
}

#[test]
fn test_antigravity_tool_tags_edge_cases_parser() {
    let text1 = "Preceding text [[[TOOL:GREP_SEARCH|Search|123]]] Tool content without closing tag.\nSubsequent dialogue text.";
    let parts1 = temp_parse_assistant_message(text1);
    assert_eq!(parts1.len(), 3);
    assert_eq!(parts1[0], TempMessagePart::Text("Preceding text ".to_string()));
    assert_eq!(parts1[1], TempMessagePart::Text("[[[TOOL:".to_string()));
    assert_eq!(parts1[2], TempMessagePart::Text("GREP_SEARCH|Search|123]]] Tool content without closing tag.\nSubsequent dialogue text.".to_string()));

    let text2 = "This is an escaped tag: \\[\\[\\[TOOL:GREP_SEARCH]]], and an unescaped tag: [[[TOOL:VIEW_FILE|View|456]]]\nContent\n[[[/TOOL]]]";
    let parts2 = temp_parse_assistant_message(text2);
    assert_eq!(parts2.len(), 2);
    assert_eq!(parts2[0], TempMessagePart::Text("This is an escaped tag: [[[TOOL:GREP_SEARCH]]], and an unescaped tag: ".to_string()));
    assert_eq!(parts2[1], TempMessagePart::Tool {
        tool_type: "VIEW_FILE".to_string(),
        header: "View".to_string(),
        content: "\nContent\n".to_string(),
        timestamp: 456,
    });
}
