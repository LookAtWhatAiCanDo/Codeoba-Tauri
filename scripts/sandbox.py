#!/usr/bin/env python3
import os
import shutil
import sqlite3
import sys
import argparse
import datetime

def encode_varint(value):
    list_bytes = []
    temp = value
    while True:
        if (temp & ~0x7F) == 0:
            list_bytes.append(temp)
            break
        else:
            list_bytes.append((temp & 0x7F) | 0x80)
            temp >>= 7
    return bytes(list_bytes)

def encode_length_delimited(field_number, data):
    tag = (field_number << 3) | 2
    return encode_varint(tag) + encode_varint(len(data)) + data

def get_cursor_dir(base_dir):
    if sys.platform == "darwin":
        return os.path.join(base_dir, "Library/Application Support/Cursor/User")
    elif sys.platform == "win32":
        return os.path.join(base_dir, "AppData/Roaming/Cursor/User")
    else:
        return os.path.join(base_dir, ".config/Cursor/User")

def get_session_times(file_path, extra_turn=False):
    now = datetime.datetime.now(datetime.timezone.utc)
    
    # Check if file exists to get original creation time
    original_time = None
    if os.path.exists(file_path):
        try:
            stat = os.stat(file_path)
            # Use birthtime if available (macOS), otherwise ctime
            timestamp = getattr(stat, 'st_birthtime', stat.st_ctime)
            original_time = datetime.datetime.fromtimestamp(timestamp, datetime.timezone.utc)
        except Exception:
            pass
            
    if not original_time:
        original_time = now
        
    def format_time(dt):
        iso = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
        ms = int(dt.timestamp() * 1000)
        return iso, ms
        
    init_iso, init_ms = format_time(original_time)
    now_iso, now_ms = format_time(now)
    
    local_init = datetime.datetime.fromtimestamp(original_time.timestamp())
    aider_init = local_init.strftime("%Y-%m-%d %H:%M:%S")
    
    return {
        "init_iso": init_iso,
        "init_ms": init_ms,
        "now_iso": now_iso,
        "now_ms": now_ms,
        "aider_init": aider_init,
    }

def write_cursor(base_dir, extra_turn=False):
    user_dir = get_cursor_dir(base_dir)
    global_dir = os.path.join(user_dir, "globalStorage")
    ws_dir = os.path.join(user_dir, "workspaceStorage/workspace-demo")
    os.makedirs(global_dir, exist_ok=True)
    os.makedirs(ws_dir, exist_ok=True)

    db_path = os.path.join(global_dir, "state.vscdb")
    times = get_session_times(db_path, extra_turn)

    conn = sqlite3.connect(db_path)
    conn.execute("DROP TABLE IF EXISTS cursorDiskKV;")
    conn.execute("CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);")
    
    turns = [
        '{"type": 1, "text": "Hey Cursor, this text is exactly twenty characters.", "model": "gpt-4o"}',
        '{"type": 2, "text": "Understood. The response contains exactly thirty-three characters."}'
    ]
    if extra_turn:
        turns.append('{"type": 1, "text": "Cursor extra turn appended.", "model": "gpt-4o"}')
        turns.append('{"type": 2, "text": "Turn registered."}')

    conversation_json = ",".join(turns)
    cursor_session_val = f"""{{
        "name": "Cursor Demo Session",
        "createdAt": {times["init_ms"]},
        "lastUpdatedAt": {times["now_ms"] if extra_turn else times["init_ms"]},
        "conversation": [{conversation_json}]
    }}"""
    conn.execute("INSERT INTO cursorDiskKV (key, value) VALUES ('composerData:session-cursor-demo', ?);", (cursor_session_val,))
    conn.commit()
    conn.close()

    ws_json = os.path.join(ws_dir, "workspace.json")
    with open(ws_json, "w") as f:
        f.write('{"folder":"file:///Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba"}')

    ws_db = os.path.join(ws_dir, "state.vscdb")
    conn_ws = sqlite3.connect(ws_db)
    conn_ws.execute("DROP TABLE IF EXISTS ItemTable;")
    conn_ws.execute("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);")
    conn_ws.execute("INSERT INTO ItemTable (key, value) VALUES ('composer.composerData', '{\"allComposers\": [{\"composerId\": \"session-cursor-demo\"}]}');")
    conn_ws.commit()
    conn_ws.close()
    print("Generated Cursor SQLite mock.")

