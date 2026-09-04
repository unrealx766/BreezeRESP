<script setup lang="ts">
// Monitoring center — command statistics panel.
// Parses INFO commandstats via backend: per-command calls / total time /
// average latency, with Top-N bar view and a sortable/filterable table.
import { ref, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Terminal, RefreshCw, Search } from "lucide-vue-next";
import type { CmdStat, CmdStatNode } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";
import CustomSelect from "@/components/shared/CustomSelect.vue";

const { t } = useI18n();
const connStore = useConnectionStore();

type SortKey = "totalUsec" | "calls" | "avgUsec";

const nodes = ref<CmdStatNode[]>([]);
const loading = ref(false);
const loaded = ref(false);
const error = ref("");
const search = ref("");
const sortKey = ref<SortKey>("totalUsec");
const TOP_N = 10;

watch(() => connStore.activeConnectionId, () => {
  nodes.value = [];
  loaded.value = false;
  error.value = "";
  if (connStore.activeConnectionId) load();
});

// Load on first mount (the panel mounts lazily when its tab is opened)
if (connStore.activeConnectionId) load();

async function load() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  loading.value = true;
  error.value = "";
  try {
    nodes.value = await tauriApi.serverAdmin.getCommandStats(connId);
    loaded.value = true;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    toast.error(error.value);
  } finally {
    loading.value = false;
  }
}

/** Aggregated stats (cluster mode already merges server-side). */
const stats = computed<CmdStat[]>(() => {
  const out: CmdStat[] = [];
  for (const node of nodes.value) {
    for (const s of node.stats) {
      const existing = out.find((x) => x.cmd === s.cmd);
      if (existing) {
        existing.calls += s.calls;
        existing.totalUsec += s.totalUsec;
        existing.rejectedCalls += s.rejectedCalls;
      } else {
        out.push({ ...s });
      }
    }
  }
  return out;
});

function avgUsec(s: CmdStat): number {
  return s.calls > 0 ? s.totalUsec / s.calls : 0;
}

function sortValue(s: CmdStat, key: SortKey): number {
  if (key === "calls") return s.calls;
  if (key === "avgUsec") return avgUsec(s);
  return s.totalUsec;
}

const filteredStats = computed(() => {
  const q = search.value.trim().toLowerCase();
  let list = stats.value;
  if (q) list = list.filter((s) => s.cmd.toLowerCase().includes(q));
  return [...list].sort((a, b) => sortValue(b, sortKey.value) - sortValue(a, sortKey.value));
});

const totalCalls = computed(() => stats.value.reduce((sum, s) => sum + s.calls, 0));
const totalUsec = computed(() => stats.value.reduce((sum, s) => sum + s.totalUsec, 0));

const topStats = computed(() => {
  const list = [...stats.value].sort((a, b) => sortValue(b, sortKey.value) - sortValue(a, sortKey.value));
  return list.slice(0, TOP_N);
});

const maxMetric = computed(() =>
  topStats.value.reduce((max, s) => Math.max(max, sortValue(s, sortKey.value)), 0)
);

function barWidth(s: CmdStat): string {
  const max = maxMetric.value;
  if (max <= 0) return "0%";
  return `${Math.max((sortValue(s, sortKey.value) / max) * 100, 1)}%`;
}

/** Microseconds → human readable duration. */
function formatUsec(usec: number): string {
  if (usec < 1000) return `${Math.round(usec)} µs`;
  if (usec < 1_000_000) return `${(usec / 1000).toFixed(2)} ms`;
  if (usec < 60_000_000_000) return `${(usec / 1_000_000).toFixed(2)} s`;
  return `${(usec / 3_600_000_000).toFixed(2)} h`;
}

