<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { Activity, RefreshCw, Search, Copy, Database, Zap, Timer } from "lucide-vue-next";
import { useSlowlogStore } from "@/stores/slowlogStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";

const { t } = useI18n();
const slowlogStore = useSlowlogStore();
const connStore = useConnectionStore();

const searchQuery = ref("");
const fetchCount = ref(128);
const durationFilter = ref<"all" | "10ms" | "100ms" | "1s">("all");

// Fetch on mount and when connection changes
onMounted(() => {
  if (connStore.activeConnectionId) {
    slowlogStore.fetchSlowlog(fetchCount.value);
  }
});

watch(() => connStore.activeConnectionId, (newId) => {
  slowlogStore.resetSlowlog();
  searchQuery.value = "";
  durationFilter.value = "all";
  if (newId) {
    slowlogStore.fetchSlowlog(fetchCount.value);
  }
});

// Cleanup auto-refresh on unmount
onBeforeUnmount(() => {
  slowlogStore.stopAutoRefresh();
});

const filteredEntries = computed(() => {
  let items = slowlogStore.entries;

  // Duration filter
  if (durationFilter.value === "10ms") {
    items = items.filter((e) => e.durationUs >= 10000);
  } else if (durationFilter.value === "100ms") {
    items = items.filter((e) => e.durationUs >= 100000);
  } else if (durationFilter.value === "1s") {
    items = items.filter((e) => e.durationUs >= 1000000);
  }

  // Search filter
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.trim().toLowerCase();
    items = items.filter((e) => e.command.toLowerCase().includes(q));
  }

  return items;
});

/** Format duration in microseconds to human-readable string */
function formatDuration(us: number): string {
  if (us < 1000) return `${us}μs`;
  if (us < 1000000) return `${(us / 1000).toFixed(1)}ms`;
  return `${(us / 1000000).toFixed(2)}s`;
}

/** Get color class based on duration */
function durationColor(us: number): string {
  if (us >= 1000000) return "text-danger"; // >1s
  if (us >= 100000) return "text-warning"; // >100ms
  if (us >= 10000) return "text-amber-400"; // >10ms
  return "text-success";
}

