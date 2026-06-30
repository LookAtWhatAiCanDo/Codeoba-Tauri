import { createSignal, createMemo, For, Show } from "solid-js";
import { useI18n } from "../i18n/i18n";
import { formatNumberWithSetting } from "../utils/format";
import { 
  Folder, 
  MessageSquare, 
  Clock, 
  Cpu, 
  Settings, 
  RefreshCw,
  Bolt,
  Layers
} from "lucide-solid";
import { 
  getSessionComputeTimeMs, 
  formatSpeed, 
  formatDuration 
} from "./Sidebar";

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
}

interface DashboardProps {
  sessions: Session[];
  numberFormat?: string;
}

interface ModelItemStats {
  modelName: string;
  turnCount: number;
  totalTokens: number;
  computeTimeMs: number;
  speedTps: number;
}

type SortDimension = "turns" | "tokens" | "speed" | "duration" | "name";

export const Dashboard = (props: DashboardProps) => {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = createSignal<"global" | "groups">("global");
  const [sortBy, setSortBy] = createSignal<SortDimension>("turns");
  const [sortAscending, setSortAscending] = createSignal(false);

  // Compute stats based on the passed sessions (which will be the visible/filtered ones)
  const stats = createMemo(() => {
    const list = props.sessions;
    const totalConversations = list.length;
    let totalTurns = 0;
    let totalDurationMs = 0;
    let totalElapsedMs = 0;
    let totalCompactions = 0;
    let totalCompactionTimeMs = 0;
    let promptTokens = 0;
    let responseTokens = 0;

    // Model aggregation
    const modelMap = new Map<string, {
      turnCount: number;
      promptChars: number;
      responseChars: number;
      computeTimeMs: number;
      totalTokens: number;
    }>();

    for (const session of list) {
      totalTurns += session.turns.length;
      totalDurationMs += getSessionComputeTimeMs(session);
      totalElapsedMs += Math.max(0, session.updatedAt - session.timestamp);

      for (const turn of session.turns) {
        const extra = turn.extraData;
        
        // Count compactions
        if (extra && extra["isCompaction"] === "true") {
          totalCompactions++;
        }
        if (extra && extra["compactionTimeMs"]) {
          const ms = parseInt(extra["compactionTimeMs"], 10);
          if (!isNaN(ms)) totalCompactionTimeMs += ms;
        }

        // Model stats
        const modelName = (extra && extra["model"]) || t("dashboard.unknownModel");
        let mStats = modelMap.get(modelName);
        if (!mStats) {
          mStats = { turnCount: 0, promptChars: 0, responseChars: 0, computeTimeMs: 0, totalTokens: 0 };
          modelMap.set(modelName, mStats);
        }
        mStats.turnCount++;
        
        const turnUserLen = (turn.userMessage || "").length;
        const turnAssistantLen = (turn.assistantMessage || "").length;
        
        mStats.promptChars += turnUserLen;
        mStats.responseChars += turnAssistantLen;
        
        let turnInputTokens = 0;
        let turnOutputTokens = 0;
        
        if (turn.inputTokens !== undefined && turn.inputTokens !== null) {
          turnInputTokens = turn.inputTokens;
        } else {
          turnInputTokens = Math.round((turnUserLen + 3) / 4);
        }
        
        if (turn.outputTokens !== undefined && turn.outputTokens !== null) {
          turnOutputTokens = turn.outputTokens;
        } else {
          turnOutputTokens = Math.round((turnAssistantLen + 3) / 4);
        }
        
        mStats.totalTokens += turnInputTokens + turnOutputTokens;
        promptTokens += turnInputTokens;
        responseTokens += turnOutputTokens;

        const compMsStr = extra ? extra["computeTimeMs"] : null;
        const compMs = compMsStr ? parseInt(compMsStr, 10) : null;
        if (compMs !== null && !isNaN(compMs) && compMs > 0) {
          mStats.computeTimeMs += Math.min(900000, compMs);
        } else if (turn.assistantMessage && turn.assistantMessage.length > 0) {
          const estMs = Math.round((turn.assistantMessage.length / 120.0) * 1000.0);
          mStats.computeTimeMs += Math.max(2000, Math.min(60000, estMs));
        }
      }
    }

    const totalEstTokens = promptTokens + responseTokens;
    const avgTurns = totalConversations > 0 ? totalTurns / totalConversations : 0;
    const avgDurationMs = totalConversations > 0 ? totalElapsedMs / totalConversations : 0;
    const avgSpeedText = formatSpeed(totalEstTokens, totalDurationMs);

    // Format model list
    const modelStatsList: ModelItemStats[] = [];
    modelMap.forEach((val, key) => {
      // If we didn't populate totalTokens (due to character estimation fallback)
      let finalTokens = val.totalTokens;
      if (finalTokens === 0) {
        finalTokens = Math.round((val.promptChars + val.responseChars) / 4);
      }
      
      const speedTps = val.computeTimeMs > 0 ? (finalTokens * 1000.0) / val.computeTimeMs : 0;
      modelStatsList.push({
        modelName: key,
        turnCount: val.turnCount,
        totalTokens: finalTokens,
        computeTimeMs: val.computeTimeMs,
        speedTps
      });
    });

    // Group aggregation
    const groupMap = new Map<string, number>();
    for (const session of list) {
      const source = session.sourceId;
      groupMap.set(source, (groupMap.get(source) || 0) + 1);
    }
    const sourceGroups = Array.from(groupMap.entries()).sort((a, b) => b[1] - a[1]);

    return {
      totalConversations,
      totalTurns,
      promptTokens,
      responseTokens,
      totalEstTokens,
      avgTurns,
      totalDurationMs,
      avgDurationMs,
      avgSpeedText,
      totalCompactions,
      totalCompactionTimeMs,
      modelStatsList,
      sourceGroups
    };
  });

  // Sorted model list
  const sortedModelStats = createMemo(() => {
    const list = [...stats().modelStatsList];
    const dim = sortBy();
    const asc = sortAscending();

    list.sort((a, b) => {
      let valA: any = 0;
      let valB: any = 0;
      if (dim === "turns") {
        valA = a.turnCount;
        valB = b.turnCount;
      } else if (dim === "tokens") {
        valA = a.totalTokens;
        valB = b.totalTokens;
      } else if (dim === "speed") {
        valA = a.speedTps;
        valB = b.speedTps;
      } else if (dim === "duration") {
        valA = a.computeTimeMs;
        valB = b.computeTimeMs;
      } else if (dim === "name") {
        return asc ? a.modelName.localeCompare(b.modelName) : b.modelName.localeCompare(a.modelName);
      }

      return asc ? valA - valB : valB - valA;
    });
    return list;
  });

  const toggleSort = (dim: SortDimension) => {
    if (sortBy() === dim) {
      setSortAscending(!sortAscending());
    } else {
      setSortBy(dim);
      setSortAscending(false);
    }
  };

  const formatNumber = (num: number) => {
    return formatNumberWithSetting(num, props.numberFormat || "system");
  };

  return (
    <div class="flex-grow h-full flex flex-col bg-background/95 min-w-0 overflow-y-auto px-8 pt-6 pb-6 space-y-6">
      {/* Overview Tabs Navigation */}
      <div class="flex bg-surface p-1 rounded-xl border border-border/60 max-w-sm flex-shrink-0">
        <button
          onClick={() => setActiveTab("global")}
          class={`flex-1 text-center py-2 text-xs font-semibold rounded-lg transition-all capitalize cursor-pointer ${
            activeTab() === "global" 
              ? "bg-background text-accent border border-border/80 shadow-sm" 
              : "text-text-secondary hover:text-text-primary"
          }`}
        >
          {t("dashboard.globalStats")}
        </button>
        <button
          onClick={() => setActiveTab("groups")}
          class={`flex-1 text-center py-2 text-xs font-semibold rounded-lg transition-all capitalize cursor-pointer ${
            activeTab() === "groups" 
              ? "bg-background text-accent border border-border/80 shadow-sm" 
              : "text-text-secondary hover:text-text-primary"
          }`}
        >
          {t("dashboard.adapterGroups")}
        </button>
      </div>

      <Show 
        when={activeTab() === "global"} 
        fallback={
          /* Groups Dashboard View */
          <div class="space-y-4 max-w-4xl">
            <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary">
              {t("dashboard.adapterGroups")}
            </h3>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <For each={stats().sourceGroups}>
                {([source, count]) => (
                  <div class="bg-surface border border-border/50 rounded-2xl p-5 flex items-center justify-between shadow-sm">
                    <div class="flex items-center gap-3">
                      <div class="p-2.5 bg-accent/10 border border-accent/25 rounded-xl text-accent">
                        <Layers class="w-5 h-5" />
                      </div>
                      <div>
                        <h4 class="text-sm font-bold text-text-primary capitalize">{source}</h4>
                        <span class="text-xs text-text-secondary">{t("settings.sources.desc")}</span>
                      </div>
                    </div>
                    <div class="text-right">
                      <div class="text-[20px] font-bold text-text-primary">{count}</div>
                      <span class="text-xs text-text-secondary">{t("sidebar.title").toLowerCase()}</span>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        }
      >
        {/* Global Stats Grid View */}
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 max-w-5xl flex-shrink-0">
          <StatCard
            title={t("dashboard.totalConversations")}
            value={formatNumber(stats().totalConversations)}
            subtitle={t("detailPane.selectSession")}
            icon={<Folder class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.totalTurns")}
            value={formatNumber(stats().totalTurns)}
            subtitle={`${t("dashboard.avgTurns")}: ${stats().avgTurns.toFixed(1)}`}
            icon={<MessageSquare class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.avgSpeed")}
            value={stats().avgSpeedText}
            subtitle={t("settings.general.logModeDesc")}
            icon={<Bolt class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.totalEstTokens")}
            value={formatNumber(stats().totalEstTokens)}
            subtitle={`${formatNumber(stats().promptTokens)} in / ${formatNumber(stats().responseTokens)} out`}
            icon={<Cpu class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.totalCompactionTime")}
            value={formatDuration(stats().totalDurationMs)}
            subtitle={t("settings.general.cacheDesc")}
            icon={<Clock class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.duration")}
            value={formatDuration(stats().avgDurationMs)}
            subtitle={t("settings.general.logModeDesc")}
            icon={<Clock class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.totalCompactions")}
            value={formatNumber(stats().totalCompactions)}
            subtitle={t("settings.general.logMode")}
            icon={<Settings class="w-5 h-5" />}
          />
          <StatCard
            title={t("dashboard.totalCompactionTime")}
            value={formatDuration(stats().totalCompactionTimeMs)}
            subtitle={stats().totalCompactions > 0 
              ? `Avg: ${((stats().totalCompactionTimeMs / stats().totalCompactions) / 1000).toFixed(2)}s`
              : "Avg: 0s"
            }
            icon={<RefreshCw class="w-5 h-5" />}
          />
        </div>

        {/* Model Performance List */}
        <div class="space-y-4 max-w-5xl">
          <div class="flex items-center justify-between border-b border-border/40 pb-2 flex-shrink-0">
            <h3 class="text-sm font-bold uppercase tracking-wider text-text-secondary">
              {t("dashboard.topModels")}
            </h3>
            
            {/* Sorting controls */}
            <div class="flex items-center gap-2">
              <span class="text-xs text-text-secondary/70">{t("dashboard.sort")}:</span>
              <For each={["turns", "tokens", "speed", "duration", "name"] as const}>
                {(dim) => (
                  <button
                    onClick={() => toggleSort(dim)}
                    class={`px-2.5 py-1 rounded-lg border text-xs font-semibold capitalize cursor-pointer transition-all ${
                      sortBy() === dim 
                        ? "bg-accent/10 border-accent/40 text-accent font-bold" 
                        : "bg-surface border-border/40 text-text-secondary hover:text-text-primary"
                    }`}
                  >
                    {t(`dashboard.${dim}`)}
                    <Show when={sortBy() === dim}>
                      <span class="ml-1 text-[10px]">{sortAscending() ? "▲" : "▼"}</span>
                    </Show>
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="space-y-3.5">
            <For 
              each={sortedModelStats()}
              fallback={
                <div class="p-6 text-center text-text-secondary text-sm">
                  {t("detailPane.noPermissions")}
                </div>
              }
            >
              {(m) => (
                <div class="bg-surface/40 border border-border/40 rounded-2xl p-5 hover:bg-surface/60 transition-all shadow-sm">
                  <div class="flex items-center justify-between mb-3">
                    <span class="text-sm font-bold text-text-primary">{m.modelName}</span>
                    <div class="flex items-center gap-1 text-xs text-accent font-semibold bg-accent-light/10 border border-accent/25 px-2 py-0.5 rounded-lg">
                      <Bolt class="w-3.5 h-3.5" />
                      <span>{m.speedTps.toFixed(1)} t/s</span>
                    </div>
                  </div>

                  <div class="grid grid-cols-3 gap-6 text-xs text-text-secondary">
                    <div>
                      <div class="text-[10px] font-semibold uppercase tracking-wider text-text-secondary/50 mb-1">
                        {t("dashboard.tokens")}
                      </div>
                      <div class="text-sm font-bold text-text-primary">{formatNumber(m.totalTokens)}</div>
                    </div>
                    <div>
                      <div class="text-[10px] font-semibold uppercase tracking-wider text-text-secondary/50 mb-1">
                        {t("dashboard.turns")}
                      </div>
                      <div class="text-sm font-bold text-text-primary">
                        {m.turnCount} {t("dashboard.turns").toLowerCase()}
                        <span class="text-xs text-text-secondary/60 font-normal ml-1.5">
                          ({stats().totalTurns > 0 ? ((m.turnCount / stats().totalTurns) * 100).toFixed(1) : 0}%)
                        </span>
                      </div>
                    </div>
                    <div>
                      <div class="text-[10px] font-semibold uppercase tracking-wider text-text-secondary/50 mb-1">
                        {t("dashboard.duration")}
                      </div>
                      <div class="text-sm font-bold text-text-primary">
                        {formatDuration(m.computeTimeMs)}
                        <span class="text-xs text-text-secondary/60 font-normal ml-1.5">
                          ({stats().totalDurationMs > 0 ? ((m.computeTimeMs / stats().totalDurationMs) * 100).toFixed(1) : 0}%)
                        </span>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
};

interface StatCardProps {
  title: string;
  value: string;
  subtitle: string;
  icon: any;
}

const StatCard = (props: StatCardProps) => {
  return (
    <div class="bg-surface/50 border border-border/50 p-5 rounded-2xl flex items-center gap-4 shadow-sm hover:border-border transition-colors">
      <div class="p-3 bg-accent-light/10 border border-accent/10 rounded-xl text-accent flex-shrink-0">
        {props.icon}
      </div>
      <div class="min-w-0">
        <div class="text-xs font-semibold text-text-secondary/80 uppercase tracking-wider mb-0.5">
          {props.title}
        </div>
        <div class="text-[22px] font-extrabold text-text-primary leading-tight mb-1">
          {props.value}
        </div>
        <div class="text-[11.5px] text-text-secondary truncate leading-none">
          {props.subtitle}
        </div>
      </div>
    </div>
  );
};