function formatCalls(n: number): string {
  if (n < 10_000) return n.toLocaleString();
  if (n < 10_000_000) return `${(n / 1000).toFixed(1)}K`;
  if (n < 10_000_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  return `${(n / 1_000_000_000).toFixed(2)}B`;
}

function callPercent(s: CmdStat): string {
  if (totalCalls.value <= 0) return "0%";
  return `${((s.calls / totalCalls.value) * 100).toFixed(1)}%`;
}

const sortOptions = computed(() => [
  { value: "totalUsec" as const, label: t("cmdstats.sortByTime") },
  { value: "calls" as const, label: t("cmdstats.sortByCalls") },
  { value: "avgUsec" as const, label: t("cmdstats.sortByAvg") },
]);
</script>

<template>
  <div class="h-full flex flex-col min-w-[600px]">
    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Terminal :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("monitor.noConnection") }}</p>
    </div>

    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center gap-2 mb-3 shrink-0 flex-wrap">
        <button
          @click="load"
          :disabled="loading"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
        >
          <RefreshCw :size="13" :class="loading ? 'animate-spin' : ''" />
          {{ t("common.refresh") }}
        </button>
        <div class="relative flex-1 min-w-[160px] max-w-xs">
          <Search :size="13" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            v-model="search"
            type="text"
            :placeholder="t('cmdstats.searchPlaceholder')"
            class="w-full h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
          />
        </div>
        <div class="ml-auto">
          <CustomSelect v-model="sortKey" :options="sortOptions" />
        </div>
      </div>

      <!-- Loading / error / empty states -->
      <div v-if="loading && !loaded" class="flex-1 flex items-center justify-center text-text-muted">
        <RefreshCw :size="24" class="animate-spin" />
      </div>
      <div v-else-if="error" class="px-3 py-2 rounded-lg border border-danger/30 bg-danger/10 text-xs text-danger">
        {{ error }}
      </div>
      <div v-else-if="stats.length === 0" class="flex-1 flex flex-col items-center justify-center text-text-muted">
        <Terminal :size="32" :stroke-width="1.5" class="mb-3 opacity-30" />
        <p class="text-xs">{{ t("cmdstats.empty") }}</p>
      </div>

      <div v-else class="flex-1 min-h-0 overflow-y-auto space-y-4">
        <!-- Summary cards -->
        <div class="grid grid-cols-3 gap-3">
          <div class="rounded-lg border border-border bg-bg-secondary/40 px-4 py-3">
            <p class="text-[11px] text-text-muted">{{ t("cmdstats.totalCommands") }}</p>
            <p class="text-lg font-semibold text-text-primary mt-0.5">{{ stats.length }}</p>
          </div>
          <div class="rounded-lg border border-border bg-bg-secondary/40 px-4 py-3">
            <p class="text-[11px] text-text-muted">{{ t("cmdstats.totalCalls") }}</p>
            <p class="text-lg font-semibold text-text-primary mt-0.5">{{ formatCalls(totalCalls) }}</p>
          </div>
          <div class="rounded-lg border border-border bg-bg-secondary/40 px-4 py-3">
            <p class="text-[11px] text-text-muted">{{ t("cmdstats.totalTime") }}</p>
            <p class="text-lg font-semibold text-text-primary mt-0.5">{{ formatUsec(totalUsec) }}</p>
          </div>
        </div>

        <!-- Top-N bar chart -->
        <div class="rounded-lg border border-border">
          <div class="px-3 py-2 border-b border-border bg-bg-secondary/50">
            <h4 class="text-[11px] font-semibold text-text-secondary uppercase tracking-wider">
              {{ t("cmdstats.topN", { n: TOP_N }) }}
            </h4>
          </div>
          <div class="p-3 space-y-2">
            <div v-for="s in topStats" :key="s.cmd" class="grid grid-cols-[110px_1fr_130px] gap-2 items-center">
              <span class="text-[11px] font-mono text-redis truncate" :title="s.cmd">{{ s.cmd }}</span>
              <div class="h-4 rounded bg-bg-secondary overflow-hidden">
                <div class="h-full rounded bg-redis/60 transition-all" :style="{ width: barWidth(s) }"></div>
              </div>
              <span class="text-[11px] font-mono text-text-secondary text-right truncate">
                {{ sortKey === "calls" ? formatCalls(s.calls) : formatUsec(sortValue(s, sortKey)) }}
              </span>
            </div>
          </div>
        </div>

        <!-- Full table -->
        <div class="rounded-lg border border-border">
          <div class="sticky top-0 z-10 grid grid-cols-[1fr_130px_120px_120px_100px] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
            <span>{{ t("cmdstats.colCmd") }}</span>
            <span class="text-right">{{ t("cmdstats.colCalls") }}</span>
            <span class="text-right">{{ t("cmdstats.colTime") }}</span>
            <span class="text-right">{{ t("cmdstats.colAvg") }}</span>
            <span class="text-right">{{ t("cmdstats.colRejected") }}</span>
          </div>
          <div class="divide-y divide-border/40">
            <div
              v-for="s in filteredStats"
              :key="s.cmd"
              class="grid grid-cols-[1fr_130px_120px_120px_100px] gap-2 px-3 py-1.5 hover:bg-bg-secondary/40 transition-colors items-center"
            >
              <span class="text-[11px] font-mono text-redis truncate" :title="s.cmd">{{ s.cmd }}</span>
              <span class="text-[11px] font-mono text-text-primary text-right">
                {{ s.calls.toLocaleString() }}
                <span class="text-text-muted">({{ callPercent(s) }})</span>
              </span>
              <span class="text-[11px] font-mono text-text-secondary text-right">{{ formatUsec(s.totalUsec) }}</span>
              <span class="text-[11px] font-mono text-text-secondary text-right">{{ formatUsec(avgUsec(s)) }}</span>
              <span class="text-[11px] font-mono text-right" :class="s.rejectedCalls > 0 ? 'text-danger' : 'text-text-muted'">
                {{ s.rejectedCalls }}
              </span>
            </div>
            <div v-if="filteredStats.length === 0" class="px-3 py-6 text-center text-xs text-text-muted">
              {{ t("cmdstats.noMatch") }}
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
