import { createSignal, createMemo, For, Show } from "solid-js";
import { useI18n } from "../i18n/i18n";
import { formatDateWithSetting } from "../utils/format";
import { 
  Search, 
  Sparkles, 
  SlidersHorizontal, 
  Pin, 
  Archive, 
  RefreshCw,
  Loader2,
  Folder,
  Clock,
  MessageSquare,
  Cpu,
  Bolt
} from "lucide-solid";

interface Turn {
  turnId: string;
  userMessage: string;
  assistantMessage: string;
  timestamp: number;
  inputTokens?: number | null;
  outputTokens?: number | null;
  extraData?: Record<string, string> | null;
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
  snippet?: string | null;
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

interface SidebarProps {
  sessions: Session[];
  searchResults: SearchResult[] | null;
  selectedSessionId: string | null;
  loadingSessionId: string | null;
  onSelectSession: (session: Session) => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  isSemantic: boolean;
  onSemanticToggle: () => void;
  selectedSources: Set<string>;
  onToggleSource: (sourceId: string) => void;
  archivalFilter: "all" | "active" | "archived";
  onArchivalFilterChange: (filter: "all" | "active" | "archived") => void;
  sources: SourceMetadata[];
  isRebuilding: boolean;
  onRebuildIndex: () => void;
  indexingProgress: {
    step: string;
    progress: number;
    currentSource: string;
  } | null;
  width: number;
  onWidthChange: (w: number) => void;
  collapsed?: boolean;
  appVersion?: string;
  dateFormat: string;
  numberFormat: string;
}

export const getSessionComputeTimeMs = (session: Session): number => {
  let totalMs = 0;
  for (const turn of session.turns) {
    const extra = turn.extraData;
    const msStr = extra ? extra["computeTimeMs"] : null;
    const ms = msStr ? parseInt(msStr, 10) : null;
    if (ms !== null && !isNaN(ms) && ms > 0) {
      totalMs += Math.min(900000, ms);
    } else if (turn.assistantMessage && turn.assistantMessage.length > 0) {
      const estMs = Math.round((turn.assistantMessage.length / 120.0) * 1000.0);
      totalMs += Math.max(2000, Math.min(60000, estMs));
    }
  }
  return totalMs;
};

export const getSessionTokensCount = (session: Session): number => {
  let total = 0;
  let hasRealTokens = false;
  for (const turn of session.turns) {
    if ((turn.inputTokens !== undefined && turn.inputTokens !== null) || 
        (turn.outputTokens !== undefined && turn.outputTokens !== null)) {
      hasRealTokens = true;
      total += (turn.inputTokens || 0) + (turn.outputTokens || 0);
    }
  }
  if (hasRealTokens) return total;
  
  let charCount = 0;
  for (const turn of session.turns) {
    charCount += (turn.userMessage || "").length + (turn.assistantMessage || "").length;
  }
  return Math.round(charCount / 4);
};

export const formatSpeed = (tokens: number, ms: number): string => {
  if (ms <= 0) return "0.0 t/s";
  const tps = (tokens * 1000.0) / ms;
  return `${tps.toFixed(1)} t/s`;
};

export const formatDuration = (ms: number): string => {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  
  if (days > 0) {
    return `${days}d ${hours % 24}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes % 60}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  }
  return `${seconds}s`;
};

export const getSessionModels = (session: Session): string[] => {
  const models: string[] = [];
  for (const turn of session.turns) {
    const extra = turn.extraData;
    const m = extra ? extra["model"] : null;
    if (m && !models.includes(m)) {
      models.push(m);
    }
  }
  return models;
};

export const Sidebar = (props: SidebarProps) => {
  const { t } = useI18n();
  const [showFilters, setShowFilters] = createSignal(false);

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = props.width;
    
    const handleMouseMove = (moveEvent: MouseEvent) => {
      const newWidth = Math.max(280, Math.min(600, startWidth + (moveEvent.clientX - startX)));
      props.onWidthChange(newWidth);
    };
    
    const handleMouseUp = () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
    
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  };

  // Helper to format source tags
  const getSourceStyle = (sourceId: string) => {
    switch (sourceId.toLowerCase()) {
      case "claude":
        return "bg-emerald-500/10 text-emerald-400 border-emerald-500/20";
      case "antigravity":
        return "bg-violet-500/10 text-violet-400 border-violet-500/20";
      case "cursor":
        return "bg-sky-500/10 text-sky-400 border-sky-500/20";
      case "aider":
        return "bg-amber-500/10 text-amber-400 border-amber-500/20";
      case "copilot":
        return "bg-pink-500/10 text-pink-400 border-pink-500/20";
      case "codex":
        return "bg-blue-500/10 text-blue-400 border-blue-500/20";
      default:
        return "bg-slate-500/10 text-slate-400 border-slate-500/20";
    }
  };

  const getSourceLabel = (sourceId: string) => {
    const found = props.sources.find(s => s.id === sourceId);
    return found ? found.displayName : sourceId;
  };

  // Helper to format timestamps to relative/absolute datetime strings
  const formatRelativeTime = (timestampMs: number) => {
    let time = timestampMs;
    const now = Date.now();
    
    if (time < 20000000000) {
      time *= 1000;
    }

    const dateObj = new Date(time);
    const timeStr = dateObj.toLocaleTimeString(undefined, { timeStyle: "short" });

    // Check if it's today
    const nowObj = new Date(now);
    const isToday = dateObj.getDate() === nowObj.getDate() &&
                    dateObj.getMonth() === nowObj.getMonth() &&
                    dateObj.getFullYear() === nowObj.getFullYear();

    if (isToday) {
      return timeStr;
    }

    // Check if it's yesterday
    const yesterday = new Date(now - 86400000);
    const isYesterday = dateObj.getDate() === yesterday.getDate() &&
                        dateObj.getMonth() === yesterday.getMonth() &&
                        dateObj.getFullYear() === yesterday.getFullYear();

    if (isYesterday) {
      return `${t("sidebar.yesterday") || "Yesterday"}, ${timeStr}`;
    }

    // Otherwise, show full date and time according to settings
    const dateStr = formatDateWithSetting(dateObj, props.dateFormat || "system");

    return `${dateStr}, ${timeStr}`;
  };

  // Extract a text snippet from a session's turns
  const getSessionSnippet = (session: Session, matchedTurns?: number[]) => {
    if (matchedTurns && matchedTurns.length > 0 && session.turns) {
      const idx = matchedTurns[0];
      const turn = session.turns[idx];
      if (turn) {
        return turn.userMessage.substring(0, 100).replace(/\s+/g, " ") || 
               turn.assistantMessage.substring(0, 100).replace(/\s+/g, " ");
      }
    }
    if (session.snippet) {
      return session.snippet;
    }
    if (session.turns && session.turns.length > 0) {
      const lastTurn = session.turns[session.turns.length - 1];
      if (lastTurn) {
        return lastTurn.userMessage.substring(0, 100).replace(/\s+/g, " ") || 
               lastTurn.assistantMessage.substring(0, 100).replace(/\s+/g, " ");
      }
    }
    return t("sidebar.noMessages");
  };

  // Determine what to display based on search results and filters
  const listItems = createMemo(() => {
    if (props.searchResults !== null) {
      return props.searchResults.map(r => ({
        session: r.session,
        matchedTurns: r.matchedTurnIndexes,
        score: r.score
      }));
    }

    return props.sessions
      .filter(s => {
        // Source filter
        if (props.selectedSources.size > 0 && !props.selectedSources.has(s.sourceId)) {
          return false;
        }
        // Archival filter
        if (props.archivalFilter === "active" && s.isArchived) return false;
        if (props.archivalFilter === "archived" && !s.isArchived) return false;
        return true;
      })
      .map(s => ({
        session: s,
        matchedTurns: undefined,
        score: undefined
      }));
  });

  return (
    <aside 
      class="border-r border-border h-full flex flex-col overflow-hidden bg-background select-none relative"
      style={{
        width: props.collapsed ? "0px" : `${props.width}px`,
        "min-width": props.collapsed ? "0px" : `${props.width}px`,
        "max-width": props.collapsed ? "0px" : `${props.width}px`,
        display: props.collapsed ? "none" : "flex",
        "padding-top": "0px"
      }}
    >
      {/* Drag Handle */}
      <div 
        onMouseDown={handleMouseDown}
        class="absolute right-0 w-1 cursor-col-resize hover:bg-accent/40 active:bg-accent/60 transition-colors z-50 select-none"
        style={{
          top: "0px",
          height: "100%"
        }}
      />
      {/* Sticky Header Section */}
      <div class="p-4 border-b border-border space-y-3 flex-shrink-0">
        <div class="flex items-center justify-between">
          <span class="text-[18px] font-semibold text-text-primary tracking-wide">
            {t("sidebar.title")}
          </span>
          <button 
            onClick={() => props.onRebuildIndex()}
            disabled={props.isRebuilding}
            title={t("sidebar.forceRebuild")}
            class="p-1.5 hover:bg-surface border border-border/40 rounded-lg text-text-secondary hover:text-accent transition-all disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw class={`w-4 h-4 ${props.isRebuilding ? 'animate-spin text-accent' : ''}`} />
          </button>
        </div>

        {/* Search Bar Group */}
        <div class="flex gap-2">
          <div class="relative flex-grow">
            <Search class="absolute left-3 top-2.5 w-4 h-4 text-text-secondary" />
            <input
              type="text"
              value={props.searchQuery}
              onInput={(e) => props.onSearchChange(e.currentTarget.value)}
              placeholder={t("sidebar.searchPlaceholder")}
              class="w-full bg-surface border border-border hover:border-border/80 focus:border-accent text-text-primary pl-9 pr-4 py-2 text-sm rounded-xl outline-none transition-all placeholder:text-text-secondary/60"
            />
          </div>
          <button
            onClick={() => props.onSemanticToggle()}
            title={props.isSemantic ? t("sidebar.semanticEnabled") : t("sidebar.lexicalEnabled")}
            class={`p-2.5 rounded-xl border transition-all flex items-center justify-center cursor-pointer ${
              props.isSemantic 
                ? "bg-accent/15 border-accent text-accent shadow-sm shadow-accent/20" 
                : "bg-surface border-border text-text-secondary hover:text-text-primary hover:border-border/80"
            }`}
          >
            <Sparkles class="w-4 h-4" />
          </button>
          <button
            onClick={() => setShowFilters(!showFilters())}
            class={`p-2.5 rounded-xl border transition-all flex items-center justify-center cursor-pointer ${
              showFilters() 
                ? "bg-surface border-accent text-accent" 
                : "bg-surface border-border text-text-secondary hover:text-text-primary"
            }`}
          >
            <SlidersHorizontal class="w-4 h-4" />
          </button>
        </div>

        {/* Collapsible Filter panel */}
        <Show when={showFilters()}>
          <div class="p-3 bg-surface/50 border border-border/80 rounded-xl space-y-3 animate-in fade-in slide-in-from-top-2 duration-200">
            {/* Source checkboxes */}
            <div class="space-y-1.5">
              <div class="text-xs font-semibold text-text-secondary uppercase tracking-wider">
                {t("sidebar.sources")}
              </div>
              <div class="grid grid-cols-2 gap-1.5">
                <For each={props.sources}>
                  {(src) => {
                    const isChecked = createMemo(() => props.selectedSources.has(src.id));
                    return (
                      <label 
                        class={`flex items-center gap-2 px-2.5 py-1.5 border rounded-lg text-xs cursor-pointer transition-all ${
                          isChecked() 
                            ? "bg-accent/10 border-accent/40 text-accent font-medium" 
                            : "border-border/40 hover:bg-surface text-text-secondary"
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={isChecked()}
                          onChange={() => props.onToggleSource(src.id)}
                          class="hidden"
                        />
                        <span>{src.displayName}</span>
                      </label>
                    );
                  }}
                </For>
              </div>
            </div>

