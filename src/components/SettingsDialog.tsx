import { createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { 
  X, 
  Trash2, 
  AlertTriangle, 
  RefreshCw,
  Palette,
  Shield,
  Layers,
  Sliders,
  Settings
} from "lucide-solid";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";
import { logFE } from "../utils/logger";
import { useI18n, LOCALES, LOCALE_NAMES, Locale } from "../i18n/i18n";

interface SourceMetadata {
  id: string;
  displayName: string;
  isAvailable: boolean;
  isAppInstalled: boolean;
}

interface SettingsDialogProps {
  isOpen: boolean;
  onClose: () => void;
  theme: string;
  onThemeChange: (theme: string) => void;
  sources: SourceMetadata[];
  onRefreshSources: () => void;
  similarityThreshold?: number;
  onSimilarityThresholdChange?: (val: number) => void;
  onUpdateAvailable?: (update: any) => void;
}

type Category = "general" | "sources" | "semantic" | "permissions";

const THEMES = [
  { id: "obsidian", name: "Obsidian", color: "bg-[#0d0e12] border-slate-700" },
  { id: "nordic-frost", name: "Nordic Frost", color: "bg-[#0b1116] border-sky-950" },
  { id: "emerald-forest", name: "Emerald Forest", color: "bg-[#09110f] border-emerald-950" },
  { id: "sunset-copper", name: "Sunset Copper", color: "bg-[#130f0d] border-amber-950" },
  { id: "royal-amethyst", name: "Royal Amethyst", color: "bg-[#100d18] border-purple-950" },
  { id: "dracula", name: "Dracula", color: "bg-[#1e1e2e] border-pink-950" },
  { id: "cyberpunk-neon", name: "Cyberpunk", color: "bg-[#080710] border-pink-700" },
  { id: "monochrome-slate", name: "Monochrome", color: "bg-[#0f172a] border-slate-700" }
];

export const SettingsDialog = (props: SettingsDialogProps) => {
  const { locale, setLocale, t } = useI18n();
  const [activeCategory, setActiveCategory] = createSignal<Category>("general");
  const [deletingSourceId, setDeletingSourceId] = createSignal<string | null>(null);
  const [checkingUpdates, setCheckingUpdates] = createSignal(false);
  const [updateCheckResult, setUpdateCheckResult] = createSignal<string | null>(null);
  const [updaterActive, setUpdaterActive] = createSignal(false);
  const [appVersion, setAppVersion] = createSignal("0.1.0");

  // Semantic Settings
  const [modelDownloaded, setModelDownloaded] = createSignal(false);
  const [downloading, setDownloading] = createSignal(false);
  const [downloadProgress, setDownloadProgress] = createSignal<number | null>(null);
  const [downloadError, setDownloadError] = createSignal<string | null>(null);

  const [localThreshold, setLocalThreshold] = createSignal(
    parseFloat(localStorage.getItem("codeoba-similarity-threshold") || "0.35")
  );
  const similarityThreshold = () => props.similarityThreshold !== undefined ? props.similarityThreshold : localThreshold();
  const setSimilarityThreshold = (val: number) => {
    if (props.onSimilarityThresholdChange) {
      props.onSimilarityThresholdChange(val);
    } else {
      setLocalThreshold(val);
      localStorage.setItem("codeoba-similarity-threshold", String(val));
    }
  };

  const handleDownloadModel = async () => {
    setDownloading(true);
    setDownloadError(null);
    setDownloadProgress(0);
    try {
      logFE("info", "Starting semantic search model download");
      await invoke("download_semantic_model");
      setModelDownloaded(true);
    } catch (err: any) {
      logFE("error", `Semantic search model download failed: ${err}`);
      setDownloadError(err.toString());
      setDownloading(false);
    }
  };

  const handleDeleteModel = async () => {
    try {
      await invoke("delete_semantic_model");
      setModelDownloaded(false);
      logFE("info", "Deleted semantic search model files.");
    } catch (err) {
      logFE("error", `Failed to delete model files: ${err}`);
    }
  };

  onMount(async () => {
    try {
      const active = await invoke<boolean>("is_updater_active");
      setUpdaterActive(active);
      const v = await getVersion();
      setAppVersion(v);

      // Check model status
      const status = await invoke<boolean>("get_semantic_model_status");
      setModelDownloaded(status);

      // Load path permissions
      await refreshPermissions();
    } catch (err) {
      logFE("error", `Failed to query startup settings state: ${err}`);
    }

    // Listen for model download progress
    const unlistenPromise = listen<number>("semantic-model-download-progress", (event) => {
      setDownloadProgress(event.payload);
      if (event.payload >= 1.0) {
        setDownloading(false);
        setDownloadProgress(null);
        setModelDownloaded(true);
      }
    });

    onCleanup(async () => {
      const unlisten = await unlistenPromise;
      unlisten();
    });
  });

  // General Settings
  const [cacheEnabled, setCacheEnabled] = createSignal(
    localStorage.getItem("codeoba-cache-enabled") !== "false"
  );
  const [autoUpdateEnabled, setAutoUpdateEnabled] = createSignal(
    localStorage.getItem("codeoba-auto-update") !== "false"
  );
  const [parserMode, setParserMode] = createSignal(
    localStorage.getItem("codeoba-parser-mode") || "standard"
  );

  // Path Permissions
  const [permissions, setPermissions] = createSignal<Array<{ path: string; preview: string; external: string }>>([]);

  const refreshPermissions = async () => {
    try {
      const backendPermissions = await invoke<Array<{
        canonical_path: string;
        action: string;
        decision: string;
        timestamp: number;
      }>>("get_all_permissions");

      const grouped: Record<string, { preview: string; external: string }> = {};
      backendPermissions.forEach(entry => {
        if (!grouped[entry.canonical_path]) {
          grouped[entry.canonical_path] = { preview: "ask", external: "ask" };
        }
        if (entry.action === "preview") {
          grouped[entry.canonical_path].preview = entry.decision;
        } else if (entry.action === "external_open") {
          grouped[entry.canonical_path].external = entry.decision;
        }
      });

      const list = Object.entries(grouped).map(([path, val]) => ({
        path,
        preview: val.preview,
        external: val.external,
      }));

      setPermissions(list);
    } catch (err) {
      logFE("error", `Failed to load path permissions: ${err}`);
    }
  };

  // Source decisions (mock list stored in localStorage)
  const [sourceDecisions, setSourceDecisions] = createSignal<Record<string, "allow" | "deny" | "ask">>(
    JSON.parse(localStorage.getItem("codeoba-source-decisions") || "{}")
  );

  const handleToggleCache = (val: boolean) => {
    setCacheEnabled(val);
    localStorage.setItem("codeoba-cache-enabled", String(val));
    logFE("info", `Persistent cache set to: ${val}`);
  };

  const handleToggleAutoUpdate = (val: boolean) => {
    setAutoUpdateEnabled(val);
    localStorage.setItem("codeoba-auto-update", String(val));
    logFE("info", `Auto-updates set to: ${val}`);
  };

  const handleParserModeChange = (mode: string) => {
    setParserMode(mode);
    localStorage.setItem("codeoba-parser-mode", mode);
    logFE("info", `Preferred parser mode set to: ${mode}`);
  };

  const handleThresholdChange = (val: number) => {
    setSimilarityThreshold(val);
  };

  const handleRestoreThresholdDefault = () => {
    handleThresholdChange(0.35);
  };

  const handleCheckUpdates = async () => {
    setCheckingUpdates(true);
    setUpdateCheckResult(null);
    try {
      logFE("info", `Settings: Initiating check for updates. Current version: v${appVersion()}`);
      logFE("info", "Settings: Querying the update service...");
      const update = await check();
      setCheckingUpdates(false);
      if (update && update.available) {
        logFE("info", `Settings: Update check successful. Found newer version: v${update.version} (released on ${update.date || 'unknown date'})`);
        setUpdateCheckResult(`Update found: v${update.version}`);
        if (props.onUpdateAvailable) {
          props.onUpdateAvailable(update);
        }
      } else {
        logFE("info", "Settings: Update check successful. The application is up to date.");
        setUpdateCheckResult("Codeoba is up to date!");
      }
    } catch (err: any) {
      logFE("error", `Settings: Update check failed. Error details: ${err}`);
      setCheckingUpdates(false);
      setUpdateCheckResult(`Error checking updates: ${err}`);
      
      // Attempt diagnostic connection to extract actual HTTP response status and body
      try {
        logFE("info", "Settings: Attempting diagnostic connection to find root cause...");
        const endpoints = await invoke<string[]>("get_resolved_updater_endpoints");
        if (endpoints && endpoints.length > 0) {
          logFE("info", `Settings: Diagnostic fetch hitting resolved endpoint: ${endpoints[0]}`);
          const diagResponse = await fetch(endpoints[0], {
            method: "GET",
            signal: AbortSignal.timeout(5000)
          });
          if (!diagResponse.ok) {
            const bodyText = await diagResponse.text();
            logFE("error", `Settings: Diagnostic fetch returned HTTP ${diagResponse.status}: ${bodyText}`);
            setUpdateCheckResult(`Error checking updates: ${bodyText}`);
          } else {
            logFE("info", "Settings: Diagnostic fetch succeeded. Update manifest exists but is likely not compatible.");
          }
        }
      } catch (diagErr: any) {
        logFE("error", `Settings: Diagnostic connection failed: ${diagErr.message || diagErr}`);
      }
    }
  };

  const handleToggleSourceDecision = (sourceId: string, decision: "allow" | "deny" | "ask") => {
    const next = { ...sourceDecisions(), [sourceId]: decision };
    setSourceDecisions(next);
    localStorage.setItem("codeoba-source-decisions", JSON.stringify(next));
    logFE("info", `Source decision for ${sourceId} set to: ${decision}`);
  };

  const handleDeleteSourceData = async (sourceId: string) => {
    try {
      logFE("info", `Deleting database and session data for source: ${sourceId}`);
      const success = await invoke<boolean>("delete_source_data", { sourceId });
      if (success) {
        logFE("info", `Successfully deleted data paths for source: ${sourceId}`);
        setDeletingSourceId(null);
        props.onRefreshSources();
      } else {
        logFE("error", `Failed to delete data paths for source: ${sourceId}`);
      }
    } catch (err: any) {
      logFE("error", `Error deleting data paths: ${err}`);
    }
  };

  const handleResetPermission = async (path: string, type: "preview" | "external" | "all") => {
    try {
      if (type === "all") {
        await invoke("delete_permission", { canonicalPath: path });
      } else {
        const action = type === "preview" ? "preview" : "external_open";
        await invoke("delete_permission", { canonicalPath: path, action });
      }
      await refreshPermissions();
    } catch (err) {
      logFE("error", `Failed to reset permission: ${err}`);
    }
  };

  const handleClearAllPermissions = async () => {
    try {
      await invoke("clear_all_permissions");
      await refreshPermissions();
    } catch (err) {
      logFE("error", `Failed to clear permissions: ${err}`);
    }
  };

  const getSourceDecision = (sourceId: string) => {
    return sourceDecisions()[sourceId] || "allow";
  };

  return (
    <Show when={props.isOpen}>
      {/* Modal scrim background */}
      <div 
        class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center animate-in fade-in duration-200 backdrop-blur-sm"
        onClick={props.onClose}
      >
        {/* Settings Dialog box */}
        <div 
          class="w-[760px] h-[520px] bg-surface border border-border/80 rounded-2xl flex overflow-hidden shadow-2xl relative animate-in zoom-in-95 duration-200"
          onClick={(e) => e.stopPropagation()} // Consume clicks
        >
          {/* Close button in top-right */}
          <button 
            onClick={props.onClose}
            class="absolute top-4 right-4 p-1.5 bg-background hover:bg-surface border border-border/60 rounded-xl text-text-secondary hover:text-text-primary transition-all cursor-pointer"
          >
            <X class="w-4 h-4" />
          </button>

          {/* Left Sidebar categories list */}
          <div class="w-[200px] border-r border-border/60 flex flex-col p-4 pt-6 gap-6 flex-shrink-0">
            <div class="flex items-center gap-2 px-2">
              <Settings class="w-4 h-4 text-accent" />
              <span class="font-bold text-text-primary tracking-wide">{t("settings.title")}</span>
            </div>
            
            <div class="flex flex-col gap-1">
              <button
                onClick={() => setActiveCategory("general")}
                class={`flex items-center gap-2.5 px-3 py-2 text-xs font-semibold rounded-xl transition-all cursor-pointer text-left ${
                  activeCategory() === "general"
                    ? "bg-accent-light/20 text-accent border border-accent/20"
                    : "text-text-secondary hover:text-text-primary border border-transparent"
                }`}
              >
                <Palette class="w-3.5 h-3.5" />
                <span>{t("settings.general.title")}</span>
              </button>
              <button
                onClick={() => setActiveCategory("sources")}
                class={`flex items-center gap-2.5 px-3 py-2 text-xs font-semibold rounded-xl transition-all cursor-pointer text-left ${
                  activeCategory() === "sources"
                    ? "bg-accent-light/20 text-accent border border-accent/20"
                    : "text-text-secondary hover:text-text-primary border border-transparent"
                }`}
              >
                <Layers class="w-3.5 h-3.5" />
                <span>{t("settings.sources.tab")}</span>
              </button>
              <button
                onClick={() => setActiveCategory("semantic")}
                class={`flex items-center gap-2.5 px-3 py-2 text-xs font-semibold rounded-xl transition-all cursor-pointer text-left ${
                  activeCategory() === "semantic"
                    ? "bg-accent-light/20 text-accent border border-accent/20"
                    : "text-text-secondary hover:text-text-primary border border-transparent"
                }`}
              >
                <Sliders class="w-3.5 h-3.5" />
                <span>{t("settings.semantic.title")}</span>
              </button>
              <button
                onClick={() => setActiveCategory("permissions")}
                class={`flex items-center gap-2.5 px-3 py-2 text-xs font-semibold rounded-xl transition-all cursor-pointer text-left ${
                  activeCategory() === "permissions"
                    ? "bg-accent-light/20 text-accent border border-accent/20"
                    : "text-text-secondary hover:text-text-primary border border-transparent"
                }`}
              >
                <Shield class="w-3.5 h-3.5" />
                <span>{t("permissions.title")}</span>
              </button>
            </div>

            {/* Version Display */}
            <div class="mt-auto px-3 py-2 bg-background/50 border border-border/40 rounded-xl flex items-center justify-between text-[10px] text-text-secondary font-medium">
              <span>Codeoba</span>
              <span class="font-mono bg-surface border border-border/60 px-1.5 py-0.5 rounded text-accent">v{appVersion()}</span>
            </div>
          </div>

          {/* Right Pane Content Area */}
          <div class="flex-grow h-full flex flex-col p-6 pt-8 overflow-y-auto min-w-0">
            <Show when={activeCategory() === "general"}>
              {/* General Settings Tab */}
              <div class="space-y-5">
                <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary mb-2">
                  {t("settings.general.title")}
                </h3>

                {/* Language Selector */}
                <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 flex items-center justify-between">
                  <div>
                    <h4 class="text-xs font-bold text-text-primary">Language / Idioma</h4>
                    <p class="text-[10px] text-text-secondary/70">Select the display language.</p>
                  </div>
                  <select
                    value={locale()}
                    onChange={(e) => setLocale(e.currentTarget.value as Locale)}
                    class="bg-background border border-border/80 rounded-xl px-3 py-1.5 text-xs text-text-primary focus:outline-none focus:border-accent font-medium cursor-pointer"
                  >
                    <For each={LOCALES}>
                      {(lang) => (
                        <option value={lang}>
                          {LOCALE_NAMES[lang]}
                        </option>
                      )}
                    </For>
                  </select>
                </div>

                {/* Theme Selector Dot Bar */}
                <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 space-y-2">
                  <div class="flex items-center justify-between">
                    <div>
                      <h4 class="text-xs font-bold text-text-primary">{t("settings.general.theme")}</h4>
                      <p class="text-[10px] text-text-secondary/70">{t("settings.general.themeDesc")}</p>
                    </div>
                    <span class="text-xs font-semibold text-accent capitalize">
                      {THEMES.find(t => t.id === props.theme)?.name || props.theme}
                    </span>
                  </div>
                  <div class="flex items-center gap-2 pt-1 flex-wrap">
                    <For each={THEMES}>
                      {(t) => (
                        <button
                          onClick={() => props.onThemeChange(t.id)}
                          title={`Switch to ${t.name}`}
                          class={`w-5 h-5 rounded-full border cursor-pointer hover:scale-110 hover:shadow-md transition-all duration-150 ${t.color} ${
                            props.theme === t.id ? "scale-105 ring-2 ring-accent ring-offset-2 ring-offset-background" : ""
                          }`}
                        />
                      )}
                    </For>
                  </div>
                </div>

                {/* Persistent cache switch */}
                <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 flex items-center justify-between">
                  <div>
                    <h4 class="text-xs font-bold text-text-primary">{t("settings.general.cache")}</h4>
                    <p class="text-[10px] text-text-secondary/70">{t("settings.general.clearCacheDesc")}</p>
                  </div>
                  <label class="relative inline-flex items-center cursor-pointer">
                    <input 
                      type="checkbox" 
                      checked={cacheEnabled()} 
                      onChange={(e) => handleToggleCache(e.currentTarget.checked)}
                      class="sr-only peer"
                    />
                    <div class="w-9 h-5 bg-background peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-text-secondary after:border-border after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-accent peer-checked:after:bg-background"></div>
                  </label>
                </div>

                {/* Auto Update Check */}
                <Show when={updaterActive()}>
                  <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 space-y-3">
                    <div class="flex items-center justify-between">
                      <div>
                        <h4 class="text-xs font-bold text-text-primary">Auto-Updates</h4>
                        <p class="text-[10px] text-text-secondary/70">Automatically check for new versions on startup.</p>
                      </div>
                      <label class="relative inline-flex items-center cursor-pointer">
                        <input 
                          type="checkbox" 
                          checked={autoUpdateEnabled()} 
                          onChange={(e) => handleToggleAutoUpdate(e.currentTarget.checked)}
                          class="sr-only peer"
                        />
                        <div class="w-9 h-5 bg-background peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-text-secondary after:border-border after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-accent peer-checked:after:bg-background"></div>
                      </label>
                    </div>
                    <div class="flex items-center justify-between pt-1 text-[11px] border-t border-border/30">
                      <span class="text-text-secondary">{t("updater.current")}: v{appVersion()}</span>
                      <button
                        onClick={handleCheckUpdates}
                        disabled={checkingUpdates()}
                        class="px-3 py-1.5 bg-background hover:bg-surface border border-border rounded-xl text-accent hover:text-accent-hover transition-all text-xs font-semibold cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
                      >
                        <Show when={checkingUpdates()} fallback={<span>{t("settings.general.update") || "Check for Updates"}</span>}>
                          <RefreshCw class="w-3.5 h-3.5 animate-spin" />
                          <span>{t("settings.general.checking")}</span>
                        </Show>
                      </button>
                    </div>
                    <Show when={updateCheckResult()}>
                      <div 
                        class="text-[11px] font-semibold"
                        classList={{
                          "text-red-400": updateCheckResult()?.startsWith("Error") || updateCheckResult()?.startsWith("Failed"),
                          "text-emerald-400": !(updateCheckResult()?.startsWith("Error") || updateCheckResult()?.startsWith("Failed"))
                        }}
                      >
                        {updateCheckResult()}
                      </div>
                    </Show>
                  </div>
                </Show>

                {/* Log Parsing Mode */}
                <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 space-y-3">
                  <div>
                    <h4 class="text-xs font-bold text-text-primary">{t("settings.general.logMode")}</h4>
                    <p class="text-[10px] text-text-secondary/70">{t("settings.general.logModeDesc")}</p>
                  </div>
                  <div class="flex bg-background p-1 rounded-lg border border-border/60">
                    <button
                      onClick={() => handleParserModeChange("standard")}
                      class={`flex-1 text-center py-1.5 text-xs font-semibold rounded-md transition-all cursor-pointer ${
                        parserMode() === "standard" 
                          ? "bg-surface text-accent border border-border/80 shadow-sm" 
                          : "text-text-secondary hover:text-text-primary"
                      }`}
                    >
                      {t("settings.general.modeStandard")}
                    </button>
                    <button
                      onClick={() => handleParserModeChange("summarizing")}
                      class={`flex-1 text-center py-1.5 text-xs font-semibold rounded-md transition-all cursor-pointer ${
                        parserMode() === "summarizing" 
                          ? "bg-surface text-accent border border-border/80 shadow-sm" 
                          : "text-text-secondary hover:text-text-primary"
                      }`}
                    >
                      {t("settings.general.modeCompact")}
                    </button>
                  </div>
                </div>
              </div>
            </Show>

            <Show when={activeCategory() === "sources"}>
              {/* Sources & Adapters Tab */}
              <div class="space-y-4">
                <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary mb-2">
                  {t("settings.sources.title")}
                </h3>

                <div class="space-y-3">
                  <For each={props.sources}>
                    {(src) => {
                      const dec = getSourceDecision(src.id);
                      return (
                        <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 flex items-center justify-between gap-4">
                          <div class="min-w-0">
                            <h4 class="text-xs font-bold text-text-primary capitalize">{src.displayName}</h4>
                            <p class="text-[10px] text-text-secondary/70 truncate">
                              {t("settings.sources.status")}: {src.isAvailable ? t("settings.sources.available") : t("settings.sources.notInstalled")}
                            </p>
                          </div>

                          <div class="flex items-center gap-2">
                            {/* Segmented controls for allow/deny/ask */}
                            <div class="flex bg-background p-0.5 rounded-lg border border-border/50 text-[10px] font-semibold text-text-primary">
                              <For each={["allow", "deny", "ask"] as const}>
                                {(option) => (
                                  <button
                                    onClick={() => handleToggleSourceDecision(src.id, option)}
                                    class={`px-2 py-1 rounded transition-all capitalize cursor-pointer ${
                                      dec === option
                                        ? "bg-surface text-accent font-bold"
                                        : "text-text-secondary hover:text-text-primary"
                                    }`}
                                  >
                                    {option}
                                  </button>
                                )}
                              </For>
                            </div>

                            {/* Trash button to delete source cache */}
                            <button
                              onClick={() => setDeletingSourceId(src.id)}
                              title={t("settings.sources.deleteData")}
                              class="p-2 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 rounded-xl text-text-secondary hover:text-red-400 transition-all cursor-pointer"
                            >
                              <Trash2 class="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </div>
            </Show>

            <Show when={activeCategory() === "semantic"}>
              {/* Semantic Settings Tab */}
              <div class="space-y-5">
                <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary mb-2">
                  {t("settings.semantic.title")}
                </h3>

                <div class="bg-surface/30 border border-border/50 rounded-2xl p-5 space-y-4">
                  <div class="space-y-1">
                    <h4 class="text-xs font-bold text-text-primary">{t("settings.semantic.downloadTitle")}</h4>
                    <p class="text-[10px] text-text-secondary/70">
                      {t("settings.semantic.downloadDesc")}
                    </p>
                  </div>

                  <Show 
                    when={modelDownloaded()} 
                    fallback={
                      <div class="space-y-3">
                        <div class="flex items-center justify-between text-xs p-3 bg-yellow-500/10 border border-yellow-500/20 rounded-xl text-yellow-400">
                          <span>{t("settings.semantic.statusNotDownloaded")}</span>
                          <button
                            onClick={handleDownloadModel}
                            disabled={downloading()}
                            class="px-3 py-1.5 bg-yellow-500 hover:bg-yellow-600 disabled:bg-yellow-500/40 text-black font-semibold rounded-xl text-xs transition-all cursor-pointer"
                          >
                            {downloading() ? t("settings.general.checking") : t("settings.semantic.downloadBtn")}
                          </button>
                        </div>
                        <Show when={downloadProgress() !== null}>
                          <div class="space-y-1">
                            <div class="flex justify-between text-[10px] font-bold text-text-secondary">
                              <span>{t("settings.semantic.downloading")}</span>
                              <span>{Math.round(downloadProgress()! * 100)}%</span>
                            </div>
                            <div class="w-full h-1.5 bg-background rounded-full overflow-hidden border border-border/50">
                              <div 
                                class="h-full bg-accent transition-all duration-100" 
                                style={{ width: `${downloadProgress()! * 100}%` }}
                              />
                            </div>
                          </div>
                        </Show>
                        <Show when={downloadError()}>
                          <div class="text-xs text-red-400 bg-red-500/10 border border-red-500/20 rounded-xl p-3">
                            {downloadError()}
                          </div>
                        </Show>
                      </div>
                    }
                  >
                    <div class="flex items-center justify-between text-xs p-3 bg-emerald-500/10 border border-emerald-500/20 rounded-xl text-emerald-400">
                      <span>{t("settings.semantic.statusInstalled")}</span>
                      <button
                        onClick={handleDeleteModel}
                        class="px-2.5 py-1 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 text-text-secondary hover:text-red-400 rounded-lg text-[10.5px] font-semibold transition-all cursor-pointer"
                      >
                        {t("settings.semantic.deleteModel")}
                      </button>
                    </div>
                  </Show>
                </div>

                <Show when={modelDownloaded()}>
                  <div class="bg-surface/30 border border-border/50 rounded-2xl p-5 space-y-4">
                    <div class="space-y-1">
                      <h4 class="text-xs font-bold text-text-primary">{t("settings.semantic.threshold")}</h4>
                      <p class="text-[10px] text-text-secondary/70">
                        {t("settings.semantic.thresholdDesc")}
                      </p>
                    </div>

                    <div class="flex items-center gap-4">
                      <input 
                        type="range"
                        min="0.0"
                        max="1.0"
                        step="0.05"
                        value={similarityThreshold()}
                        onInput={(e) => handleThresholdChange(parseFloat(e.currentTarget.value))}
                        class="flex-grow accent-accent h-1.5 bg-background rounded-lg appearance-none cursor-pointer"
                      />
                      <div class="w-12 py-1 bg-background border border-border rounded-lg text-center text-xs font-bold text-text-primary">
                        {similarityThreshold().toFixed(2)}
                      </div>
                    </div>

                    <div class="flex justify-end pt-1 border-t border-border/20">
                      <button
                        onClick={handleRestoreThresholdDefault}
                        class="px-3 py-1.5 bg-background hover:bg-surface border border-border rounded-xl text-accent hover:text-accent-hover transition-all text-xs font-semibold cursor-pointer"
                      >
                        Restore to Default
                      </button>
                    </div>
                  </div>
                </Show>
              </div>
            </Show>

            <Show when={activeCategory() === "permissions"}>
              {/* Path Permissions Tab */}
              <div class="space-y-4">
                <div class="flex items-center justify-between border-b border-border/30 pb-2 mb-2 flex-shrink-0">
                  <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary">
                    {t("permissions.title")}
                  </h3>
                  <Show when={permissions().length > 0}>
                    <button
                      onClick={handleClearAllPermissions}
                      class="px-2.5 py-1.5 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 rounded-xl text-red-400 transition-all text-xs font-semibold cursor-pointer"
                    >
                      {t("settings.permissions.clearAll")}
                    </button>
                  </Show>
                </div>

                <Show 
                  when={permissions().length > 0}
                  fallback={
                    <div class="flex-grow flex flex-col items-center justify-center p-8 text-text-secondary select-none text-xs">
                      {t("settings.permissions.noPermissions")}
                    </div>
                  }
                >
                  <div class="space-y-3">
                    <For each={permissions()}>
                      {(p) => (
                        <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 space-y-3">
                          <div class="space-y-1">
                            <div class="text-xs font-mono font-bold text-text-primary truncate" title={p.path}>
                              {p.path}
                            </div>
                            <div class="flex gap-4 text-[10px] text-text-secondary/70">
                              <span>{t("fileViewer.title")}: <span class={p.preview === 'allow' ? 'text-accent font-semibold' : ''}>{p.preview}</span></span>
                              <span>External: <span class={p.external === 'allow' ? 'text-accent font-semibold' : ''}>{p.external}</span></span>
                            </div>
                          </div>

                          <div class="flex gap-2 justify-end border-t border-border/20 pt-2.5 text-[10.5px]">
                            <Show when={p.preview !== 'ask'}>
                              <button
                                onClick={() => handleResetPermission(p.path, "preview")}
                                class="px-2.5 py-1.5 bg-background hover:bg-surface border border-border rounded-xl text-text-primary transition-all font-semibold cursor-pointer"
                              >
                                Reset Preview
                              </button>
                            </Show>
                            <Show when={p.external !== 'ask'}>
                              <button
                                onClick={() => handleResetPermission(p.path, "external")}
                                class="px-2.5 py-1.5 bg-background hover:bg-surface border border-border rounded-xl text-text-primary transition-all font-semibold cursor-pointer"
                              >
                                Reset External
                              </button>
                            </Show>
                            <button
                              onClick={() => handleResetPermission(p.path, "all")}
                              class="px-2.5 py-1.5 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 rounded-xl text-red-400 transition-all font-semibold cursor-pointer shadow-red-500/5 shadow"
                            >
                              {t("common.delete")}
                            </button>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>
          </div>

          {/* Delete Data Confirmation Scrim overlay */}
          <Show when={deletingSourceId() !== null}>
            <div class="absolute inset-0 bg-black/85 z-50 flex items-center justify-center p-6 animate-in fade-in duration-200">
              <div class="w-[400px] bg-surface border border-border/80 p-6 rounded-2xl flex flex-col items-center gap-4 text-center shadow-2xl animate-in zoom-in-95 duration-200">
                <AlertTriangle class="w-12 h-12 text-red-500 animate-pulse" />
                <h3 class="text-base font-bold text-red-500 uppercase tracking-wide">
                  {t("settings.sources.deleteData")}?
                </h3>
                <p class="text-xs text-text-secondary/80 leading-relaxed">
                  {t("detailPane.confirmDelete")}
                  <br /><br />
                  <span class="font-bold text-text-primary capitalize">{deletingSourceId()}</span>
                </p>
                <div class="flex gap-3 w-full pt-2">
                  <button
                    onClick={() => setDeletingSourceId(null)}
                    class="flex-1 py-2 border border-border bg-background hover:bg-surface rounded-xl text-xs font-semibold text-text-secondary hover:text-text-primary transition-all cursor-pointer"
                  >
                    {t("common.cancel")}
                  </button>
                  <button
                    onClick={() => handleDeleteSourceData(deletingSourceId()!)}
                    class="flex-1 py-2 bg-red-500 hover:bg-red-600 border border-red-600 rounded-xl text-xs font-semibold text-white transition-all cursor-pointer shadow-md"
                  >
                    {t("common.delete")}
                  </button>
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};
