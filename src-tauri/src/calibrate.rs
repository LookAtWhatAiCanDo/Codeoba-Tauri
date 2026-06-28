use codeoba_lib::tokenizer::estimate_tokens;
use codeoba_lib::parsers::{SourceAdapter, claude::ClaudeSource};

fn main() {
    println!("==================================================");
    println!("        Codeoba Tokenization Calibration Utility   ");
    println!("==================================================");

    // 1. Run local calibrated checks
    println!("\n--- 1. Testing Default Calibrated Estimator Output ---");
    let test_string = "fn main() {\n    println!(\"Hello World!\");\n}";
    println!("Test string ({} bytes):\n{}", test_string.len(), test_string);

    for model in &["gpt-4", "claude-3-opus", "gemini-pro", "llama-3-70b", "unknown-model"] {
        let count = estimate_tokens(test_string, model);
        println!("  - Model: {:<15} -> Estimated Tokens: {}", model, count);
    }

    // 2. Guide the user on custom tokenizers
    println!("\n--- 2. How to Enable 100% Precise Tokenization ---");
    println!("Codeoba supports loading official Hugging Face tokenizer configurations.");
    println!("To configure precise token counting:");
    println!("  1. Download a 'tokenizer.json' file (e.g. for cl100k_base/GPT-4, Llama, etc.).");
    println!("  2. Save it to: ~/.codeoba/tokenizers/<family>.json");
    println!("     Supported families: 'cl100k_base', 'claude', 'gemini', 'llama'.");
    println!("     Example: ~/.codeoba/tokenizers/cl100k_base.json");

    // 3. Search and calibrate against actual Claude compactions if available
    println!("\n--- 3. Scanning for local Claude Code compactions to calibrate ---");
    let claude = ClaudeSource;
    if claude.is_available() {
        println!("Claude Code is installed. Scanning project logs...");
        tauri::async_runtime::block_on(async {
            let sessions = claude.parse_all_sessions().await;
            let mut compactions_checked = 0;

            for session in sessions {
                for turn in session.turns {
                    if turn.extra_data.contains_key("isCompaction") {
                        if let Some(comp_time) = turn.extra_data.get("compactionTimeMs") {
                            println!("\nFound Compaction Event in session: {}", session.id);
                            println!("  Compaction Duration: {} ms", comp_time);

                            let est_user = turn.input_tokens.unwrap_or(0);
                            let est_asst = turn.output_tokens.unwrap_or(0);
                            println!("  Our Offline Turn Estimates: User = {} tokens, Assistant = {} tokens", est_user, est_asst);
                            compactions_checked += 1;
                        }
                    }
                }
            }

            if compactions_checked == 0 {
                println!("No compaction history found in your local Claude Code logs yet.");
            } else {
                println!("\nChecked {} compaction(s) for calibration.", compactions_checked);
            }
        });
    } else {
        println!("Claude Code was not detected on this system. Skipping log scanning.");
    }
    println!("\n==================================================");
}
