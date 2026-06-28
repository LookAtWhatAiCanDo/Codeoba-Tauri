pub mod lexical;
pub mod tokenizer;
pub mod cache;
pub mod semantic;
pub mod downloader;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ArchivalFilter {
    All,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilter {
    #[serde(default)]
    pub source_ids: HashSet<String>,
    #[serde(default)]
    pub min_timestamp: i64,
    #[serde(default)]
    pub max_timestamp: Option<i64>,
    #[serde(default)]
    pub cwd_filter: Option<String>,
    #[serde(default)]
    pub match_case: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub use_regex: bool,
    #[serde(default = "default_archival_filter")]
    pub archival_filter: ArchivalFilter,
    #[serde(default)]
    pub session_ids: Option<HashSet<String>>,
}

fn default_archival_filter() -> ArchivalFilter {
    ArchivalFilter::All
}

impl Default for SearchFilter {
    fn default() -> Self {
        Self {
            source_ids: HashSet::new(),
            min_timestamp: 0,
            max_timestamp: None,
            cwd_filter: None,
            match_case: false,
            whole_word: false,
            use_regex: false,
            archival_filter: ArchivalFilter::All,
            session_ids: None,
        }
    }
}

impl SearchFilter {
    pub fn matches(&self, session: &crate::models::Session) -> bool {
        if !self.source_ids.is_empty() && !self.source_ids.contains(&session.source_id) {
            return false;
        }
        let max_ts = self.max_timestamp.unwrap_or(i64::MAX);
        if session.updated_at < self.min_timestamp || session.updated_at > max_ts {
            return false;
        }
        if let Some(ref cwd_filter) = self.cwd_filter {
            let cwd = match session.cwd.as_ref() {
                Some(c) => c,
                None => return false,
            };
            if !cwd.to_lowercase().contains(&cwd_filter.to_lowercase()) {
                return false;
            }
        }
        match self.archival_filter {
            ArchivalFilter::Active => {
                if session.is_archived {
                    return false;
                }
            }
            ArchivalFilter::Archived => {
                if !session.is_archived {
                    return false;
                }
            }
            ArchivalFilter::All => {}
        }
        if let Some(ref sids) = self.session_ids {
            if !sids.contains(&session.id) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub session: crate::models::Session,
    pub matched_turn_indexes: Vec<usize>,
    pub score: f32,
}

#[derive(Clone)]
pub struct SessionVectorIndex {
    pub thread_name_embedding: Vec<f32>,
    pub turn_embeddings: Vec<Vec<f32>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingProgress {
    pub step: String,
    pub progress: f32,
    pub current_source: String,
}

pub struct SearchIndexState {
    pub sessions: RwLock<HashMap<String, crate::models::Session>>,
    pub embeddings: RwLock<HashMap<String, SessionVectorIndex>>,
    pub last_progress: RwLock<Option<IndexingProgress>>,
    pub is_rebuilding: std::sync::atomic::AtomicBool,
}

impl SearchIndexState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            embeddings: RwLock::new(HashMap::new()),
            last_progress: RwLock::new(None),
            is_rebuilding: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn load_cached_sessions(&self) {
        let start = std::time::Instant::now();
        let sources = crate::parsers::get_sources_list();
        let mut session_map = HashMap::new();
        
        let cache_mgr = crate::parsers::cache::get_cache_manager();
        for source in &sources {
            if source.is_available() {
                let cache = cache_mgr.load_cache(source.id());
                for entry in cache.into_values() {
                    session_map.insert(entry.session.id.clone(), entry.session);
                }
            }
        }
        
        let count = session_map.len();
        if let Ok(mut guard) = self.sessions.write() {
            *guard = session_map;
        }
        crate::log_info!("[SearchIndexState] Loaded {} cached sessions in {:?}", count, start.elapsed());
    }

    pub async fn rebuild<R: tauri::Runtime>(
        &self, 
        use_semantic: bool,
        app_handle: Option<tauri::AppHandle<R>>,
    ) -> Result<(), String> {
        if self.is_rebuilding.swap(true, std::sync::atomic::Ordering::SeqCst) {
            crate::log_warn!("[rebuild] Rebuild is already in progress. Ignoring concurrent request.");
            return Ok(());
        }

        struct RebuildGuard<'a>(&'a std::sync::atomic::AtomicBool);
        impl<'a> Drop for RebuildGuard<'a> {
            fn drop(&mut self) {
                self.0.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let _guard = RebuildGuard(&self.is_rebuilding);

        let total_start = std::time::Instant::now();
        
        let emit_progress = |step: &str, progress: f32, current_source: &str| {
            let info = IndexingProgress {
                step: step.to_string(),
                progress,
                current_source: current_source.to_string(),
            };
            if let Ok(mut guard) = self.last_progress.write() {
                *guard = Some(info.clone());
            }
            if let Some(ref handle) = app_handle {
                use tauri::Emitter;
                let _ = handle.emit("indexing-progress", info);
            }
        };

        emit_progress("start", 0.0, "Initializing search index...");

        let model_path = downloader::get_model_file();
        let vocab_path = downloader::get_vocab_file();

        let run_embeddings = use_semantic && model_path.exists() && vocab_path.exists();

        let onnx_embedder = if run_embeddings {
            let onnx_load_start = std::time::Instant::now();
            let embedder = semantic::OnnxSemanticEmbedder::new(&model_path, &vocab_path).ok();
            crate::log_info!("[rebuild] ONNX embedder load time: {:?}", onnx_load_start.elapsed());
            embedder
        } else {
            None
        };

        let cache_mgr = if run_embeddings {
            let model_id = "all-MiniLM-L6-v2";
            let mgr = cache::EmbeddingCacheManager::new(model_id);
            let cache_load_start = std::time::Instant::now();
            mgr.load_cache();
            crate::log_info!("[rebuild] Cache load time: {:?}", cache_load_start.elapsed());
            Some(mgr)
        } else {
            None
        };

        let parse_start = std::time::Instant::now();
        let sources = crate::parsers::get_sources_list();
        let mut all_sessions = Vec::new();
        
        let available_sources: Vec<_> = sources.iter().filter(|s| s.is_available()).collect();
        let total_sources = available_sources.len() as f32;
        let mut current_idx = 0;

        for source in available_sources {
            current_idx += 1;
            let pct = 0.05 + (current_idx as f32 / total_sources) * 0.70; // 5% to 75%
            emit_progress("parsing", pct, source.display_name());

            let source_start = std::time::Instant::now();
            all_sessions.extend(source.parse_all_sessions().await);
            crate::log_info!("[rebuild] Parsed source '{}' in {:?}", source.id(), source_start.elapsed());
            tokio::task::yield_now().await;
        }
        crate::log_info!("[rebuild] Total parsing time: {:?}", parse_start.elapsed());

        emit_progress("embedding", 0.80, "Calculating semantic embeddings...");

        let embed_start = std::time::Instant::now();

        let mut session_map = HashMap::new();
        let mut embedding_map = HashMap::new();

        let existing_sessions: Option<HashMap<String, crate::models::Session>> = {
            if let Ok(guard) = self.sessions.read() {
                Some(guard.clone())
            } else {
                None
            }
        };
        let existing_embeddings: Option<HashMap<String, SessionVectorIndex>> = {
            if let Ok(guard) = self.embeddings.read() {
                Some(guard.clone())
            } else {
                None
            }
        };

        let mut sessions_to_embed = Vec::new();
        for session in all_sessions {
            let mut reused = false;
            if run_embeddings {
                if let (Some(ref old_sessions), Some(ref old_embs)) = (&existing_sessions, &existing_embeddings) {
                    if let (Some(old_sess), Some(old_emb)) = (old_sessions.get(&session.id), old_embs.get(&session.id)) {
                        if old_sess.updated_at == session.updated_at && old_sess.turns.len() == session.turns.len() {
                            embedding_map.insert(session.id.clone(), old_emb.clone());
                            session_map.insert(session.id.clone(), session.clone());
                            reused = true;
                        }
                    }
                }
            }

            if !reused {
                sessions_to_embed.push(session);
            }
        }

        let (session_map, embedding_map, final_onnx_invocations, final_cache_hits) = {
            let onnx_emb_val = onnx_embedder;
            let cache_mgr_val = cache_mgr;
            let app_handle_clone = app_handle.clone();
            let run_embeddings_val = run_embeddings;

            tokio::task::spawn_blocking(move || {
                let mut s_map = session_map;
                let mut e_map = embedding_map;

                let onnx_invs = std::sync::atomic::AtomicUsize::new(0);
                let c_hits = std::sync::atomic::AtomicUsize::new(0);

                let onnx_emb = onnx_emb_val;
                let cache_ref = cache_mgr_val;

                let total_to_embed = sessions_to_embed.len();
                crate::log_info!("[rebuild] Starting embedding loop: {} sessions to embed.", total_to_embed);

                for (idx, session) in sessions_to_embed.into_iter().enumerate() {
                    let thread_name = session.thread_name.as_deref().unwrap_or("Untitled Session");

                    // Periodic progress reporting (every 5 sessions, or if it's the last one)
                    if idx % 5 == 0 || idx == total_to_embed - 1 {
                        let pct = 0.80 + (idx as f32 / total_to_embed.max(1) as f32) * 0.19; // 80% to 99%
                        let display_text = format!("Calculating semantic embeddings... ({}/{})", idx + 1, total_to_embed);
                        crate::log_info!("[rebuild] Progress: {}/{} sessions ({}%)", idx + 1, total_to_embed, (pct * 100.0) as u32);
                        
                        let info = IndexingProgress {
                            step: "embedding".to_string(),
                            progress: pct,
                            current_source: display_text,
                        };
                        
                        if let Some(ref handle) = app_handle_clone {
                            use tauri::Manager;
                            let state = handle.state::<SearchIndexState>();
                            if let Ok(mut guard) = state.last_progress.write() {
                                *guard = Some(info.clone());
                            }
                            use tauri::Emitter;
                            let _ = handle.emit("indexing-progress", info);
                        }
                    }

                    let mut vec_index = None;
                    if let Some(ref cache) = cache_ref {
                        let hash_emb = semantic::HashSemanticEmbedder::new(384);
                        
                        let thread_emb = if let Some(v) = cache.get(thread_name) {
                            c_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            v
                        } else {
                            let v = if let Some(ref embedder) = onnx_emb {
                                onnx_invs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                embedder.get_embeddings(thread_name).unwrap_or_else(|e| {
                                    crate::log_warn!("[rebuild]       ONNX error on thread name: {}. Falling back to hash.", e);
                                    hash_emb.get_embeddings(thread_name)
                                })
                            } else {
                                hash_emb.get_embeddings(thread_name)
                            };
                            cache.put(thread_name, v.clone());
                            v
                        };

                        let mut turn_embs = Vec::with_capacity(session.turns.len());
                        for turn in &session.turns {
                            let text = format!("{}\n{}", turn.user_message, turn.assistant_message);
                            
                            let turn_emb = if let Some(v) = cache.get(&text) {
                                c_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                v
                            } else {
                                let v = if let Some(ref embedder) = onnx_emb {
                                    onnx_invs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    embedder.get_embeddings(&text).unwrap_or_else(|e| {
                                        crate::log_warn!("[rebuild]       ONNX error on turn: {}. Falling back to hash.", e);
                                        hash_emb.get_embeddings(&text)
                                    })
                                } else {
                                    hash_emb.get_embeddings(&text)
                                };
                                cache.put(&text, v.clone());
                                v
                            };
                            turn_embs.push(turn_emb);
                        }

                        vec_index = Some(SessionVectorIndex {
                            thread_name_embedding: thread_emb,
                            turn_embeddings: turn_embs,
                        });
                    }

                    if let Some(vi) = vec_index {
                        e_map.insert(session.id.clone(), vi);
                    }
                    s_map.insert(session.id.clone(), session);

                    if idx > 0 && idx % 10 == 0 {
                        if let Some(ref cache) = cache_ref {
                            cache.save_cache();
                        }
                    }
                }

                let final_invs = onnx_invs.load(std::sync::atomic::Ordering::Relaxed);
                let final_hits = c_hits.load(std::sync::atomic::Ordering::Relaxed);

                // Prune and save cache synchronously inside the blocking task
                if run_embeddings_val {
                    let mut active_texts = std::collections::HashSet::new();
                    for session in s_map.values() {
                        let thread_name = session.thread_name.as_deref().unwrap_or("Untitled Session");
                        active_texts.insert(thread_name.to_string());
                        for turn in &session.turns {
                            let text = format!("{}\n{}", turn.user_message, turn.assistant_message);
                            active_texts.insert(text);
                        }
                    }

                    let cache_save_start = std::time::Instant::now();
                    if let Some(ref cache) = cache_ref {
                        cache.prune_orphans(&active_texts);
                        cache.save_cache();
                    }
                    crate::log_info!("[rebuild] Synchronous cache save time: {:?}", cache_save_start.elapsed());
                }

                (s_map, e_map, final_invs, final_hits)
            })
            .await
            .map_err(|e| format!("Embedding calculation task failed: {}", e))?
        };

        if run_embeddings {
            crate::log_info!("[rebuild] Embedding calculations: ONNX = {}, Cache Hits = {}, Elapsed: {:?}", final_onnx_invocations, final_cache_hits, embed_start.elapsed());
        }

        if let Ok(mut sessions_guard) = self.sessions.write() {
            *sessions_guard = session_map;
        }
        if let Ok(mut embeddings_guard) = self.embeddings.write() {
            *embeddings_guard = embedding_map;
        }

        emit_progress("complete", 1.0, "Index rebuild complete.");
        crate::log_info!("[rebuild] Total rebuild time: {:?}", total_start.elapsed());
        Ok(())
    }



    pub async fn update_session(&self, session: crate::models::Session) -> Result<(), String> {
        let model_path = downloader::get_model_file();
        let vocab_path = downloader::get_vocab_file();

        let mut onnx_embedder = if model_path.exists() && vocab_path.exists() {
            semantic::OnnxSemanticEmbedder::new(&model_path, &vocab_path).ok()
        } else {
            None
        };
        let hash_embedder = semantic::HashSemanticEmbedder::new(384);

        let model_id = if onnx_embedder.is_some() { "all-MiniLM-L6-v2" } else { "hash-384" };
        let cache_mgr = cache::EmbeddingCacheManager::new(model_id);
        cache_mgr.load_cache();

        let thread_name = session.thread_name.as_deref().unwrap_or("Untitled Session");
        let thread_emb = if let Some(v) = cache_mgr.get(thread_name) {
            v
        } else {
            let v = if let Some(ref mut onnx) = onnx_embedder {
                onnx.get_embeddings(thread_name).unwrap_or_else(|_| hash_embedder.get_embeddings(thread_name))
            } else {
                hash_embedder.get_embeddings(thread_name)
            };
            cache_mgr.put(thread_name, v.clone());
            v
        };

        let mut turn_embs = Vec::new();
        for turn in &session.turns {
            let text = format!("{}\n{}", turn.user_message, turn.assistant_message);
            let turn_emb = if let Some(v) = cache_mgr.get(&text) {
                v
            } else {
                let v = if let Some(ref mut onnx) = onnx_embedder {
                    onnx.get_embeddings(&text).unwrap_or_else(|_| hash_embedder.get_embeddings(&text))
                } else {
                    hash_embedder.get_embeddings(&text)
                };
                cache_mgr.put(&text, v.clone());
                v
            };
            turn_embs.push(turn_emb);
        }

        let vec_index = SessionVectorIndex {
            thread_name_embedding: thread_emb,
            turn_embeddings: turn_embs,
        };

        cache_mgr.save_cache();

        if let Ok(mut sessions_guard) = self.sessions.write() {
            sessions_guard.insert(session.id.clone(), session.clone());
        }
        if let Ok(mut embeddings_guard) = self.embeddings.write() {
            embeddings_guard.insert(session.id.clone(), vec_index);
        }

        Ok(())
    }
}