def write_claude(base_dir, extra_turn=False):
    claude_projects_dir = os.path.join(base_dir, ".claude/projects/project-demo")
    claude_plans_dir = os.path.join(base_dir, ".claude/plans")
    os.makedirs(claude_projects_dir, exist_ok=True)
    os.makedirs(claude_plans_dir, exist_ok=True)

    claude_log = os.path.join(claude_projects_dir, "session-claude-demo.jsonl")
    times = get_session_times(claude_log, extra_turn)

    with open(claude_log, "w") as f:
        f.write(f'{{"type":"user","timestamp":"{times["init_iso"]}","message":{{"role":"user","content":"Verify Claude fallback formula."}},"sessionId":"session-claude-demo","cwd":"/Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba","slug":"claude-plan-slug"}}\n')
        f.write(f'{{"parentUuid":null,"logicalParentUuid":"123","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Compacted","isMeta":false,"timestamp":"{times["init_iso"]}","uuid":"abc","level":"info","compactMetadata":{{"durationMs":8000}},"sessionId":"session-claude-demo"}}\n')
        f.write(f'{{"type":"assistant","timestamp":"{times["init_iso"]}","message":{{"role":"assistant","content":[{{"type":"text","text":"Claude reply verified."}}]}}}}\n')
        if extra_turn:
            f.write(f'{{"type":"user","timestamp":"{times["now_iso"]}","message":{{"role":"user","content":"Claude extra turn."}},"sessionId":"session-claude-demo"}}\n')
            f.write(f'{{"type":"assistant","timestamp":"{times["now_iso"]}","message":{{"role":"assistant","content":[{{"type":"text","text":"Spawning reply."}}]}}}}\n')

    claude_plan = os.path.join(claude_plans_dir, "claude-plan-slug.md")
    with open(claude_plan, "w") as f:
        f.write("# Goal: Claude Demo Session\nVerification plan.")
    print("Generated Claude Code JSONL mock.")

def write_aider(base_dir, extra_turn=False):
    aider_dir = os.path.join(base_dir, "Dev/aider-demo")
    os.makedirs(aider_dir, exist_ok=True)

    aider_log = os.path.join(aider_dir, ".aider.chat.history.md")
    times = get_session_times(aider_log, extra_turn)

    with open(aider_log, "w") as f:
        f.write(f'# Aider chat started at {times["aider_init"]}\n\n')
        f.write('#### User:\n')
        f.write('Aider user query prompt.\n\n')
        f.write('#### Assistant:\n')
        f.write('Aider assistant reply text here.\n')
        if extra_turn:
            f.write('\n#### User:\n')
            f.write('Aider extra turn prompt.\n\n')
            f.write('#### Assistant:\n')
            f.write('Aider extra reply.\n')
    print("Generated Aider Markdown mock.")

def write_copilot(base_dir, extra_turn=False):
    copilot_dir = os.path.join(base_dir, ".copilot/session-state/session-copilot-demo")
    os.makedirs(copilot_dir, exist_ok=True)

    events_jsonl = os.path.join(copilot_dir, "events.jsonl")
    times = get_session_times(events_jsonl, extra_turn)

    ws_yaml = os.path.join(copilot_dir, "workspace.yaml")
    with open(ws_yaml, "w") as f:
        f.write(f"""id: session-copilot-demo
name: Copilot Demo Session
cwd: /Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba
branch: main
created_at: {times["init_iso"]}
updated_at: {times["now_iso"] if extra_turn else times["init_iso"]}
""")

    with open(events_jsonl, "w") as f:
        f.write(f'{{"type":"user.message","timestamp":"{times["init_iso"]}.000Z","data":{{"content":"Copilot user query."}}}}\n')
        f.write(f'{{"type":"assistant.message","timestamp":"{times["init_iso"]}.000Z","data":{{"content":"Copilot assistant reply.","model":"gpt-4o"}}}}\n')
        if extra_turn:
            f.write(f'{{"type":"user.message","timestamp":"{times["now_iso"]}.000Z","data":{{"content":"Copilot extra turn."}}}}\n')
            f.write(f'{{"type":"assistant.message","timestamp":"{times["now_iso"]}.000Z","data":{{"content":"Reply.","model":"gpt-4o"}}}}\n')
    print("Generated Copilot YAML/JSONL mock.")

