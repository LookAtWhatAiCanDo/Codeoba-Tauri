// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "search" {
        if args.len() < 3 {
            println!("Usage: cargo run -- search \"<query>\" [--semantic]");
            return;
        }
        let query = &args[2];
        let use_semantic = args.iter().any(|arg| arg == "--semantic");

        println!("==================================================");
        println!("           Codeoba CLI Search Tool                ");
        println!("==================================================");
        println!("Query: \"{}\"", query);
        println!("Mode:  {}", if use_semantic { "Semantic" } else { "Lexical" });
        println!("Indexing sessions... please wait...");

        // Initialize cache key synchronously to prevent background race conditions
        let _ = codeoba_lib::keyring::get_or_create_cache_key();

        tauri::async_runtime::block_on(async {
            let state = codeoba_lib::search::SearchIndexState::new();
            if let Err(e) = state.rebuild(use_semantic, None::<tauri::AppHandle>).await {
                println!("Error building index: {}", e);
                return;
            }

            let sessions = {
                let guard = state.sessions.read().unwrap();
                println!("Total sessions in index: {}", guard.len());
                guard.values().cloned().collect::<Vec<_>>()
            };

            let filter = codeoba_lib::search::SearchFilter::default();

            let search_start = std::time::Instant::now();
            let results = if use_semantic {
                let model_path = codeoba_lib::search::downloader::get_model_file();
                let vocab_path = codeoba_lib::search::downloader::get_vocab_file();

                if !model_path.exists() || !vocab_path.exists() {
                    println!("Error: Semantic search is unavailable because the ONNX model or vocab.txt was not found under ~/.codeoba/models/.");
                    return;
                }
                let onnx_embedder = match codeoba_lib::search::semantic::OnnxSemanticEmbedder::new(&model_path, &vocab_path) {
                    Ok(e) => e,
                    Err(err) => {
                        println!("Error loading model: {}", err);
                        return;
                    }
                };
                let query_vector = match onnx_embedder.get_embeddings(query) {
                    Ok(v) => v,
                    Err(err) => {
                        println!("Error calculating query embeddings: {}", err);
                        return;
                    }
                };

                let embeddings_guard = state.embeddings.read().unwrap();
                codeoba_lib::search::semantic::semantic_search(
                    &sessions,
                    &embeddings_guard,
                    &query_vector,
                    0.35,
                    &filter,
                )
            } else {
                codeoba_lib::search::lexical::lexical_search(&sessions, query, &filter)
            };
            println!("[main] Search execution time: {:?}", search_start.elapsed());

            let print_start = std::time::Instant::now();
            println!("\nFound {} matching session(s):\n", results.len());
            for (idx, result) in results.iter().enumerate() {
                let thread_name = result.session.thread_name.as_deref().unwrap_or("Untitled");
                println!("{}. [{}] {} (Score: {:.4})", idx + 1, result.session.source_id, thread_name, result.score);
                println!("   Path: {}", result.session.file_path);
                if !result.matched_turn_indexes.is_empty() {
                    println!("   Matched turn indexes: {:?}", result.matched_turn_indexes);
                    if let Some(&turn_idx) = result.matched_turn_indexes.first() {
                        if let Some(turn) = result.session.turns.get(turn_idx) {
                            let user_snippet = if turn.user_message.len() > 80 {
                                format!("{}...", &turn.user_message[0..80].replace("\n", " "))
                            } else {
                                turn.user_message.replace("\n", " ")
                            };
                            println!("   Snippet (Turn {}): {}", turn_idx, user_snippet);
                        }
                    }
                }
                println!();
            }
            println!("[main] Printing results time: {:?}", print_start.elapsed());
        });
        println!("==================================================");
    } else {
        codeoba_lib::run()
    }
}
