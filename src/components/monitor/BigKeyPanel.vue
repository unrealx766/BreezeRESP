<script setup lang="ts">
// Monitoring center — big key / memory analysis panel.
// Iterates SCAN batches (cluster-aware via backend), enriches each key with
// TYPE / PTTL / MEMORY USAGE / element count, then ranks by memory.
import { ref, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Play, Square, RefreshCw, Search, MemoryStick, Stethoscope, Flame, ArrowUpDown, ChevronDown, ChevronRight, CheckCircle, Info } from "lucide-vue-next";
import type { BigKeyEntry, MemoryStatItem, MemoryDoctorEntry } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";

const { t } = useI18n();
const connStore = useConnectionStore();

// Hard cap on entries collected in one analysis run
const MAX_COLLECT = 20000;
const BATCH_COUNT = 500;

const pattern = ref("*");
const scanning = ref(false);
const scannedKeys = ref(0);
const entries = ref<BigKeyEntry[]>([]);
const error = ref("");

// Sorting
type SortKey = "memoryBytes" | "elementCount" | "ttl" | "key";
const sortKey = ref<SortKey>("memoryBytes");
const sortDesc = ref(true);

function toggleSort(key: SortKey) {
  if (sortKey.value === key) sortDesc.value = !sortDesc.value;
  else {
    sortKey.value = key;
    sortDesc.value = true;
  }
}

const sortedEntries = computed(() => {
  const list = [...entries.value];
  const dir = sortDesc.value ? -1 : 1;
  list.sort((a, b) => {
    if (sortKey.value === "key") return a.key.localeCompare(b.key) * dir;
    return (a[sortKey.value] - b[sortKey.value]) * dir;
  });
  return list;
});

/** Run a full scan loop until the cursor wraps around or the cap is hit. */
async function startScan() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  scanning.value = true;
  error.value = "";
  entries.value = [];
  scannedKeys.value = 0;

  try {
    let cursor = 0;
    let rounds = 0;
    do {
      if (!scanning.value) break; // user stopped
      rounds++;
      if (rounds > 200000) {
        error.value = t("bigkey.tooManyRounds");
        break;
      }
      const batch = await tauriApi.bigkey.scanBigKeys(connId, pattern.value || "*", cursor, BATCH_COUNT);
      entries.value.push(...batch.entries);
      scannedKeys.value += batch.entries.length;
      if (entries.value.length >= MAX_COLLECT) {
        entries.value = entries.value.slice(0, MAX_COLLECT);
        error.value = t("bigkey.collectCapped", { max: MAX_COLLECT });
        break;
      }
      cursor = batch.nextCursor;
      // Yield to keep the UI responsive between batches
      await new Promise((r) => setTimeout(r, 0));
    } while (cursor !== 0);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    toast.error(error.value);
  } finally {
    scanning.value = false;
  }
}

function stopScan() {
  scanning.value = false;
}

function resetScan() {
  scanning.value = false;
  entries.value = [];
  scannedKeys.value = 0;
  error.value = "";
  doctorEntries.value = [];
  stats.value = [];
}

// Reset results when the active connection changes
watch(() => connStore.activeConnectionId, resetScan);

// ---- MEMORY STATS / DOCTOR ----
const stats = ref<MemoryStatItem[]>([]);
const statsFilter = ref("");
const loadingStats = ref(false);

const filteredStats = computed(() => {
  let items = stats.value.filter((s) => s.value !== 0);
  const q = statsFilter.value.trim().toLowerCase();
  if (q) items = items.filter((s) => s.name.toLowerCase().includes(q));
  return items.sort((a, b) => Math.abs(b.value) - Math.abs(a.value));
});

async function loadMemoryStats() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  loadingStats.value = true;
  try {
    stats.value = await tauriApi.bigkey.memoryStats(connId);
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    loadingStats.value = false;
  }
}

const doctorEntries = ref<MemoryDoctorEntry[]>([]);
const loadingDoctor = ref(false);
const doctorExpanded = ref<Set<string>>(new Set());

function toggleDoctorNode(addr: string) {
  const s = new Set(doctorExpanded.value);
  if (s.has(addr)) s.delete(addr);
  else s.add(addr);
  doctorExpanded.value = s;
}

/** Whether a node's advice indicates no problems */
function isHealthy(advice: string): boolean {
  return !advice || advice.trim().startsWith("Sam, I have no memory problems");
}