def write_codex(base_dir, extra_turn=False):
    codex_dir = os.path.join(base_dir, ".codex")
    codex_sessions_dir = os.path.join(codex_dir, "sessions")
    os.makedirs(codex_sessions_dir, exist_ok=True)

    codex_index = os.path.join(codex_dir, "session_index.jsonl")
    with open(codex_index, "w") as f:
        f.write('{"id":"codex-demo","thread_name":"Codex Demo Session"}\n')

    rollout_file = os.path.join(codex_sessions_dir, "rollout-codex-demo.jsonl")
    times = get_session_times(rollout_file, extra_turn)

    with open(rollout_file, "w") as f:
        f.write(f'{{"timestamp":"{times["init_iso"]}","type":"session_meta","payload":{{"id":"codex-demo","timestamp":"{times["init_iso"]}","cwd":"/Users/pv/Dev/GitHub/LookAtWhatAiCanDo/Codeoba"}}}}\n')
        f.write(f'{{"timestamp":"{times["init_iso"]}","type":"response_item","payload":{{"role":"user","content":[{{"text":"Codex user query."}}]}}}}\n')
        f.write(f'{{"timestamp":"{times["init_iso"]}","type":"response_item","payload":{{"role":"assistant","content":[{{"text":"Codex assistant reply."}}]}}}}\n')
        if extra_turn:
            f.write(f'{{"timestamp":"{times["now_iso"]}","type":"response_item","payload":{{"role":"user","content":[{{"text":"Codex extra query."}}]}}}}\n')
            f.write(f'{{"timestamp":"{times["now_iso"]}","type":"response_item","payload":{{"role":"assistant","content":[{{"text":"Codex extra reply."}}]}}}}\n')
    print("Generated Codex JSONL mock.")

def write_antigravity(base_dir, extra_turn=False):
    antigravity_dir = os.path.join(base_dir, ".gemini/antigravity")
    logs_dir = os.path.join(antigravity_dir, "brain/session-antigravity-demo/.system_generated/logs")
    os.makedirs(logs_dir, exist_ok=True)

    pb_file = os.path.join(antigravity_dir, "agyhub_summaries_proto.pb")
    uuid_bytes = "session-antigravity-demo".encode('utf-8')
    uuid_field = encode_length_delimited(1, uuid_bytes)
    title_bytes = "Antigravity Demo Session".encode('utf-8')
    title_field = encode_length_delimited(1, title_bytes)
    info_field = encode_length_delimited(2, title_field)
    entry_field = encode_length_delimited(1, uuid_field + info_field)
    with open(pb_file, "wb") as f:
        f.write(entry_field)

    transcript_file = os.path.join(logs_dir, "transcript.jsonl")
    times = get_session_times(transcript_file, extra_turn)

    with open(transcript_file, "w") as f:
        f.write(f'{{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"{times["init_iso"]}","content":"<USER_REQUEST>Antigravity user query.</USER_REQUEST>"}}\n')
        f.write(f'{{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"{times["init_iso"]}","content":"Antigravity assistant reply."}}\n')
        if extra_turn:
            f.write(f'{{"step_index":2,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"{times["now_iso"]}","content":"<USER_REQUEST>Antigravity extra turn.</USER_REQUEST>"}}\n')
            f.write(f'{{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"{times["now_iso"]}","content":"Reply."}}\n')
    print("Generated Antigravity JSONL mock.")

