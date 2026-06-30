import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Show } from "solid-js";
import { 
  Folder, 
  Copy, 
  Check, 
  Clock, 
  ExternalLink,
  MessageSquare,
  Cpu,
  Bookmark,
  ChevronDown,
  ChevronRight,
  Terminal,
  Search,
  FileText
} from "lucide-solid";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { useI18n } from "../i18n/i18n";
import { logFE } from "../utils/logger";
import { parseAssistantMessage, MessageToolPart } from "../utils/messageParser";
import { formatDateWithSetting, formatNumberWithSetting } from "../utils/format";

interface Turn {
  turnId: string;
  userMessage: string;
  assistantMessage: string;
  timestamp: number;
  inputTokens?: number | null;
  outputTokens?: number | null;
  extraData?: Record<string, string>;
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

interface DetailPaneProps {
  session: Session | null;
  onCopyPath: (path: string) => void;
  loadTime: string | null;
  isLoading: boolean;
  sidebarCollapsed?: boolean;
  searchQuery?: string;
  dateFormat?: string;
  numberFormat?: string;
}

export const DetailPane = (props: DetailPaneProps) => {
  const { t } = useI18n();
  const [copiedPath, setCopiedPath] = createSignal(false);
  const [copiedSession, setCopiedSession] = createSignal(false);
  const [visibleTurns, setVisibleTurns] = createSignal(10);

  const compactionCount = createMemo(() => {
    if (!props.session) return 0;
    return props.session.turns.filter(t => t.extraData?.isCompaction === "true").length;
  });

  let scrollContainerRef: HTMLDivElement | undefined;
  const visibilitySetters = new Map<Element, (v: boolean) => void>();
  const heightCache = new Map<string, number>();
  let observer: IntersectionObserver | undefined;

  onMount(() => {
    observer = new IntersectionObserver((entries) => {
      entries.forEach(entry => {
        const setter = visibilitySetters.get(entry.target);
        if (setter) {
          setter(entry.isIntersecting);
        }
      });
    }, {
      rootMargin: "500px 0px" // Render turns 500px above/below viewport to prevent flickers
    });
  });

  onCleanup(() => {
    if (observer) {
      observer.disconnect();
    }
  });

  const registerElement = (el: HTMLElement, setVisible: (v: boolean) => void, _turnId: string) => {
    visibilitySetters.set(el, setVisible);
    if (observer) {
      observer.observe(el);
    }
  };

  const unregisterElement = (el: HTMLElement) => {
    visibilitySetters.delete(el);
    if (observer) {
      observer.unobserve(el);
    }
  };

  const getCachedHeight = (turnId: string) => heightCache.get(turnId);
  const setCachedHeight = (turnId: string, h: number) => heightCache.set(turnId, h);

  // Reset pagination and scroll to bottom when session changes
  createEffect(() => {
    const id = props.session?.id;
    if (id) {
      setVisibleTurns(10);
      
      // Auto-scroll to bottom of conversation turns
      setTimeout(() => {
        if (scrollContainerRef) {
          scrollContainerRef.scrollTop = scrollContainerRef.scrollHeight;
          logFE("info", `Auto-scrolled session detail scrollbar to bottom`);
        }
      }, 50);
    }
  });

  // Extract folder name from CWD as "Workspace"
  const getWorkspaceName = () => {
    if (!props.session?.cwd) return "Local Workspace";
    const parts = props.session.cwd.split(/[/\\]/);
    return parts.filter(Boolean).pop() || "Workspace";
  };

  const handleCopyPath = () => {
    if (props.session) {
      props.onCopyPath(props.session.filePath);
      setCopiedPath(true);
      setTimeout(() => setCopiedPath(false), 2000);
    }
  };

  const handleCopyFullSession = () => {
    if (props.session) {
      const formatted = props.session.turns.map(turn => {
        return `### User\n\n${turn.userMessage}\n\n### Assistant\n\n${turn.assistantMessage}\n`;
      }).join("\n---\n\n");
      
      navigator.clipboard.writeText(formatted);
      setCopiedSession(true);
      setTimeout(() => setCopiedSession(false), 2000);
    }
  };

  const formatFullDate = (timestampMs: number) => {
    let time = timestampMs;
    if (time < 20000000000) {
      time *= 1000;
    }
    const dateObj = new Date(time);
    const dateStr = formatDateWithSetting(dateObj, props.dateFormat || "system");
    const timeStr = dateObj.toLocaleTimeString(undefined, { timeStyle: "short" });
    return `${dateStr}, ${timeStr}`;
  };

  const slicedTurns = createMemo(() => {
    if (!props.session) return [];
    return props.session.turns.slice(-visibleTurns());
  });

  return (
    <div class="flex-grow h-full flex flex-col bg-background/95 min-w-0">
      <Show 
        when={!props.isLoading} 
        fallback={
          <div class="flex-grow h-full flex flex-col bg-background/95 min-w-0 animate-pulse">
            {/* Header Skeleton */}
            <div 
              class="px-6 border-b border-border/60 flex items-center justify-between flex-shrink-0"
              style={{ height: "var(--sk-header-height, 76px)" }}
            >
              <div class="flex flex-col gap-2">
                <div class="h-3.5 w-40 bg-surface rounded" />
                <div class="h-2.5 w-60 bg-surface rounded" />
              </div>
            </div>
            
            {/* Messages Scroll Area Skeleton */}
            <div class="flex-grow px-8 py-6 space-y-6 overflow-y-auto">
              <div class="p-4 bg-surface/30 border border-border/40 rounded-2xl flex gap-6">
                <div class="h-4 w-24 bg-surface rounded" />
                <div class="h-4 w-32 bg-surface rounded" />
                <div class="h-4 w-20 bg-surface rounded" />
              </div>

              {[1, 2].map((_i) => (
                <div class="space-y-4">
                  <div class="flex flex-col items-start max-w-2xl">
                    <div class="h-3 w-16 bg-surface rounded mb-2 ml-3" />
                    <div class="w-96 h-12 bg-surface border border-border/50 rounded-2xl" />
                  </div>
                  <div class="flex flex-col items-start max-w-3xl pl-6">
                    <div class="h-3 w-20 bg-surface rounded mb-2 ml-3" />
                    <div class="w-full h-32 bg-surface/50 border border-border/30 rounded-2xl" />
                  </div>
                </div>
              ))}
            </div>
          </div>
        }
      >
        <Show 
          when={props.session} 
          fallback={
            <div class="flex-grow h-full flex flex-col items-center justify-center bg-background/95 text-text-secondary select-none">
              <MessageSquare class="w-16 h-16 mb-4 text-border animate-pulse" />
              <p class="text-[15px] font-medium tracking-wide">{t("detailPane.selectSession")}</p>
            </div>
          }
        >
          {/* Top Header / Action Bar */}
          <div 
            class="border-b border-border/60 flex items-center justify-between glass flex-shrink-0 transition-all duration-200 px-6"
            style={{ 
              height: "var(--sk-header-height, 76px)"
            }}
          >
            <div class="min-w-0 flex flex-col gap-0.5 pt-2">
              <div class="flex items-center gap-1.5 text-xs text-text-secondary/80">
                <span class="hover:text-text-primary transition-colors cursor-default">
                  {getWorkspaceName()}
                </span>
                <span class="text-border">/</span>
                <span class="truncate font-medium text-text-primary max-w-[240px] cursor-default">
                  {props.session!.threadName || t("detailPane.noSelection")}
                </span>
                <Show when={compactionCount() > 0}>
                  <span class="px-2 py-0.5 bg-accent/15 border border-accent/30 text-accent rounded-full text-[9px] font-bold select-none leading-none pt-[3px] pb-[3px]">
                    {t("dashboard.totalCompactions")}: {compactionCount()}
                  </span>
                </Show>
              </div>
              
              <Show when={props.session!.cwd}>
                <div dir="ltr" class="flex items-center gap-1.5 text-[11px] text-text-secondary/60 text-left">
                  <Folder class="w-3.5 h-3.5 flex-shrink-0" />
                  <span class="truncate hover:text-text-primary transition-colors" title={props.session!.cwd!}>
                    {props.session!.cwd}
                  </span>
                </div>
              </Show>
            </div>

            <div class="flex items-center gap-2">
              <button
                onClick={handleCopyPath}
                title={t("detailPane.copyPath")}
                class="p-2 bg-surface hover:bg-surface/80 border border-border/80 rounded-xl text-text-secondary hover:text-text-primary transition-all flex items-center gap-1.5 text-xs font-medium cursor-pointer"
              >
                <Show when={copiedPath()} fallback={<ExternalLink class="w-3.5 h-3.5" />}>
                  <Check class="w-3.5 h-3.5 text-emerald-400" />
                </Show>
                <span>{t("detailPane.copyPathLabel")}</span>
              </button>

              <button
                onClick={handleCopyFullSession}
                title={t("detailPane.copyCwd")}
                class="p-2 bg-surface hover:bg-surface/80 border border-border/80 rounded-xl text-text-secondary hover:text-text-primary transition-all flex items-center gap-1.5 text-xs font-medium cursor-pointer"
              >
                <Show when={copiedSession()} fallback={<Copy class="w-3.5 h-3.5" />}>
                  <Check class="w-3.5 h-3.5 text-emerald-400" />
                </Show>
                <span>{t("detailPane.copyCwdLabel")}</span>
              </button>
            </div>
          </div>

          {/* Main Conversation Turns Scrollable Area */}
          <div 
            ref={scrollContainerRef}
            class="flex-grow overflow-y-auto px-8 py-6 space-y-6 scroll-smooth"
          >
            {/* Session Metadata Panel */}
            <div class="p-4 bg-surface/30 border border-border/40 rounded-2xl flex flex-wrap gap-y-3 gap-x-6 text-xs text-text-secondary/70">
              <div class="flex items-center gap-1.5">
                <Bookmark class="w-3.5 h-3.5 text-accent" />
                <span class="font-semibold text-text-primary">{t("settings.sources.tab")}:</span>
                <span class="capitalize">{props.session!.sourceId}</span>
              </div>
              <div class="flex items-center gap-1.5">
                <Clock class="w-3.5 h-3.5 text-accent" />
                <span class="font-semibold text-text-primary">{t("settings.permissions.authorizedOn")}:</span>
                <span>{formatFullDate(props.session!.timestamp)}</span>
              </div>
              <div class="flex items-center gap-1.5">
                <Cpu class="w-3.5 h-3.5 text-accent" />
                <span class="font-semibold text-text-primary">{t("dashboard.totalTurns")}:</span>
                <span>{props.session!.turns.length}</span>
              </div>
              <Show when={props.loadTime}>
                <div class="flex items-center gap-1.5">
                  <Clock class="w-3.5 h-3.5 text-accent animate-pulse" />
                  <span class="font-semibold text-text-primary">{t("dashboard.duration")}:</span>
                  <span class="font-mono text-accent">{props.loadTime}</span>
                </div>
              </Show>
            </div>

            {/* Pagination Trigger */}
            <Show when={props.session!.turns.length > visibleTurns()}>
              <div class="flex justify-center pb-4 border-b border-border/40 gap-3">
                <button
                  onClick={() => setVisibleTurns(prev => Math.min(props.session!.turns.length, prev + 20))}
                  class="px-4 py-2 bg-surface hover:bg-surface/80 border border-border text-xs font-semibold rounded-xl text-text-secondary hover:text-text-primary transition-all cursor-pointer shadow-sm"
                >
                  Load 20 older messages ({props.session!.turns.length - visibleTurns()} remaining)
                </button>
                <button
                  onClick={() => setVisibleTurns(props.session!.turns.length)}
                  class="px-4 py-2 bg-surface hover:bg-surface/80 border border-border text-xs font-semibold rounded-xl text-text-secondary hover:text-text-primary transition-all cursor-pointer shadow-sm shadow-accent/5 capitalize"
                >
                  {t("sidebar.filterAll")}
                </button>
              </div>
            </Show>

            {/* Render Virtualized Conversation Bubbles */}
            <For each={slicedTurns()}>
              {(turn, index) => {
                const actualIndex = createMemo(() => props.session!.turns.length - visibleTurns() + index());
                return (
                  <VirtualTurn
                    turn={turn}
                    actualIndex={actualIndex()}
                    formatFullDate={formatFullDate}
                    sourceId={props.session!.sourceId}
                    registerElement={registerElement}
                    unregisterElement={unregisterElement}
                    getCachedHeight={getCachedHeight}
                    setCachedHeight={setCachedHeight}
                    searchQuery={props.searchQuery}
                    numberFormat={props.numberFormat}
                  />
                );
              }}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
};

interface VirtualTurnProps {
  turn: Turn;
  actualIndex: number;
  formatFullDate: (timestamp: number) => string;
  sourceId: string;
  registerElement: (el: HTMLElement, setVisible: (v: boolean) => void, turnId: string) => void;
  unregisterElement: (el: HTMLElement) => void;
  getCachedHeight: (turnId: string) => number | undefined;
  setCachedHeight: (turnId: string, h: number) => void;
  searchQuery?: string;
  numberFormat?: string;
}

const VirtualTurn = (props: VirtualTurnProps) => {
  const { t } = useI18n();
  let elementRef: HTMLDivElement | undefined;
  const [isVisible, setIsVisible] = createSignal(false);
  const turnKey = createMemo(() => props.turn.turnId || String(props.actualIndex));

  createEffect(() => {
    const el = elementRef;
    if (el) {
      props.registerElement(el, setIsVisible, turnKey());
      onCleanup(() => {
        props.unregisterElement(el);
      });
    }
  });

  // Track height of this turn when it goes offscreen
  createEffect(() => {
    const visible = isVisible();
    const el = elementRef;
    if (!visible && el) {
      const cached = props.getCachedHeight(turnKey());
      if (cached) {
        el.style.height = `${cached}px`;
      }
    } else if (visible && el) {
      el.style.height = "auto";
      
      const ro = new ResizeObserver(entries => {
        for (const entry of entries) {
          const h = entry.target.getBoundingClientRect().height;
          if (h > 0) {
            props.setCachedHeight(turnKey(), h);
          }
        }
      });
      ro.observe(el);
      onCleanup(() => ro.disconnect());
    }
  });

  return (
    <div 
      ref={elementRef}
      data-turn-id={turnKey()}
      class="space-y-4"
      style={props.actualIndex >= 2 ? {
        "content-visibility": "auto",
        "contain-intrinsic-size": "auto 200px"
      } : undefined}
    >
      <Show 
        when={isVisible()} 
        fallback={
          // Empty skeleton shell while virtualized out to minimize memory
          <div class="w-full py-6 flex items-center justify-center text-text-secondary/20">
            <div class="flex gap-1.5">
              <div class="w-2 h-2 rounded-full bg-current animate-pulse" />
              <div class="w-2 h-2 rounded-full bg-current animate-pulse delay-75" />
              <div class="w-2 h-2 rounded-full bg-current animate-pulse delay-150" />
            </div>
          </div>
        }
      >
        {/* User message block */}
        <div class="flex flex-col items-start max-w-4xl animate-in fade-in duration-200">
          <div class="flex items-center gap-2 mb-1.5 pl-3">
            <div class="w-2 h-2 rounded-full bg-accent" />
            <span class="text-[12px] font-semibold text-text-primary tracking-wide">
              {t("common.user")}
            </span>
            <span class="text-[10px] text-text-secondary/50">
              {props.formatFullDate(props.turn.timestamp)}
            </span>
          </div>
          <div class="w-full bg-surface border border-border/50 p-4 rounded-2xl text-[14.5px] leading-relaxed text-text-primary/90 font-sans shadow-sm">
            <p class="whitespace-pre-wrap">{props.turn.userMessage}</p>
          </div>
        </div>

        {/* Assistant message block */}
        <div class="flex flex-col items-start max-w-4xl pl-2 md:pl-6 animate-in fade-in duration-200">
          <div class="flex items-center justify-between w-full mb-1.5 pl-3 pr-2">
            <div class="flex items-center gap-2">
              <div class="w-2 h-2 rounded-full bg-emerald-400" />
              <span class="text-[12px] font-semibold text-text-primary tracking-wide">
                {t("common.assistant")}
              </span>
            </div>
            <Show when={props.turn.inputTokens || props.turn.outputTokens}>
              <div class="flex items-center gap-1.5 text-[10px] text-text-secondary/50 font-mono">
                {props.turn.inputTokens && <span>in: {formatNumberWithSetting(props.turn.inputTokens, props.numberFormat || "system")}</span>}
                {props.turn.inputTokens && props.turn.outputTokens && <span>•</span>}
                {props.turn.outputTokens && <span>out: {formatNumberWithSetting(props.turn.outputTokens, props.numberFormat || "system")}</span>}
              </div>
            </Show>
          </div>
          <div class="w-full bg-accent-light/10 border border-accent/20 p-5 rounded-2xl shadow-sm">
            <AssistantMessageRenderer message={props.turn.assistantMessage} searchQuery={props.searchQuery} />
          </div>
        </div>
      </Show>
    </div>
  );
};

const AssistantMessageRenderer = (props: { message: string; searchQuery?: string }) => {
  const parts = createMemo(() => parseAssistantMessage(props.message));
  
  const groupedParts = createMemo(() => {
    const list = parts();
    const result: Array<{ type: "text"; content: string } | { type: "toolGroup"; tools: MessageToolPart[] }> = [];
    let currentToolGroup: MessageToolPart[] = [];
    
    for (const part of list) {
      if (part.type === "tool") {
        currentToolGroup.push(part);
      } else {
        if (currentToolGroup.length > 0) {
          result.push({ type: "toolGroup", tools: currentToolGroup });
          currentToolGroup = [];
        }
        result.push(part);
      }
    }
    
    if (currentToolGroup.length > 0) {
      result.push({ type: "toolGroup", tools: currentToolGroup });
    }
    
    return result;
  });

  return (
    <div class="space-y-4">
      <For each={groupedParts()}>
        {(part) => {
          if (part.type === "text") {
            return <MarkdownRenderer content={part.content} />;
          } else {
            return <WorkedForBlock tools={part.tools} searchQuery={props.searchQuery} />;
          }
        }}
      </For>
    </div>
  );
};

const WorkedForBlock = (props: { tools: MessageToolPart[]; searchQuery?: string }) => {
  const matchesSearch = createMemo(() => {
    if (!props.searchQuery || props.searchQuery.trim() === "") return false;
    const q = props.searchQuery.toLowerCase();
    return props.tools.some(tool => 
      tool.header.toLowerCase().includes(q) || 
      tool.content.toLowerCase().includes(q)
    );
  });

  const [isExpanded, setIsExpanded] = createSignal(false);

  createEffect(() => {
    if (matchesSearch()) {
      setIsExpanded(true);
    }
  });

  const title = createMemo(() => {
    return `Worked (${props.tools.length} tool execution${props.tools.length > 1 ? 's' : ''})`;
  });

  return (
    <div class="border border-border/40 rounded-2xl overflow-hidden bg-background/40 my-3">
      {/* Level 1: Chevron-toggle header */}
      <button
        onClick={() => setIsExpanded(!isExpanded())}
        class="w-full flex items-center gap-2 px-4 py-2.5 hover:bg-surface/50 transition-all text-xs font-semibold text-text-secondary hover:text-text-primary cursor-pointer select-none"
      >
        <Show when={isExpanded()} fallback={<ChevronRight class="w-3.5 h-3.5" />}>
          <ChevronDown class="w-3.5 h-3.5" />
        </Show>
        <Cpu class="w-3.5 h-3.5 text-accent/80" />
        <span>{title()}</span>
      </button>

      {/* Level 2 & 3 content */}
      <Show when={isExpanded()}>
        <div class="px-4 pb-4 pt-1 space-y-3 relative">
          {/* Thread connector line */}
          <div class="absolute left-6 top-0 bottom-4 w-[1px] bg-border/50 opacity-50" />
          
          <div class="space-y-3 pl-6">
            <For each={props.tools}>
              {(tool) => (
                <ToolOutputBlock tool={tool} searchQuery={props.searchQuery} startExpanded={matchesSearch()} />
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
};

const ToolOutputBlock = (props: { tool: MessageToolPart; searchQuery?: string; startExpanded: boolean }) => {
  const matchesSearch = createMemo(() => {
    if (!props.searchQuery || props.searchQuery.trim() === "") return false;
    const q = props.searchQuery.toLowerCase();
    return props.tool.header.toLowerCase().includes(q) || props.tool.content.toLowerCase().includes(q);
  });

  const [isOpen, setIsOpen] = createSignal(props.startExpanded || matchesSearch());

  createEffect(() => {
    if (matchesSearch()) {
      setIsOpen(true);
    }
  });

  const icon = createMemo(() => {
    const type = props.tool.toolType.toLowerCase();
    if (type.includes("command") || type.includes("shell") || type.includes("terminal")) {
      return <Terminal class="w-3.5 h-3.5 text-accent-hover" />;
    }
    if (type.includes("search") || type.includes("find") || type.includes("grep")) {
      return <Search class="w-3.5 h-3.5 text-sky-400" />;
    }
    return <FileText class="w-3.5 h-3.5 text-text-secondary/70" />;
  });

  return (
    <div class="space-y-1.5">
      {/* Level 2: Tool header */}
      <button
        onClick={() => setIsOpen(!isOpen())}
        class="flex items-center gap-2 hover:text-text-primary text-text-secondary transition-all text-xs font-semibold cursor-pointer select-none text-left"
      >
        <span class="opacity-60">{isOpen() ? "▼" : "▶"}</span>
        {icon()}
        <span class="hover:underline">{props.tool.header}</span>
      </button>

      <Show when={isOpen()}>
        <div class="ml-4 pl-1">
          <pre dir="ltr" class="bg-background border border-border/60 rounded-xl p-3 text-[11px] leading-relaxed overflow-x-auto font-mono text-text-primary/80 max-h-96 scrollbar shadow-inner text-left">
            <code>{props.tool.content}</code>
          </pre>
        </div>
      </Show>
    </div>
  );
};