async function loadDoctor() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  loadingDoctor.value = true;
  try {
    doctorEntries.value = await tauriApi.bigkey.memoryDoctor(connId);
    // Auto-expand all nodes that have problems
    const expand = new Set<string>();
    for (const e of doctorEntries.value) {
      if (!isHealthy(e.advice)) expand.add(e.addr);
    }
    doctorExpanded.value = expand;
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    loadingDoctor.value = false;
  }
}

// ---- Hot keys (OBJECT FREQ, LFU policies only) ----
const freqLoading = ref(false);
const freqMap = ref<Record<string, number | null>>({});
const freqUnsupported = ref(false);

async function detectHotKeys() {
  const connId = connStore.activeConnectionId;
  if (!connId || entries.value.length === 0) return;
  freqLoading.value = true;
  freqUnsupported.value = false;
  try {
    const topKeys = sortedEntries.value.slice(0, 100).map((e) => e.key);
    const result = await tauriApi.serverAdmin.objectFreq(connId, topKeys);
    const map: Record<string, number | null> = {};
    let anySupported = false;
    for (const item of result) {
      map[item.key] = item.freq;
      if (item.freq !== null) anySupported = true;
    }
    freqMap.value = map;
    freqUnsupported.value = !anySupported;
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    freqLoading.value = false;
  }
}

// ---- Formatting helpers ----
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0B";
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(2)}KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(2)}MB`;
  return `${(bytes / 1073741824).toFixed(2)}GB`;
}

function formatTtl(ms: number): string {
  if (ms === -1) return t("bigkey.noTtl");
  if (ms === -2) return "-";
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(0)}s`;
  if (s < 3600) return `${(s / 60).toFixed(0)}m`;
  if (s < 86400) return `${(s / 3600).toFixed(1)}h`;
  return `${(s / 86400).toFixed(1)}d`;
}

function typeBadgeClass(type: string): string {
  const map: Record<string, string> = {
    string: "bg-success/10 text-success",
    hash: "bg-info/10 text-info",
    list: "bg-warning/10 text-warning",
    set: "bg-redis/10 text-redis",
    zset: "bg-purple-500/10 text-purple-400",
    stream: "bg-cyan-500/10 text-cyan-400",
  };
  return map[type] ?? "bg-bg-hover text-text-secondary";
}

const maxMemory = computed(() =>
  Math.max(...entries.value.map((e) => e.memoryBytes), 1)
);
</script>

