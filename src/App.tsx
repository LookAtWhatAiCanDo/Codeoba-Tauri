import { createSignal, createEffect, onMount, onCleanup, Show, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import packageJson from "../package.json";
import { Sidebar } from "./components/Sidebar";
import { DetailPane } from "./components/DetailPane";
import { Dashboard } from "./components/Dashboard";
import { SettingsDialog } from "./components/SettingsDialog";
import { FileViewerDialog } from "./components/FileViewerDialog";
import { MarkdownRenderer } from "./components/MarkdownRenderer";
import { logFE } from "./utils/logger";
import { useI18n } from "./i18n/i18n";
import { 
  Layers, 
  Terminal, 
  AlertCircle,
  PanelLeftClose,
  PanelLeftOpen,
  ArrowLeft,
  ArrowRight,
  Home,
  RotateCw,
  Settings,
  X,
  Download,
  Bug
} from "lucide-solid";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

interface Turn {
  turnId: string;
  userMessage: string;
  assistantMessage: string;
  timestamp: number;
  inputTokens?: number | null;
  outputTokens?: number | null;
}

interface Session {
  id: string;
  sourceId: string;
  filePath: string;
  timestamp: number;
  updatedAt: number;
  cwd?: string | null;
  threadName?: string | null;
  turns: Turn[];
  isArchived: boolean;
  isPinned: boolean;
}

interface SearchResult {
  session: Session;
  matchedTurnIndexes: number[];
  score: number;
}

interface SourceMetadata {
  id: string;
  displayName: string;
  isAvailable: boolean;
  isAppInstalled: boolean;
}



function App() {
  const { t } = useI18n();
  const [theme, setTheme] = createSignal(localStorage.getItem("codeoba-theme") || "obsidian");
  const [sidebarWidth, setSidebarWidth] = createSignal(parseInt(localStorage.getItem("codeoba-sidebar-width") || "380"));
  const [sidebarCollapsed, setSidebarCollapsed] = createSignal(localStorage.getItem("codeoba-sidebar-collapsed") === "true");
  const [showSettings, setShowSettings] = createSignal(false);
  const [showDisclaimer, setShowDisclaimer] = createSignal(false);
  const [similarityThreshold, setSimilarityThreshold] = createSignal(
    parseFloat(localStorage.getItem("codeoba-similarity-threshold") || "0.35")
  );
  const [dateFormat, setDateFormat] = createSignal(localStorage.getItem("codeoba-date-format") || "system");
  const [numberFormat, setNumberFormat] = createSignal(localStorage.getItem("codeoba-number-format") || "system");

  const handleDateFormatChange = (val: string) => {
    setDateFormat(val);
    localStorage.setItem("codeoba-date-format", val);
  };

  const handleNumberFormatChange = (val: string) => {
    setNumberFormat(val);
    localStorage.setItem("codeoba-number-format", val);
  };

  // Auto-update states
  const [updateManifest, setUpdateManifest] = createSignal<any>(null);
  const [showUpdateModal, setShowUpdateModal] = createSignal(false);
  const [isUpdating, setIsUpdating] = createSignal(false);
  const [updateProgress, setUpdateProgress] = createSignal(0);
  const [updateError, setUpdateError] = createSignal<string | null>(null);
  const releaseNotes = createMemo(() => {
    const manifest = updateManifest();
    if (!manifest) return "";
    return manifest.body || manifest.notes || manifest.rawJson?.notes || manifest.rawJson?.body || "";
  });

  const [navHistory, setNavHistory] = createSignal<string[]>(["dashboard"]);
  const [historyIndex, setHistoryIndex] = createSignal<number>(0);

  const [sources, setSources] = createSignal<SourceMetadata[]>([]);
  const [sessions, setSessions] = createSignal<Session[]>([]);
  const [searchResults, setSearchResults] = createSignal<SearchResult[] | null>(null);
  const [selectedSession, setSelectedSession] = createSignal<Session | null>(null);
  
  const [searchQuery, setSearchQuery] = createSignal("");
  const [isSemantic, setIsSemantic] = createSignal(false);
  const [selectedSources, setSelectedSources] = createSignal<Set<string>>(new Set());
  const [archivalFilter, setArchivalFilter] = createSignal<"all" | "active" | "archived">("active");
  
  const [isLoading, setIsLoading] = createSignal(true);
  const [isRebuilding, setIsRebuilding] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);
  const [indexingProgress, setIndexingProgress] = createSignal<{
    step: string;
    progress: number;
    currentSource: string;
  } | null>(null);
  const [loadTime, setLoadTime] = createSignal<string | null>(null);
  const [loadingSessionId, setLoadingSessionId] = createSignal<string | null>(null);
  const [appVersion, setAppVersion] = createSignal(packageJson.version);

  // Sync theme selection to DOM
  createEffect(() => {
    document.documentElement.setAttribute("data-theme", theme());
    localStorage.setItem("codeoba-theme", theme());
  });

  // Sync sidebar width selection to localStorage
  createEffect(() => {
    localStorage.setItem("codeoba-sidebar-width", String(sidebarWidth()));
  });

  // Sync sidebar collapsed selection to localStorage
  createEffect(() => {
    localStorage.setItem("codeoba-sidebar-collapsed", String(sidebarCollapsed()));
  });

  // Sync similarity threshold to localStorage
  createEffect(() => {
    localStorage.setItem("codeoba-similarity-threshold", String(similarityThreshold()));
  });

  // Load backend metadata & sessions on startup, and register listeners
  onMount(async () => {
    // Set window title with app version
    try {
      const version = await getVersion();
      setAppVersion(version);
      document.title = `Codeoba v${version}`;
    } catch (err) {
      console.error("Failed to set window title:", err);
    }

    // Check disclaimer acknowledgement
    const disclaimerAck = localStorage.getItem("codeoba-disclaimer-acknowledged");
    if (!disclaimerAck) {
      setShowDisclaimer(true);
    }

    // Hide startup skeleton once UI is mounted
    const skeleton = document.getElementById("sk-container");
    if (skeleton) {
      skeleton.classList.add("sk-fade-out");
      setTimeout(() => {
        skeleton.remove();
      }, 250);
    }

    let unlistenSession: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;

    // Register progress and live listeners immediately
    try {
      unlistenSession = await listen<Session>("session-updated", (event) => {
        const updated = event.payload;
        logFE("info", `Live event update: ${updated.id}`);

        // Update sessions state list
        setSessions(prev => {
          const index = prev.findIndex(s => s.id === updated.id);
          const list = [...prev];
          if (index !== -1) {
            list[index] = updated;
          } else {
            list.unshift(updated);
          }
          list.sort((a, b) => b.updatedAt - a.updatedAt);
          return list;
        });

        // Update selected view if open
        const current = selectedSession();
        if (current && current.id === updated.id) {
          setSelectedSession(updated);
        }
      });

      unlistenProgress = await listen<{
        step: string;
        progress: number;
        currentSource: string;
      }>("indexing-progress", (event) => {
        const payload = event.payload;
        setIndexingProgress(payload);

        if (payload.step === "complete") {
          // Re-fetch sessions from backend once rebuild is complete
          invoke<Session[]>("get_all_sessions").then((list) => {
            setSessions(list);
          });
          // Hide progress indicator after a short delay
          setTimeout(() => {
            setIndexingProgress(null);
          }, 1500);
        }
      });
    } catch (err) {
      console.error("Failed to register listeners:", err);
    }

    onCleanup(() => {
      if (unlistenSession) unlistenSession();
      if (unlistenProgress) unlistenProgress();
    });

    try {
      setIsLoading(true);
      const metadata = await invoke<SourceMetadata[]>("get_sources");
      setSources(metadata);

      const list = await invoke<Session[]>("get_all_sessions");
      setSessions(list);
      
      setErrorMsg(null);

      // Get initial indexing progress state
      try {
        const initialProgress = await invoke<any>("get_indexing_progress");
        if (initialProgress) {
          setIndexingProgress(initialProgress);
          if (initialProgress.step === "complete") {
            setIsRebuilding(false);
            // Wait 4 seconds then clear
            setTimeout(() => {
              setIndexingProgress(current => {
                if (current && current.step === "complete") {
                  return null;
                }
                return current;
              });
            }, 4000);
          } else {
            setIsRebuilding(true);
          }
        }
      } catch (err) {
        console.error("Failed to fetch initial indexing progress:", err);
      }
    } catch (err: any) {
      console.error("Failed to load sessions:", err);
      setErrorMsg(String(err));
    } finally {
      setIsLoading(false);
    }

    // Trigger background rebuild on launch only if not already rebuilding
    const progress = indexingProgress();
    const isAlreadyIndexing = progress && progress.step !== "complete";
    if (!isAlreadyIndexing) {
      handleRebuildIndex();
    }

    // Background update check if enabled
    const autoUpdate = localStorage.getItem("codeoba-auto-update") !== "false";
    if (autoUpdate) {
      setTimeout(async () => {
        try {
          const updaterActive = await invoke<boolean>("is_updater_active");
          if (!updaterActive) {
            return;
          }

          const currentVersion = await getVersion();
          logFE("info", `Background Updater: Initiating background check. Current version: v${currentVersion}`);
          logFE("info", "Background Updater: Querying the update service...");
          const update = await check();
          if (update && update.available) {
            logFE("info", `Background Updater: Update check successful. Found newer version: v${update.version} (released on ${update.date || 'unknown date'})`);
            setUpdateManifest(update);
            setShowUpdateModal(true);
          } else {
            logFE("info", "Background Updater: Update check successful. The application is up to date.");
          }
        } catch (err: any) {
          logFE("error", `Background Updater: Update check failed. Error details: ${err}`);
        }
      }, 3000); // delay check slightly after startup
    }
  });

  const handleStartUpdate = async () => {
    const update = updateManifest();
    if (!update) return;

    setIsUpdating(true);
    setUpdateError(null);
    setUpdateProgress(0);

    try {
      logFE("info", `Starting download and installation for v${update.version}...`);
      
      let downloaded = 0;
      let contentLength = 0;
      
      await update.downloadAndInstall((event: any) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data?.contentLength || 0;
            logFE("info", `Download started. Size: ${contentLength}`);
            break;
          case "Progress":
            downloaded += event.data?.chunkLength || 0;
            if (contentLength > 0) {
              setUpdateProgress(Math.round((downloaded / contentLength) * 100));
            }
            break;
          case "Finished":
            logFE("info", "Download finished.");
            setUpdateProgress(100);
            break;
        }
      });

      logFE("info", "Update installation completed successfully. Relaunching...");
      await relaunch();
    } catch (err: any) {
      logFE("error", `Failed to download and install update: ${err}`);
      setUpdateError(String(err));
      setIsUpdating(false);
    }
  };

  // Handle debounced search changes
  createEffect(() => {
    const query = searchQuery();
    const sem = isSemantic();
    const sources = selectedSources();
    const filter = archivalFilter();
    const thresh = similarityThreshold();

    if (query.trim() === "") {
      setSearchResults(null);
      return;
    }

    const delayDebounce = setTimeout(() => {
      performSearch(query, sem, sources, filter, thresh);
    }, 250);

    onCleanup(() => clearTimeout(delayDebounce));
  });

  const performSearch = async (
    query: string,
    sem: boolean,
    sourcesSet: Set<string>,
    filterType: "all" | "active" | "archived",
    thresh: number
  ) => {
    try {
      setErrorMsg(null);
      const filter = {
        sourceIds: Array.from(sourcesSet),
        minTimestamp: 0,
        maxTimestamp: null,
        cwdFilter: null,
        matchCase: false,
        wholeWord: false,
        useRegex: false,
        archivalFilter: filterType,
        sessionIds: null
      };

      const results = await invoke<SearchResult[]>("search_sessions", {
        query,
        filter,
        useSemantic: sem,
        similarityThreshold: thresh
      });
      setSearchResults(results);
    } catch (err: any) {
      logFE("error", `Search error: ${err}`);
      setErrorMsg(String(err));
    }
  };

  const handleToggleSource = (sourceId: string) => {
    const next = new Set(selectedSources());
    if (next.has(sourceId)) {
      next.delete(sourceId);
    } else {
      next.add(sourceId);
    }
    setSelectedSources(next);
  };

  const handleRebuildIndex = async () => {
    try {
      setIsRebuilding(true);
      setErrorMsg(null);
      await invoke("rebuild_index");
      
      // Refresh session list
      const list = await invoke<Session[]>("get_all_sessions");
      setSessions(list);
      
      // Re-trigger search if query exists
      const query = searchQuery();
      if (query.trim() !== "") {
        performSearch(query, isSemantic(), selectedSources(), archivalFilter(), similarityThreshold());
      }
    } catch (err: any) {
      logFE("error", `Rebuild error: ${err}`);
      setErrorMsg(String(err));
    } finally {
      setIsRebuilding(false);
    }
  };

  const handleSelectSession = async (session: Session, skipHistory = false) => {
    if (!skipHistory) {
      const history = [...navHistory().slice(0, historyIndex() + 1)];
      if (history[history.length - 1] !== session.id) {
        history.push(session.id);
        setNavHistory(history);
        setHistoryIndex(history.length - 1);
      }
    }

    const start = performance.now();
    (window as any).sessionSelectionStart = start;
    logFE("info", `Selecting session: ${session.id} (${session.threadName || 'Untitled'})`);
    setLoadTime(t("common.loading"));
    setLoadingSessionId(session.id);
    try {
      const fullSession = await invoke<Session | null>("get_session", {
        sourceId: session.sourceId,
        filePath: session.filePath,
      });
      const fetchTime = performance.now() - start;
      logFE("info", `Fetched session ${session.id} turns in ${fetchTime.toFixed(1)}ms`);

      if (fullSession) {
        setSelectedSession(fullSession);
        
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            const paintTime = performance.now() - start;
            const msg = `${paintTime.toFixed(0)}ms (fetch: ${fetchTime.toFixed(0)}ms, render: ${(paintTime - fetchTime).toFixed(0)}ms)`;
            logFE("info", `Rendered and painted session ${session.id} in ${paintTime.toFixed(1)}ms total. Detail metrics: ${msg}`);
            setLoadTime(msg);
            setLoadingSessionId(null);
          });
        });
      } else {
        setLoadTime(null);
        setLoadingSessionId(null);
      }
    } catch (err: any) {
      logFE("error", `Failed to load session details: ${err}`);
      setErrorMsg(t("common.error"));
      setLoadTime(null);
      setLoadingSessionId(null);
    }
  };

  const handleGoHome = (skipHistory = false) => {
    if (!skipHistory) {
      const history = [...navHistory().slice(0, historyIndex() + 1)];
      if (history[history.length - 1] !== "dashboard") {
        history.push("dashboard");
        setNavHistory(history);
        setHistoryIndex(history.length - 1);
      }
    }
    setSelectedSession(null);
  };

  const handleNavBack = () => {
    if (historyIndex() > 0) {
      const prevIdx = historyIndex() - 1;
      setHistoryIndex(prevIdx);
      const target = navHistory()[prevIdx];
      if (target === "dashboard") {
        handleGoHome(true);
      } else {
        const found = sessions().find(s => s.id === target) || 
                      (searchResults()?.find(r => r.session.id === target)?.session);
        if (found) {
          handleSelectSession(found, true);
        } else {
          handleGoHome(true);
        }
      }
    }
  };

  const handleNavForward = () => {
    if (historyIndex() < navHistory().length - 1) {
      const nextIdx = historyIndex() + 1;
      setHistoryIndex(nextIdx);
      const target = navHistory()[nextIdx];
      if (target === "dashboard") {
        handleGoHome(true);
      } else {
        const found = sessions().find(s => s.id === target) || 
                      (searchResults()?.find(r => r.session.id === target)?.session);
        if (found) {
          handleSelectSession(found, true);
        } else {
          handleGoHome(true);
        }
      }
    }
  };

  const filteredSessions = createMemo(() => {
    if (searchResults() !== null) {
      return searchResults()!.map(r => r.session);
    }
    return sessions().filter(s => {
      // Source filter
      if (selectedSources().size > 0 && !selectedSources().has(s.sourceId)) {
        return false;
      }
      // Archival filter
      if (archivalFilter() === "active" && s.isArchived) return false;
      if (archivalFilter() === "archived" && !s.isArchived) return false;
      return true;
    });
  });

  const handleCopyPath = (path: string) => {
    navigator.clipboard.writeText(path);
  };

  const handleOpenIssues = async () => {
    try {
      await openUrl("https://github.com/LookAtWhatAiCanDo/Codeoba/issues");
    } catch (err) {
      console.error("Failed to open issues URL:", err);
    }
  };

  const handleAcknowledgeDisclaimer = () => {
    localStorage.setItem("codeoba-disclaimer-acknowledged", "true");
    setShowDisclaimer(false);
  };

  const renderNavigationPill = () => (
    <div class="flex items-center gap-1 bg-surface/60 border border-border/55 rounded-xl p-1 pointer-events-auto flex-shrink-0">
      <button
        onClick={() => setSidebarCollapsed(!sidebarCollapsed())}
        title={sidebarCollapsed() ? "Show Sidebar" : "Hide Sidebar"}
        class="p-1.5 hover:bg-surface border border-transparent hover:border-border/60 hover:text-text-primary text-text-secondary rounded-lg transition-all cursor-pointer"
      >
        <Show when={sidebarCollapsed()} fallback={<PanelLeftClose class="w-4 h-4" />}>
          <PanelLeftOpen class="w-4 h-4" />
        </Show>
      </button>

      <div class="w-[1px] h-4 bg-border/40 mx-1" />

      <button
        onClick={handleNavBack}
        disabled={historyIndex() <= 0}
        title="Go Back"
        class="p-1.5 hover:bg-surface border border-transparent hover:border-border/60 hover:text-text-primary text-text-secondary rounded-lg transition-all cursor-pointer disabled:opacity-20 disabled:pointer-events-none"
      >
        <ArrowLeft class="w-4 h-4" />
      </button>

      <button
        onClick={handleNavForward}
        disabled={historyIndex() >= navHistory().length - 1}
        title="Go Forward"
        class="p-1.5 hover:bg-surface border border-transparent hover:border-border/60 hover:text-text-primary text-text-secondary rounded-lg transition-all cursor-pointer disabled:opacity-20 disabled:pointer-events-none"
      >
        <ArrowRight class="w-4 h-4" />
      </button>

      <button
        onClick={() => handleGoHome()}
        title={t("dashboard.globalStats")}
        class={`p-1.5 hover:bg-surface border hover:border-border/60 rounded-lg transition-all cursor-pointer ${
          selectedSession() === null ? "text-accent bg-accent/10 border-accent/20" : "border-transparent text-text-secondary"
        }`}
      >
        <Home class="w-4 h-4" />
      </button>

      <button
        onClick={handleRebuildIndex}
        disabled={isRebuilding()}
        title={t("sidebar.forceRebuild")}
        class="p-1.5 hover:bg-surface border border-transparent hover:border-border/60 hover:text-text-primary text-text-secondary rounded-lg transition-all cursor-pointer disabled:opacity-50"
      >
        <RotateCw class={`w-4 h-4 ${isRebuilding() ? 'animate-spin text-accent' : ''}`} />
      </button>

      <div class="w-[1px] h-4 bg-border/40 mx-1" />

      <button
        onClick={() => setShowSettings(true)}
        title={t("settings.title")}
        class="p-1.5 hover:bg-surface border border-transparent hover:border-border/60 hover:text-text-primary text-text-secondary rounded-lg transition-all cursor-pointer"
      >
        <Settings class="w-4 h-4" />
      </button>
    </div>
  );

  return (
    <div class="flex h-screen w-screen overflow-hidden bg-background text-text-primary">
      {/* Dynamic Headers based on style selection */}
      {/* Dynamic Headers based on style selection */}
      {/* Modern Sidebar Header (Unified Layout) */}
      <div 
        class="absolute top-0 left-0 right-0 h-[var(--sk-header-height)] pointer-events-auto z-50 flex items-center justify-between select-none border-b border-border/10 glass transition-all duration-200"
        style={{
          "padding-left": "80px",
          "padding-right": "24px"
        }}
        data-tauri-drag-region
      >
        <div class="flex items-center gap-4 pointer-events-auto">
          <div class="flex items-center gap-2 w-[176px] flex-shrink-0">
            <Terminal class="w-4.5 h-4.5 text-accent animate-pulse" />
            <span class="font-bold tracking-widest text-[14px] text-text-primary leading-none">
              CODEOBA
            </span>
            <span class="text-[9px] font-mono bg-surface border border-white/10 rounded text-accent font-semibold leading-none w-[46px] h-[18px] inline-flex items-center justify-center">
              v{appVersion()}
            </span>
          </div>
          {renderNavigationPill()}
        </div>

        <div class="flex items-center gap-3 pointer-events-auto">
          <div class="hidden md:flex items-center gap-2 text-[11px] font-medium text-text-secondary bg-surface/30 px-3 py-1 rounded-full border border-border/40">
            <Show 
              when={selectedSession()} 
              fallback={
                <span class="text-accent font-semibold flex items-center gap-1">
                  <Layers class="w-3 h-3" /> {t("dashboard.globalStats")}
                </span>
              }
            >
              <span class="text-text-secondary/70 truncate max-w-[120px]" title={selectedSession()?.cwd || ""}>
                {selectedSession()?.cwd?.split(/[/\\]/).pop() || "Root"}
              </span>
              <span class="text-border">/</span>
              <span class="text-text-primary truncate max-w-[160px]" title={selectedSession()?.threadName || "Untitled"}>
                {selectedSession()?.threadName || "Untitled"}
              </span>
            </Show>
          </div>
          <button
            onClick={handleOpenIssues}
            title={t("disclaimer.bugTracker")}
            class="p-1.5 bg-surface/40 hover:bg-surface border border-border/60 hover:border-accent/40 rounded-xl text-text-secondary hover:text-accent transition-all cursor-pointer flex items-center justify-center"
          >
            <Bug class="w-4 h-4 text-accent" />
          </button>
        </div>
      </div>

      {/* Main Grid: Sidebar + Detail Pane */}
      <div 
        class="flex w-full h-full min-h-0 min-w-0"
        style={{
          "padding-top": "var(--sk-header-height, 48px)"
        }}
      >
        <Sidebar
          sessions={sessions()}
          searchResults={searchResults()}
          selectedSessionId={selectedSession()?.id || null}
          loadingSessionId={loadingSessionId()}
          onSelectSession={handleSelectSession}
          searchQuery={searchQuery()}
          onSearchChange={setSearchQuery}
          isSemantic={isSemantic()}
          onSemanticToggle={() => setIsSemantic(!isSemantic())}
          selectedSources={selectedSources()}
          onToggleSource={handleToggleSource}
          archivalFilter={archivalFilter()}
          onArchivalFilterChange={setArchivalFilter}
          sources={sources()}
          isRebuilding={isRebuilding()}
          onRebuildIndex={handleRebuildIndex}
          indexingProgress={indexingProgress()}
          width={sidebarWidth()}
          onWidthChange={setSidebarWidth}
          collapsed={sidebarCollapsed()}
          appVersion={appVersion()}
          dateFormat={dateFormat()}
          numberFormat={numberFormat()}
        />

        <div class="flex-grow h-full flex flex-col min-w-0 overflow-hidden">
          {/* Main Error Alert Bar */}
          <Show when={errorMsg()}>
            <div class="bg-red-500/10 border-b border-red-500/20 px-6 py-2.5 flex items-center gap-2 text-xs text-red-400 flex-shrink-0 animate-in fade-in slide-in-from-top-1 duration-150">
              <AlertCircle class="w-4 h-4 flex-shrink-0" />
              <span class="truncate">{errorMsg()}</span>
              <button 
                onClick={() => setErrorMsg(null)}
                class="ml-auto hover:text-white font-medium cursor-pointer"
              >
                {t("common.close")}
              </button>
            </div>
          </Show>

          <Show 
            when={!isLoading()} 
            fallback={
              <div class="flex-grow flex flex-col items-center justify-center text-text-secondary select-none animate-pulse">
                <Layers class="w-12 h-12 text-border animate-bounce mb-3" />
                <p class="text-sm font-medium tracking-wider">{t("dashboard.scanning")}</p>
              </div>
            }
          >
            <Show when={selectedSession()} fallback={<Dashboard sessions={filteredSessions()} numberFormat={numberFormat()} />}>
              <DetailPane
                session={selectedSession()}
                onCopyPath={handleCopyPath}
                loadTime={loadTime()}
                isLoading={loadingSessionId() !== null}
                sidebarCollapsed={sidebarCollapsed()}
                searchQuery={searchQuery()}
                dateFormat={dateFormat()}
                numberFormat={numberFormat()}
              />
            </Show>
          </Show>
        </div>
      </div>

      <SettingsDialog
        isOpen={showSettings()}
        onClose={() => setShowSettings(false)}
        theme={theme()}
        onThemeChange={setTheme}
        sources={sources()}
        onRefreshSources={async () => {
          const metadata = await invoke<SourceMetadata[]>("get_sources");
          setSources(metadata);
        }}
        similarityThreshold={similarityThreshold()}
        onSimilarityThresholdChange={setSimilarityThreshold}
        dateFormat={dateFormat()}
        onDateFormatChange={handleDateFormatChange}
        numberFormat={numberFormat()}
        onNumberFormatChange={handleNumberFormatChange}
        onUpdateAvailable={(update) => {
          setUpdateManifest(update);
          setShowUpdateModal(true);
          setShowSettings(false);
        }}
      />
      <FileViewerDialog sessionCwd={selectedSession()?.cwd} />

      {/* Update Modal Overlay */}
      <Show when={showUpdateModal() && updateManifest()}>
        <div class="fixed inset-0 bg-black/75 z-[60] flex items-center justify-center animate-in fade-in duration-200 backdrop-blur-md">
          <div class="w-[460px] bg-surface border border-border/80 p-6 rounded-2xl flex flex-col gap-5 shadow-2xl relative animate-in zoom-in-95 duration-200">
            
            {/* Close button - only show if NOT currently installing an update */}
            <Show when={!isUpdating()}>
              <button 
                onClick={() => setShowUpdateModal(false)}
                class="absolute top-4 right-4 p-1.5 bg-background hover:bg-surface border border-border/60 rounded-xl text-text-secondary hover:text-text-primary transition-all cursor-pointer"
              >
                <X class="w-4 h-4" />
              </button>
            </Show>

            {/* Header info */}
            <div class="flex items-center gap-3">
              <div class="p-2.5 bg-accent/10 border border-accent/20 text-accent rounded-xl">
                <RotateCw class={`w-5 h-5 ${isUpdating() ? 'animate-spin' : ''}`} />
              </div>
              <div>
                <h3 class="text-sm font-bold text-text-primary uppercase tracking-wider">
                  {t("updater.title")}
                </h3>
                <p class="text-[10px] text-text-secondary/70">{t("updater.description", { version: updateManifest().version })}</p>
              </div>
            </div>

            {/* Version Details */}
            <div class="bg-background/50 border border-border/40 rounded-xl p-4 space-y-2 text-xs">
              <div class="flex items-center justify-between font-semibold">
                <span class="text-text-secondary">Version:</span>
                <span class="text-accent bg-accent/10 border border-accent/20 px-2 py-0.5 rounded-full text-[10px]">
                  v{updateManifest().version}
                </span>
              </div>
              
              <Show when={releaseNotes()}>
                <div class="border-t border-border/30 pt-3 space-y-2">
                  <span class="text-text-secondary font-semibold">Release Notes:</span>
                  <div class="max-h-48 overflow-y-auto bg-background/30 p-3 rounded-xl border border-border/20 text-left update-notes-container">
                    <MarkdownRenderer content={releaseNotes()} />
                  </div>
                </div>
              </Show>
            </div>

            {/* Status & Progress Bar */}
            <Show when={isUpdating()}>
              <div class="space-y-2">
                <div class="flex justify-between text-[10px] font-semibold text-text-secondary">
                  <span>{t("updater.downloading", { progress: updateProgress() })}</span>
                  <span class="text-accent">{updateProgress()}%</span>
                </div>
                <div class="w-full h-1.5 bg-background rounded-full overflow-hidden border border-border/40">
                  <div 
                    class="h-full bg-accent transition-all duration-300 rounded-full"
                    style={{ width: `${updateProgress()}%` }}
                  />
                </div>
              </div>
            </Show>

            {/* Error Message */}
            <Show when={updateError()}>
              <div class="bg-red-500/10 border border-red-500/20 px-4 py-2.5 rounded-xl flex items-center gap-2 text-[10px] text-red-400">
                <AlertCircle class="w-4 h-4 flex-shrink-0" />
                <span class="truncate flex-1">{t("updater.failed", { error: updateError() || "" })}</span>
              </div>
            </Show>

            {/* Actions */}
            <div class="flex gap-3 w-full pt-1">
              <Show when={!isUpdating()}>
                <button
                  onClick={() => setShowUpdateModal(false)}
                  class="flex-1 py-2 border border-border bg-background hover:bg-surface rounded-xl text-xs font-semibold text-text-secondary hover:text-text-primary transition-all cursor-pointer"
                >
                  {t("updater.later")}
                </button>
                <button
                  onClick={handleStartUpdate}
                  class="flex-1 py-2 bg-accent hover:bg-accent/90 border border-accent/20 rounded-xl text-xs font-semibold text-background hover:text-background transition-all cursor-pointer shadow-md flex items-center justify-center gap-1.5"
                >
                  <Download class="w-3.5 h-3.5" />
                  <span>{t("updater.updateBtn")}</span>
                </button>
              </Show>
            </div>
          </div>
        </div>
      </Show>
      {/* Disclaimer Modal Overlay */}
      <Show when={showDisclaimer()}>
        <div class="fixed inset-0 bg-black/75 z-[70] flex items-center justify-center animate-in fade-in duration-200 backdrop-blur-md">
          <div class="w-[480px] bg-surface border border-border/80 p-6 rounded-2xl flex flex-col gap-5 shadow-2xl relative animate-in zoom-in-95 duration-200">
            
            {/* Header info */}
            <div class="flex items-center gap-3">
              <div class="p-2.5 bg-yellow-500/10 border border-yellow-500/20 text-yellow-500 rounded-xl">
                <AlertCircle class="w-5 h-5" />
              </div>
              <div>
                <h3 class="text-sm font-bold text-text-primary uppercase tracking-wider">
                  {t("disclaimer.title")}
                </h3>
                <span class="text-[9px] font-mono bg-yellow-500/10 border border-yellow-500/20 rounded text-yellow-500 px-1.5 py-0.5 font-semibold">
                  ALPHA / BETA
                </span>
              </div>
            </div>

            {/* Description Details */}
            <div class="bg-background/50 border border-border/40 rounded-xl p-4 space-y-3 text-xs leading-relaxed text-text-secondary">
              <p>
                {t("disclaimer.message1")}
              </p>
              <p>
                {t("disclaimer.message2")}
              </p>
              <div class="border-t border-border/30 pt-3 space-y-2">
                <p class="text-text-primary/95 font-semibold font-medium">
                  {t("disclaimer.feedbackPrompt")}
                </p>
                <button
                  onClick={handleOpenIssues}
                  class="flex items-center gap-2 px-3 py-2 bg-background hover:bg-surface border border-border/60 hover:border-accent/40 rounded-xl text-xs font-semibold text-text-secondary hover:text-accent transition-all cursor-pointer w-full justify-center"
                >
                  <Bug class="w-4 h-4 text-accent" />
                  <span>{t("disclaimer.githubIssues")}</span>
                </button>
              </div>
            </div>

            {/* Actions */}
            <div class="flex gap-3 w-full pt-1">
              <button
                onClick={handleAcknowledgeDisclaimer}
                class="flex-1 py-2.5 bg-accent hover:bg-accent/90 border border-accent/20 rounded-xl text-xs font-semibold text-background hover:text-background transition-all cursor-pointer shadow-md flex items-center justify-center gap-1.5"
              >
                <span>{t("disclaimer.button")}</span>
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default App;
