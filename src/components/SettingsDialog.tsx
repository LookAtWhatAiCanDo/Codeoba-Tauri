import { createSignal, For, Show, onMount } from "solid-js";
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
import { check } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";
import { logFE } from "../utils/logger";

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
  const [activeCategory, setActiveCategory] = createSignal<Category>("general");
  const [deletingSourceId, setDeletingSourceId] = createSignal<string | null>(null);
  const [checkingUpdates, setCheckingUpdates] = createSignal(false);
  const [updateCheckResult, setUpdateCheckResult] = createSignal<string | null>(null);
  const [updaterActive, setUpdaterActive] = createSignal(false);
  const [appVersion, setAppVersion] = createSignal("0.1.0");

  onMount(async () => {
    try {
      const active = await invoke<boolean>("is_updater_active");
      setUpdaterActive(active);
      const v = await getVersion();
      setAppVersion(v);
    } catch (err) {
      logFE("error", `Failed to query updater active/version state: ${err}`);
    }
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

  // Semantic Settings
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

  // Path Permissions (mock list stored in localStorage)
  const [permissions, setPermissions] = createSignal<Array<{ path: string; preview: string; external: string }>>(
    JSON.parse(localStorage.getItem("codeoba-path-permissions") || "[]")
  );

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

  const handleResetPermission = (path: string, type: "preview" | "external" | "all") => {
    let list = [...permissions()];
    if (type === "all") {
      list = list.filter(p => p.path !== path);
    } else {
      const item = list.find(p => p.path === path);
      if (item) {
        if (type === "preview") item.preview = "ask";
        if (type === "external") item.external = "ask";
      }
    }
    setPermissions(list);
    localStorage.setItem("codeoba-path-permissions", JSON.stringify(list));
  };

  const handleClearAllPermissions = () => {
    setPermissions([]);
    localStorage.setItem("codeoba-path-permissions", "[]");
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
              <span class="font-bold text-text-primary tracking-wide">Settings</span>
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
                <span>General Settings</span>
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
                <span>Sources & Adapters</span>
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
                <span>Semantic Search</span>
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
                <span>Path Permissions</span>
              </button>
            </div>
          </div>

          {/* Right Pane Content Area */}
          <div class="flex-grow h-full flex flex-col p-6 pt-8 overflow-y-auto min-w-0">
            <Show when={activeCategory() === "general"}>
              {/* General Settings Tab */}
              <div class="space-y-5">
                <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary mb-2">
                  General Settings
                </h3>

                {/* Theme Selector Dot Bar */}
                <div class="bg-surface/30 border border-border/50 rounded-2xl p-4 space-y-2">
                  <div class="flex items-center justify-between">
                    <div>
                      <h4 class="text-xs font-bold text-text-primary">Application Theme</h4>
                      <p class="text-[10px] text-text-secondary/70">Select the visual appearance of the workspace.</p>
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
                    <h4 class="text-xs font-bold text-text-primary">Persistent Startup Cache</h4>
                    <p class="text-[10px] text-text-secondary/70">Speed up startup time by caching parsed sessions on disk.</p>
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
                      <span class="text-text-secondary">Current Version: v{appVersion()}</span>
                      <button
                        onClick={handleCheckUpdates}
                        disabled={checkingUpdates()}
                        class="px-3 py-1.5 bg-background hover:bg-surface border border-border rounded-xl text-accent hover:text-accent-hover transition-all text-xs font-semibold cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
                      >
                        <Show when={checkingUpdates()} fallback={<span>Check for Updates</span>}>
                          <RefreshCw class="w-3.5 h-3.5 animate-spin" />
                          <span>Checking...</span>
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
                    <h4 class="text-xs font-bold text-text-primary">Log Parsing Mode</h4>
                    <p class="text-[10px] text-text-secondary/70">Configure how conversation transcripts are processed and summarized.</p>
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
                      Standard Parsing
                    </button>
                    <button
                      onClick={() => handleParserModeChange("summarizing")}
                      class={`flex-1 text-center py-1.5 text-xs font-semibold rounded-md transition-all cursor-pointer ${
                        parserMode() === "summarizing" 
                          ? "bg-surface text-accent border border-border/80 shadow-sm" 
                          : "text-text-secondary hover:text-text-primary"
                      }`}
                    >
                      AI Summarizing
                    </button>
                  </div>
                </div>
              </div>
            </Show>

            <Show when={activeCategory() === "sources"}>
              {/* Sources & Adapters Tab */}
              <div class="space-y-4">
                <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary mb-2">
                  Sources & Adapters
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
                              Status: {src.isAvailable ? 'Monitoring active' : 'Not available'}
                            </p>
                          </div>

                          <div class="flex items-center gap-2">
                            {/* Segmented controls for allow/deny/ask */}
                            <div class="flex bg-background p-0.5 rounded-lg border border-border/50 text-[10px] font-semibold">
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
                              title="Delete Parser Cache Database"
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
                  Semantic Search Settings
                </h3>

                <div class="bg-surface/30 border border-border/50 rounded-2xl p-5 space-y-4">
                  <div class="space-y-1">
                    <h4 class="text-xs font-bold text-text-primary">Similarity Threshold</h4>
                    <p class="text-[10px] text-text-secondary/70">
                      Configure the minimum confidence score required for search matches. Lower values return more results (fuzzier), higher values return fewer results (stricter).
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
              </div>
            </Show>

            <Show when={activeCategory() === "permissions"}>
              {/* Path Permissions Tab */}
              <div class="space-y-4">
                <div class="flex items-center justify-between border-b border-border/30 pb-2 mb-2 flex-shrink-0">
                  <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary">
                    Workspace Path Permissions
                  </h3>
                  <Show when={permissions().length > 0}>
                    <button
                      onClick={handleClearAllPermissions}
                      class="px-2.5 py-1.5 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 rounded-xl text-red-400 transition-all text-xs font-semibold cursor-pointer"
                    >
                      Clear All
                    </button>
                  </Show>
                </div>

                <Show 
                  when={permissions().length > 0}
                  fallback={
                    <div class="flex-grow flex flex-col items-center justify-center p-8 text-text-secondary select-none text-xs">
                      No custom path permissions saved.
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
                              <span>Preview: <span class={p.preview === 'allow' ? 'text-accent font-semibold' : ''}>{p.preview}</span></span>
                              <span>External Open: <span class={p.external === 'allow' ? 'text-accent font-semibold' : ''}>{p.external}</span></span>
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
                              class="px-2.5 py-1.5 bg-background hover:bg-red-500/10 border border-border hover:border-red-500/20 rounded-xl text-red-400 transition-all font-semibold cursor-pointer"
                            >
                              Clear
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
                  Delete Data Permanently?
                </h3>
                <p class="text-xs text-text-secondary/80 leading-relaxed">
                  Are you sure you want to permanently delete the database and session cache files for <span class="font-bold text-text-primary capitalize">{deletingSourceId()}</span>?
                  <br /><br />
                  This action is irreversible and requires a full rebuild index to reload the data.
                </p>
                <div class="flex gap-3 w-full pt-2">
                  <button
                    onClick={() => setDeletingSourceId(null)}
                    class="flex-1 py-2 border border-border bg-background hover:bg-surface rounded-xl text-xs font-semibold text-text-secondary hover:text-text-primary transition-all cursor-pointer"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => handleDeleteSourceData(deletingSourceId()!)}
                    class="flex-1 py-2 bg-red-500 hover:bg-red-600 border border-red-600 rounded-xl text-xs font-semibold text-white transition-all cursor-pointer shadow-md"
                  >
                    Delete Data
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