/** Format timestamp to readable time */
function formatTime(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** Extract the Redis command verb (first word) */
function cmdVerb(cmd: string): string {
  return cmd.split(/\s/)[0] ?? cmd;
}

/** Extract the args part */
function cmdArgs(cmd: string): string {
  const idx = cmd.indexOf(" ");
  return idx >= 0 ? cmd.slice(idx + 1) : "";
}

/** Format threshold for display */
function formatThreshold(us: number): string {
  if (us < 0) return t("slowlog.disabled");
  return formatDuration(us);
}

async function copyCommand(cmd: string) {
  try {
    await navigator.clipboard.writeText(cmd);
    toast.success(t("common.copied"));
  } catch {
    toast.error(t("common.error"));
  }
}

function handleRefresh() {
  slowlogStore.fetchSlowlog(fetchCount.value);
}

function toggleAutoRefresh() {
  if (slowlogStore.autoRefresh) {
    slowlogStore.stopAutoRefresh();
  } else {
    slowlogStore.startAutoRefresh(3000, fetchCount.value);
  }
}

function handleCountChange() {
  slowlogStore.fetchSlowlog(fetchCount.value);
  if (slowlogStore.autoRefresh) {
    slowlogStore.stopAutoRefresh();
    slowlogStore.startAutoRefresh(3000, fetchCount.value);
  }
}
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-auto min-w-[600px]">
    <!-- Header -->
    <div class="flex items-start justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Activity :size="20" class="text-redis" />
          {{ t("slowlog.title") }}
        </h2>
        <p v-if="filteredEntries.length > 0" class="text-sm text-text-muted mt-1">
          {{ t("slowlog.totalEntries", { count: filteredEntries.length }) }}
        </p>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <!-- Count selector -->
        <select
          v-model.number="fetchCount"
          @change="handleCountChange"
          class="h-7 px-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary focus:outline-none focus:border-redis/50 transition-colors"
        >
          <option :value="20">{{ t("slowlog.count20") }}</option>
          <option :value="50">{{ t("slowlog.count50") }}</option>
          <option :value="128">{{ t("slowlog.count128") }}</option>
          <option :value="200">{{ t("slowlog.count200") }}</option>
        </select>
        <!-- Duration filter -->
        <select
          v-model="durationFilter"
          class="h-7 px-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary focus:outline-none focus:border-redis/50 transition-colors"
        >
          <option value="all">{{ t("slowlog.filterAll") }}</option>
          <option value="10ms">{{ t("slowlog.filter10ms") }}</option>
          <option value="100ms">{{ t("slowlog.filter100ms") }}</option>
          <option value="1s">{{ t("slowlog.filter1s") }}</option>
        </select>
        <!-- Search -->
        <div class="relative">
          <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            v-model="searchQuery"
            type="text"
            :placeholder="t('slowlog.searchPlaceholder')"
            class="w-40 h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
          />
        </div>
        <!-- Auto-refresh toggle -->
        <button
          @click="toggleAutoRefresh"
          class="h-7 px-2.5 text-xs rounded-lg border transition-colors flex items-center gap-1"
          :class="slowlogStore.autoRefresh
            ? 'bg-redis/10 text-redis border-redis/30'
            : 'border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary'"
        >
          <RefreshCw :size="13" :class="slowlogStore.autoRefresh ? 'animate-spin' : ''" />
          {{ t("slowlog.autoRefresh") }}
        </button>
        <!-- Refresh button -->
        <button
          @click="handleRefresh"
          :disabled="slowlogStore.loading"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
        >
          <RefreshCw :size="13" :class="slowlogStore.loading ? 'animate-spin' : ''" />
          {{ t("slowlog.refresh") }}
        </button>
      </div>
    </div>

    <!-- Summary cards -->
    <div v-if="connStore.activeConnectionId" class="grid grid-cols-3 gap-3 mb-4 shrink-0">
      <div class="flex items-center gap-2 p-3 rounded-lg border border-border bg-bg-secondary/50">
        <Database :size="16" class="text-redis shrink-0" />
        <div class="min-w-0">
          <p class="text-[11px] text-text-muted">{{ t("slowlog.totalLen") }}</p>
          <p class="text-sm font-semibold text-text-primary">{{ slowlogStore.totalLen.toLocaleString() }}</p>
        </div>
      </div>
      <div class="flex items-center gap-2 p-3 rounded-lg border border-border bg-bg-secondary/50">
        <Timer :size="16" class="text-redis shrink-0" />
        <div class="min-w-0">
          <p class="text-[11px] text-text-muted">{{ t("slowlog.threshold") }}</p>
          <p class="text-sm font-semibold text-text-primary">{{ formatThreshold(slowlogStore.slowlogLogSlowerThan) }}</p>
        </div>
      </div>
      <div class="flex items-center gap-2 p-3 rounded-lg border border-border bg-bg-secondary/50">
        <Zap :size="16" class="text-redis shrink-0" />
        <div class="min-w-0">
          <p class="text-[11px] text-text-muted">{{ t("slowlog.fetchedCount") }}</p>
          <p class="text-sm font-semibold text-text-primary">{{ slowlogStore.entries.length }}</p>
        </div>
      </div>
    </div>

    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Activity :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("slowlog.noConnection") }}</p>
    </div>

    <!-- Empty state -->
    <div
      v-else-if="filteredEntries.length === 0 && !slowlogStore.loading"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Activity :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("slowlog.noEntries") }}</p>
    </div>

    <!-- Loading state -->
    <div
      v-else-if="slowlogStore.loading && slowlogStore.entries.length === 0"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <RefreshCw :size="32" class="animate-spin mb-4" />
      <p class="text-sm">{{ t("common.loading") }}</p>
    </div>

    <!-- Content -->
    <template v-else>
      <div class="flex-1 overflow-y-auto">
        <!-- Table header -->
        <div class="sticky top-0 z-10 grid grid-cols-[60px_140px_100px_1fr_120px_40px] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
          <span>{{ t("slowlog.colId") }}</span>
          <span>{{ t("slowlog.colTime") }}</span>
          <span>{{ t("slowlog.colDuration") }}</span>
          <span>{{ t("slowlog.colCommand") }}</span>
          <span>{{ t("slowlog.colClient") }}</span>
          <span></span>
        </div>

        <!-- Entries -->
        <div class="divide-y divide-border/50">
          <div
            v-for="entry in filteredEntries"
            :key="entry.id"
            class="group grid grid-cols-[60px_140px_100px_1fr_120px_40px] gap-2 px-3 py-2 hover:bg-bg-secondary/60 transition-colors items-center"
          >
            <!-- ID -->
            <span class="text-xs font-mono text-text-muted">#{{ entry.id }}</span>

            <!-- Time -->
            <span class="text-xs font-mono text-text-secondary" :title="new Date(entry.timestamp * 1000).toLocaleString()">
              {{ formatTime(entry.timestamp) }}
            </span>

            <!-- Duration -->
            <span class="text-xs font-mono font-semibold" :class="durationColor(entry.durationUs)">
              {{ formatDuration(entry.durationUs) }}
            </span>

            <!-- Command -->
            <div class="flex items-center min-w-0" :title="entry.command">
              <code class="text-xs font-mono truncate">
                <span class="text-redis font-semibold">{{ cmdVerb(entry.command) }}</span>
                <span v-if="cmdArgs(entry.command)" class="text-text-secondary ml-1">{{ cmdArgs(entry.command) }}</span>
              </code>
            </div>

            <!-- Client info -->
            <span class="text-[11px] font-mono text-text-muted truncate" :title="entry.clientAddr || ''">
              {{ entry.clientAddr || '-' }}
            </span>

            <!-- Copy button -->
            <button
              @click="copyCommand(entry.command)"
              class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded hover:bg-bg-hover"
              :title="t('slowlog.copyCommand')"
            >
              <Copy :size="12" class="text-text-muted" />
            </button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