<template>
  <div class="h-full flex flex-col min-w-[600px]">
    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <MemoryStick :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("monitor.noConnection") }}</p>
    </div>

    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center gap-2 flex-wrap mb-4 shrink-0">
        <div class="relative">
          <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            v-model="pattern"
            type="text"
            :disabled="scanning"
            placeholder="*"
            class="w-48 h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors disabled:opacity-50"
          />
        </div>
        <button
          v-if="!scanning"
          @click="startScan"
          class="h-7 px-3 text-xs font-medium rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1.5"
        >
          <Play :size="13" />
          {{ t("bigkey.start") }}
        </button>
        <button
          v-else
          @click="stopScan"
          class="h-7 px-3 text-xs font-medium rounded-lg bg-warning text-white hover:opacity-90 transition-colors flex items-center gap-1.5"
        >
          <Square :size="13" />
          {{ t("bigkey.stop") }}
        </button>
        <button
          @click="resetScan"
          :disabled="scanning"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors disabled:opacity-40 flex items-center gap-1"
        >
          <RefreshCw :size="13" />
          {{ t("bigkey.reset") }}
        </button>
        <button
          @click="detectHotKeys"
          :disabled="scanning || entries.length === 0 || freqLoading"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors disabled:opacity-40 flex items-center gap-1"
          :title="t('bigkey.hotKeyHint')"
        >
          <Flame :size="13" :class="freqLoading ? 'animate-pulse text-warning' : ''" />
          {{ t("bigkey.hotKeys") }}
        </button>
        <button
          @click="loadMemoryStats"
          :disabled="loadingStats"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors disabled:opacity-40 flex items-center gap-1"
        >
          <RefreshCw :size="13" :class="loadingStats ? 'animate-spin' : ''" />
          MEMORY STATS
        </button>
        <button
          @click="loadDoctor"
          :disabled="loadingDoctor"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors disabled:opacity-40 flex items-center gap-1"
        >
          <Stethoscope :size="13" :class="loadingDoctor ? 'animate-pulse' : ''" />
          MEMORY DOCTOR
        </button>
        <span v-if="scanning || scannedKeys > 0" class="text-[11px] font-mono text-text-muted">
          <RefreshCw v-if="scanning" :size="11" class="inline animate-spin mr-1" />
          {{ t("bigkey.scannedCount", { count: scannedKeys }) }}
        </span>
      </div>

      <!-- Error / notice banner -->
      <div
        v-if="error"
        class="px-3 py-2 mb-3 rounded-lg border border-warning/30 bg-warning/10 text-xs text-warning shrink-0"
      >
        {{ error }}
      </div>
      <div
        v-if="freqUnsupported"
        class="px-3 py-2 mb-3 rounded-lg border border-border bg-bg-secondary/50 text-xs text-text-muted shrink-0"
      >
        {{ t("bigkey.lfuUnsupported") }}
      </div>

      <div class="flex-1 min-h-0 grid grid-cols-1 xl:grid-cols-3 gap-4">
        <!-- Big key table -->
        <div class="xl:col-span-2 rounded-lg border border-border flex flex-col min-h-0">
          <div v-if="entries.length === 0 && !scanning" class="flex-1 flex flex-col items-center justify-center text-text-muted">
            <MemoryStick :size="40" :stroke-width="1.5" class="mb-3 opacity-30" />
            <p class="text-sm">{{ t("bigkey.empty") }}</p>
          </div>
          <div v-else class="flex-1 overflow-y-auto min-h-0">
            <div class="sticky top-0 z-10 grid grid-cols-[minmax(0,1fr)_70px_70px_90px_90px_80px] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
              <button @click="toggleSort('key')" class="text-left flex items-center gap-1 hover:text-text-primary transition-colors">
                {{ t("bigkey.colKey") }} <ArrowUpDown :size="10" :class="sortKey === 'key' ? 'text-redis' : 'opacity-40'" />
              </button>
              <span>{{ t("bigkey.colType") }}</span>
              <button @click="toggleSort('ttl')" class="text-left flex items-center gap-1 hover:text-text-primary transition-colors">
                TTL <ArrowUpDown :size="10" :class="sortKey === 'ttl' ? 'text-redis' : 'opacity-40'" />
              </button>
              <button @click="toggleSort('memoryBytes')" class="text-right flex items-center justify-end gap-1 hover:text-text-primary transition-colors">
                {{ t("bigkey.colMemory") }} <ArrowUpDown :size="10" :class="sortKey === 'memoryBytes' ? 'text-redis' : 'opacity-40'" />
              </button>
              <button @click="toggleSort('elementCount')" class="text-right flex items-center justify-end gap-1 hover:text-text-primary transition-colors">
                {{ t("bigkey.colElements") }} <ArrowUpDown :size="10" :class="sortKey === 'elementCount' ? 'text-redis' : 'opacity-40'" />
              </button>
              <span class="text-right">{{ t("bigkey.colFreq") }}</span>
            </div>
            <div class="divide-y divide-border/50">
              <div
                v-for="entry in sortedEntries"
                :key="entry.key"
                class="grid grid-cols-[minmax(0,1fr)_70px_70px_90px_90px_80px] gap-2 px-3 py-2 hover:bg-bg-secondary/60 transition-colors items-center"
              >
                <span class="text-xs font-mono text-text-primary truncate" :title="entry.key">{{ entry.key }}</span>
                <span class="text-[10px] font-semibold px-1.5 py-0.5 rounded w-fit" :class="typeBadgeClass(entry.keyType)">{{ entry.keyType }}</span>
                <span class="text-xs font-mono text-text-muted">{{ formatTtl(entry.ttl) }}</span>
                <div class="text-right">
                  <span class="text-xs font-mono font-semibold text-text-primary">{{ formatBytes(entry.memoryBytes) }}</span>
                  <div class="h-1 mt-0.5 rounded-full bg-bg-secondary overflow-hidden">
                    <div class="h-full bg-redis/40 rounded-full" :style="{ width: `${(entry.memoryBytes / maxMemory) * 100}%` }"></div>
                  </div>
                </div>
                <span class="text-xs font-mono text-text-secondary text-right">{{ entry.elementCount.toLocaleString() }}</span>
                <span class="text-xs font-mono text-right" :class="freqMap[entry.key] != null ? 'text-warning font-semibold' : 'text-text-muted'">
                  {{ entry.key in freqMap ? (freqMap[entry.key] ?? '-') : '-' }}
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- Memory side panel -->
        <div class="flex flex-col gap-4 min-h-0 overflow-y-auto">
          <!-- MEMORY DOCTOR -->
          <div class="rounded-lg border border-border flex flex-col max-h-[45%] min-h-0">
            <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 px-4 pt-4 pb-2 shrink-0">
              <Stethoscope :size="15" class="text-redis" />
              MEMORY DOCTOR
              <span class="cursor-help" :title="t('bigkey.doctorDesc')"><Info :size="12" class="text-text-muted" /></span>
            </h3>
            <div class="flex-1 overflow-y-auto min-h-0 px-4 pb-4">
              <p v-if="loadingDoctor" class="text-xs text-text-muted">
                <RefreshCw :size="11" class="inline animate-spin mr-1" />{{ t("common.loading") }}
              </p>
              <p v-else-if="doctorEntries.length === 0" class="text-xs text-text-muted">{{ t("bigkey.doctorEmpty") }}</p>
              <div v-else class="space-y-1.5">
                <div
                  v-for="entry in doctorEntries"
                  :key="entry.addr"
                  class="rounded-lg border transition-colors"
                  :class="isHealthy(entry.advice) ? 'border-border bg-bg-secondary/30' : 'border-warning/30 bg-warning/5'"
                >
                  <!-- Node header (clickable in cluster mode) -->
                  <button
                    v-if="doctorEntries.length > 1"
                    @click="toggleDoctorNode(entry.addr)"
                    class="w-full flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono transition-colors hover:bg-bg-hover/50 rounded-lg"
                  >
                    <component :is="doctorExpanded.has(entry.addr) ? ChevronDown : ChevronRight" :size="12" class="shrink-0 text-text-muted" />
                    <CheckCircle v-if="isHealthy(entry.advice)" :size="12" class="shrink-0 text-success" />
                    <Stethoscope v-else :size="12" class="shrink-0 text-warning" />
                    <span class="truncate" :class="isHealthy(entry.advice) ? 'text-text-secondary' : 'text-warning font-medium'">{{ entry.addr }}</span>
                    <span v-if="isHealthy(entry.advice)" class="text-[10px] text-success ml-auto">{{ t("bigkey.doctorHealthy") }}</span>
                  </button>
                  <!-- Standalone header (single node, not clickable) -->
                  <div v-else class="flex items-center gap-1.5 px-3 py-1.5 text-xs">
                    <CheckCircle v-if="isHealthy(entry.advice)" :size="12" class="shrink-0 text-success" />
                    <Stethoscope v-else :size="12" class="shrink-0 text-warning" />
                    <span v-if="entry.addr" class="font-mono text-text-secondary truncate">{{ entry.addr }}</span>
                    <span v-if="isHealthy(entry.advice)" class="text-[10px] text-success ml-auto">{{ t("bigkey.doctorHealthy") }}</span>
                  </div>
                  <!-- Advice content (collapsible) -->
                  <pre
                    v-if="doctorEntries.length === 1 || doctorExpanded.has(entry.addr)"
                    v-show="!isHealthy(entry.advice)"
                    class="px-3 pb-2 text-[11px] text-text-secondary whitespace-pre-wrap break-words font-mono leading-relaxed"
                  >{{ entry.advice }}</pre>
                </div>
              </div>
            </div>
          </div>

          <!-- MEMORY STATS -->
          <div class="rounded-lg border border-border p-4 flex-1 min-h-0 flex flex-col">
            <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-2 shrink-0">
              <MemoryStick :size="15" class="text-redis" />
              MEMORY STATS
              <span class="cursor-help" :title="t('bigkey.statsDesc')"><Info :size="12" class="text-text-muted" /></span>
            </h3>
            <input
              v-model="statsFilter"
              type="text"
              :placeholder="t('bigkey.statsFilterPlaceholder')"
              class="w-full h-7 px-2 mb-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors shrink-0"
            />
            <div v-if="loadingStats" class="text-xs text-text-muted py-2">
              <RefreshCw :size="11" class="inline animate-spin mr-1" />{{ t("common.loading") }}
            </div>
            <div v-else-if="filteredStats.length === 0" class="text-xs text-text-muted py-2">{{ t("bigkey.statsEmpty") }}</div>
            <div v-else class="space-y-1 overflow-y-auto min-h-0">
              <div
                v-for="item in filteredStats"
                :key="item.name"
                class="flex items-center justify-between gap-2 px-2 py-1 rounded hover:bg-bg-secondary/60 transition-colors"
              >
                <span class="text-[11px] font-mono text-text-secondary truncate" :title="item.name">{{ item.name }}</span>
                <span class="text-[11px] font-mono text-text-primary shrink-0">{{ item.value.toLocaleString() }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