            {/* Archival segmented controls */}
            <div class="space-y-1.5">
              <div class="text-xs font-semibold text-text-secondary uppercase tracking-wider">
                {t("sidebar.statusFilter")}
              </div>
              <div class="flex bg-surface p-1 rounded-lg border border-border/60">
                <For each={["all", "active", "archived"] as const}>
                  {(tab) => (
                    <button
                      onClick={() => props.onArchivalFilterChange(tab)}
                      class={`flex-1 text-center py-1 text-xs rounded-md transition-all capitalize cursor-pointer ${
                        props.archivalFilter === tab 
                          ? "bg-background text-accent border border-border font-medium shadow-sm" 
                          : "text-text-secondary hover:text-text-primary"
                      }`}
                    >
                      {t(`sidebar.filter${tab.charAt(0).toUpperCase() + tab.slice(1)}`)}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>
        </Show>
      </div>

      {/* Indexing Progress Indicator */}
      <Show when={props.indexingProgress}>
        <div class="px-4 py-3 bg-accent/5 border-b border-border/40 space-y-1.5 flex-shrink-0 animate-in fade-in slide-in-from-top-1 duration-150">
          <div class="flex items-center justify-between text-[11px] font-medium">
            <span class="text-accent uppercase tracking-wider font-semibold animate-pulse">
              {props.indexingProgress!.step === "complete" ? "Finished" : "Indexing"}
            </span>
            <span class="text-text-secondary truncate max-w-[180px]">
              {props.indexingProgress!.currentSource}
            </span>
            <span class="font-mono text-accent">
              {Math.round(props.indexingProgress!.progress * 100)}%
            </span>
          </div>
          <div class="h-1.5 w-full bg-border/40 rounded-full overflow-hidden">
            <div 
              class="h-full bg-accent transition-all duration-300 ease-out" 
              style={{ width: `${props.indexingProgress!.progress * 100}%` }}
            />
          </div>
        </div>
      </Show>

      {/* Sessions List Area */}
      <div class="flex-grow overflow-y-auto min-h-0 divide-y divide-border/30">
        <Show 
          when={listItems().length > 0} 
          fallback={
            <div class="p-8 text-center text-text-secondary text-sm">
              No matching sessions found.
            </div>
          }
        >
          <For each={listItems()}>
            {({ session, matchedTurns, score }) => {
              const isSelected = createMemo(() => props.selectedSessionId === session.id);
              const snippet = createMemo(() => getSessionSnippet(session, matchedTurns));
              const relativeTime = createMemo(() => formatRelativeTime(session.updatedAt));
              
              return (
                <SessionCard
                  session={session}
                  isSelected={isSelected()}
                  isLoading={props.loadingSessionId === session.id}
                  onSelect={props.onSelectSession}
                  snippet={snippet()}
                  relativeTime={relativeTime()}
                  score={score}
                  getSourceStyle={getSourceStyle}
                  getSourceLabel={getSourceLabel}
                />
              );
            }}
          </For>
        </Show>
      </div>
    </aside>
  );
};

interface SessionCardProps {
  session: Session;
  isSelected: boolean;
  isLoading: boolean;
  onSelect: (session: Session) => void;
  snippet: string;
  relativeTime: string;
  score?: number;
  getSourceStyle: (sourceId: string) => string;
  getSourceLabel: (sourceId: string) => string;
}

const SessionCard = (props: SessionCardProps) => {
  const title = createMemo(() => props.session.threadName || "Untitled Session");
  const models = createMemo(() => getSessionModels(props.session));
  const durationMs = createMemo(() => getSessionComputeTimeMs(props.session));
  const tokensCount = createMemo(() => getSessionTokensCount(props.session));
  const speedText = createMemo(() => formatSpeed(tokensCount(), durationMs()));
  const formattedDuration = createMemo(() => formatDuration(durationMs()));
  const turnsCount = createMemo(() => props.session.turns.length);
  const formattedTokens = createMemo(() => {
    const t = tokensCount();
    if (t >= 1000000) {
      return `${(t / 1000000).toFixed(1)}M`;
    }
    if (t >= 1000) {
      return `${(t / 1000).toFixed(1)}k`;
    }
    return String(t);
  });
  
  const getWorkspaceFolder = () => {
    if (!props.session.cwd) return "";
    const parts = props.session.cwd.split(/[/\\]/);
    return parts.filter(Boolean).pop() || "";
  };

  return (
    <div
      onClick={() => props.onSelect(props.session)}
      class={`p-4 flex flex-col gap-2.5 cursor-pointer transition-all border-b border-border/20 ${
        props.isSelected 
          ? "bg-accent-light/35 border-l-2 border-accent" 
          : "hover:bg-surface/20 border-l-2 border-transparent"
      }`}
    >
      {/* Title & Badge */}
      <div class="flex items-start justify-between gap-2">
        <span class={`text-[13.5px] font-semibold leading-snug break-all line-clamp-2 ${
          props.isSelected ? "text-accent" : "text-text-primary/95"
        }`}>
          {title()}
        </span>
        <div class="flex items-center gap-1.5 flex-shrink-0 pt-0.5">
          <span class="text-[10px] text-text-secondary/50 font-normal mr-1">{props.relativeTime}</span>
          <Show when={props.isLoading}>
            <Loader2 class="w-3.5 h-3.5 text-accent animate-spin" />
          </Show>
          <Show when={props.session.isPinned}>
            <Pin class="w-3.5 h-3.5 text-accent animate-pulse" />
          </Show>
          <Show when={props.session.isArchived}>
            <Archive class="w-3.5 h-3.5 text-text-secondary" />
          </Show>
        </div>
      </div>

      {/* Models & Speed */}
      <Show when={models().length > 0}>
        <div class="flex items-center justify-between gap-2 text-[10.5px]">
          <span class="text-accent/80 font-medium truncate max-w-[200px]" title={models().join(", ")}>
            {models().join(", ")}
          </span>
          <Show when={durationMs() > 0}>
            <div class="flex items-center gap-0.5 text-accent font-semibold flex-shrink-0">
              <Bolt class="w-3 h-3" />
              <span>{speedText()}</span>
            </div>
          </Show>
        </div>
      </Show>

      {/* Snippet preview */}
      <p class="text-xs text-text-secondary/70 line-clamp-2 break-all leading-normal">
        {props.snippet}
      </p>

      {/* Footer Metadata */}
      <div class="flex items-center justify-between text-[10.5px] mt-0.5 text-text-secondary/60 gap-2">
        {/* Left Side: Source & CWD */}
        <div class="flex items-center gap-1.5 min-w-0">
          <span class={`px-1.5 py-0.5 border rounded text-[9.5px] uppercase font-bold flex-shrink-0 ${props.getSourceStyle(props.session.sourceId)}`}>
            {props.getSourceLabel(props.session.sourceId)}
          </span>
          <Show when={getWorkspaceFolder()}>
            <div class="flex items-center gap-0.5 min-w-0 text-text-secondary/50">
              <Folder class="w-3 h-3 flex-shrink-0" />
              <span class="truncate" title={props.session.cwd || ""}>{getWorkspaceFolder()}</span>
            </div>
          </Show>
        </div>
        
        {/* Right Side: Stats */}
        <div class="flex items-center gap-2 flex-shrink-0 text-text-secondary/50">
          <div class="flex items-center gap-0.5">
            <Clock class="w-3 h-3" />
            <span>{formattedDuration()}</span>
          </div>
          <div class="flex items-center gap-0.5">
            <MessageSquare class="w-3 h-3" />
            <span>{turnsCount()}</span>
          </div>
          <div class="flex items-center gap-0.5">
            <Cpu class="w-3 h-3" />
            <span>{formattedTokens()}</span>
          </div>
        </div>
      </div>
    </div>
  );
};