def delete_source(base_dir, source):
    if source == "cursor":
        shutil.rmtree(get_cursor_dir(base_dir), ignore_errors=True)
    elif source == "claude":
        shutil.rmtree(os.path.join(base_dir, ".claude"), ignore_errors=True)
    elif source == "aider":
        shutil.rmtree(os.path.join(base_dir, "Dev/aider-demo"), ignore_errors=True)
    elif source == "copilot":
        shutil.rmtree(os.path.join(base_dir, ".copilot"), ignore_errors=True)
    elif source == "codex":
        shutil.rmtree(os.path.join(base_dir, ".codex"), ignore_errors=True)
    elif source == "antigravity":
        shutil.rmtree(os.path.join(base_dir, ".gemini"), ignore_errors=True)
    print(f"Deleted {source} mock files.")

def main():
    parser = argparse.ArgumentParser(description="Interactive sandboxed telemetry mock generator for Codeoba.")
    parser.add_argument("--init", action="store_true", help="Clear the mock environment and set up empty base folders.")
    parser.add_argument("--all", action="store_true", help="Generate all mock log profiles at once.")
    parser.add_argument("--cursor", action="store_true", help="Generate Cursor mock SQLite DB.")
    parser.add_argument("--claude", action="store_true", help="Generate Claude Code mock project JSONL.")
    parser.add_argument("--aider", action="store_true", help="Generate Aider mock chat history Markdown.")
    parser.add_argument("--copilot", action="store_true", help="Generate Copilot mock events logs.")
    parser.add_argument("--codex", action="store_true", help="Generate Codex mock rollout logs.")
    parser.add_argument("--antigravity", action="store_true", help="Generate Antigravity mock transcripts.")
    
    parser.add_argument("--add-turn", choices=["cursor", "claude", "aider", "copilot", "codex", "antigravity"], help="Append an extra Turn to a specific mock session to trigger a Modify update event.")
    parser.add_argument("--delete", choices=["cursor", "claude", "aider", "copilot", "codex", "antigravity"], help="Delete a specific mock session file to trigger a Deletion update event.")

    args = parser.parse_args()
    base_dir = os.path.abspath("demo_mock")

    # If no arguments provided, default to showing help
    if len(sys.argv) == 1:
        parser.print_help()
        print(f"\nDefaulting to setting up a complete mock environment at: {base_dir}")
        args.all = True

    if args.init:
        if os.path.exists(base_dir):
            shutil.rmtree(base_dir)
        os.makedirs(base_dir)
        os.makedirs(os.path.join(base_dir, "Dev"), exist_ok=True)
        print(f"Initialized empty mock sandbox at: {base_dir}")
        return

    # Create base directory if not exists
    if not os.path.exists(base_dir):
        os.makedirs(base_dir)
        os.makedirs(os.path.join(base_dir, "Dev"), exist_ok=True)

    if args.all:
        write_cursor(base_dir)
        write_claude(base_dir)
        write_aider(base_dir)
        write_copilot(base_dir)
        write_codex(base_dir)
        write_antigravity(base_dir)
    else:
        if args.cursor:
            write_cursor(base_dir)
        if args.claude:
            write_claude(base_dir)
        if args.aider:
            write_aider(base_dir)
        if args.copilot:
            write_copilot(base_dir)
        if args.codex:
            write_codex(base_dir)
        if args.antigravity:
            write_antigravity(base_dir)

    if args.add_turn:
        if args.add_turn == "cursor":
            write_cursor(base_dir, extra_turn=True)
        elif args.add_turn == "claude":
            write_claude(base_dir, extra_turn=True)
        elif args.add_turn == "aider":
            write_aider(base_dir, extra_turn=True)
        elif args.add_turn == "copilot":
            write_copilot(base_dir, extra_turn=True)
        elif args.add_turn == "codex":
            write_codex(base_dir, extra_turn=True)
        elif args.add_turn == "antigravity":
            write_antigravity(base_dir, extra_turn=True)

    if args.delete:
        delete_source(base_dir, args.delete)

    print("\nSandbox command executed!")
    print("To run the app pointing to this sandbox, launch with:")
    print(f"CODEOBA_MOCK_HOME={base_dir} npm run tauri dev")

if __name__ == "__main__":
    main()
