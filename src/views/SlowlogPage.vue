<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Activity, RefreshCw, Search, Copy, Database, Zap, Timer, List, BarChart3, Download, TrendingUp, KeyRound, BookOpen, PenLine, X } from "lucide-vue-next";
import { useSlowlogStore } from "@/stores/slowlogStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";
import { tauriApi } from "@/services/tauriApi";

const trendCanvasRef = ref<HTMLCanvasElement | null>(null);
const trendContainerRef = ref<HTMLElement | null>(null);

const { t } = useI18n();
const slowlogStore = useSlowlogStore();
const connStore = useConnectionStore();

const searchQuery = ref("");
const fetchCount = ref(128);
const durationFilter = ref<"all" | "10ms" | "100ms" | "1s">("all");
const viewMode = ref<"list" | "analytics">("list");

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
  viewMode.value = "list";
  clearChartFilters();
  expandedId.value = null;
  if (newId) {
    slowlogStore.fetchSlowlog(fetchCount.value);
  }
});

onBeforeUnmount(() => {
  slowlogStore.stopAutoRefresh();
});

const filteredEntries = computed(() => {
  let items = slowlogStore.entries;
  if (durationFilter.value === "10ms") {
    items = items.filter((e) => e.durationUs >= 10000);
  } else if (durationFilter.value === "100ms") {
    items = items.filter((e) => e.durationUs >= 100000);
  } else if (durationFilter.value === "1s") {
    items = items.filter((e) => e.durationUs >= 1000000);
  }
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.trim().toLowerCase();
    items = items.filter((e) => e.command.toLowerCase().includes(q));
  }
  // Chart-linked filters
  if (bucketFilterIdx.value !== null) {
    const b = BUCKET_DEFS[bucketFilterIdx.value];
    if (b) items = items.filter((e) => e.durationUs >= b.minUs && e.durationUs < b.maxUs);
  }
  if (cmdFilter.value) {
    const verb = cmdFilter.value;
    items = items.filter((e) => cmdVerb(e.command).toUpperCase() === verb);
  }
  if (keyFilter.value) {
    const k = keyFilter.value;
    items = items.filter((e) => extractKeys(e.command).includes(k));
  }
  return items;
});

// ---- Chart-linked filtering ----
const bucketFilterIdx = ref<number | null>(null);
const cmdFilter = ref<string | null>(null);
const keyFilter = ref<string | null>(null);

function applyBucketFilter(idx: number) {
  bucketFilterIdx.value = bucketFilterIdx.value === idx ? null : idx;
  viewMode.value = "list";
}

function applyCmdFilter(name: string) {
  cmdFilter.value = cmdFilter.value === name ? null : name;
  viewMode.value = "list";
}

function applyKeyFilter(key: string) {
  keyFilter.value = keyFilter.value === key ? null : key;
  viewMode.value = "list";
}

function clearChartFilters() {
  bucketFilterIdx.value = null;
  cmdFilter.value = null;
  keyFilter.value = null;
}

const hasChartFilters = computed(
  () => bucketFilterIdx.value !== null || cmdFilter.value !== null || keyFilter.value !== null
);

// ---- Row detail expansion ----
const expandedId = ref<number | null>(null);

function toggleExpand(id: number) {
  expandedId.value = expandedId.value === id ? null : id;
}

// ---- Analytics: Key hotspot ----
interface HotKey {
  key: string;
  count: number;
  totalDurationUs: number;
  avgDurationUs: number;
}

/** Commands whose arguments are all keys */
const MULTI_KEY_CMDS = new Set(["DEL", "UNLINK", "MGET", "MSET", "MSETNX", "TOUCH", "EXISTS", "WATCH", "SINTER", "SUNION", "SDIFF", "SMOVE", "RPOPLPUSH", "LMOVE", "PFCOUNT", "PFMERGE", "GEORADIUS", "RENAME", "RENAMENX", "COPY"]);
/** Commands with no key argument */
const NO_KEY_CMDS = new Set(["KEYS", "SCAN", "RANDOMKEY", "INFO", "PING", "ECHO", "SELECT", "AUTH", "DBSIZE", "TIME", "CONFIG", "CLIENT", "SLOWLOG", "DEBUG", "WAIT", "SCRIPT", "EVAL", "EVALSHA", "FLUSHDB", "FLUSHALL", "SAVE", "BGSAVE", "MONITOR", "SUBSCRIBE", "UNSUBSCRIBE", "PUBLISH", "RESET", "COMMAND", "MEMORY", "SHUTDOWN", "OBJECT", "SWAPDB", "LATENCY", "CLUSTER", "READONLY", "READWRITE", "REPLICAOF", "SLAVEOF"]);

/** Extract key names from a slowlog command string (heuristic) */
function extractKeys(cmd: string): string[] {
  const parts = cmd.split(/\s+/);
  if (parts.length === 0) return [];
  const verb = parts[0].toUpperCase();
  if (NO_KEY_CMDS.has(verb)) return [];
  if (MULTI_KEY_CMDS.has(verb)) return parts.slice(1);
  return parts.length > 1 ? [parts[1]] : [];
}

const hotKeys = computed<HotKey[]>(() => {
  const map = new Map<string, { count: number; total: number }>();
  for (const entry of filteredEntries.value) {
    for (const key of extractKeys(entry.command)) {
      const existing = map.get(key) ?? { count: 0, total: 0 };
      existing.count++;
      existing.total += entry.durationUs;
      map.set(key, existing);
    }
  }
  return Array.from(map.entries())
    .map(([key, v]) => ({ key, count: v.count, totalDurationUs: v.total, avgDurationUs: v.total / v.count }))
    .sort((a, b) => b.count - a.count || b.totalDurationUs - a.totalDurationUs)
    .slice(0, 20);
});

// ---- Analytics: Read/Write classification ----
const WRITE_CMDS = new Set(["DEL", "UNLINK", "SET", "SETNX", "SETEX", "PSETEX", "MSET", "MSETNX", "APPEND", "INCR", "INCRBY", "INCRBYFLOAT", "DECR", "DECRBY", "SETRANGE", "EXPIRE", "EXPIREAT", "PEXPIRE", "PEXPIREAT", "PERSIST", "RENAME", "RENAMENX", "RESTORE", "HSET", "HSETNX", "HMSET", "HDEL", "HINCRBY", "HINCRBYFLOAT", "LPUSH", "LPUSHX", "RPUSH", "RPUSHX", "LPOP", "RPOP", "LSET", "LREM", "LTRIM", "LINSERT", "LMOVE", "RPOPLPUSH", "SADD", "SREM", "SPOP", "SMOVE", "SINTERSTORE", "SUNIONSTORE", "SDIFFSTORE", "ZADD", "ZREM", "ZINCRBY", "ZPOPMIN", "ZPOPMAX", "ZRANGESTORE", "ZUNIONSTORE", "ZINTERSTORE", "XADD", "XDEL", "XTRIM", "PFADD", "PFMERGE", "GEOADD", "PUBLISH", "COPY", "FLUSHDB", "FLUSHALL", "EVAL", "EVALSHA", "SCRIPT", "TOUCH", "SORT"]);

const readWriteStats = computed(() => {
  let readCount = 0, readTotal = 0, writeCount = 0, writeTotal = 0;
  for (const entry of filteredEntries.value) {
    const verb = cmdVerb(entry.command).toUpperCase();
    if (WRITE_CMDS.has(verb)) {
      writeCount++;
      writeTotal += entry.durationUs;
    } else {
      readCount++;
      readTotal += entry.durationUs;
    }
  }
  return { readCount, readTotal, writeCount, writeTotal };
});

// ---- Analytics: Command type grouping ----
interface CommandGroup {
  name: string;
  count: number;
  totalDurationUs: number;
  avgDurationUs: number;
  maxDurationUs: number;
}

const commandGroups = computed<CommandGroup[]>(() => {
  const map = new Map<string, { count: number; total: number; max: number }>();
  for (const entry of filteredEntries.value) {
    const verb = cmdVerb(entry.command).toUpperCase();
    const existing = map.get(verb) ?? { count: 0, total: 0, max: 0 };
    existing.count++;
    existing.total += entry.durationUs;
    existing.max = Math.max(existing.max, entry.durationUs);
    map.set(verb, existing);
  }
  return Array.from(map.entries())
    .map(([name, v]) => ({
      name,
      count: v.count,
      totalDurationUs: v.total,
      avgDurationUs: v.total / v.count,
      maxDurationUs: v.max,
    }))
    .sort((a, b) => b.totalDurationUs - a.totalDurationUs);
});

const maxGroupTotal = computed(() =>
  Math.max(...commandGroups.value.map((g) => g.totalDurationUs), 1)
);

// ---- Analytics: Duration distribution ----
interface DurationBucket {
  label: string;
  minUs: number;
  maxUs: number;
  count: number;
}

const BUCKET_DEFS: Omit<DurationBucket, "count">[] = [
  { label: "<1ms", minUs: 0, maxUs: 1000 },
  { label: "1-5ms", minUs: 1000, maxUs: 5000 },
  { label: "5-10ms", minUs: 5000, maxUs: 10000 },
  { label: "10-50ms", minUs: 10000, maxUs: 50000 },
  { label: "50-100ms", minUs: 50000, maxUs: 100000 },
  { label: "100-500ms", minUs: 100000, maxUs: 500000 },
  { label: "500ms-1s", minUs: 500000, maxUs: 1000000 },
  { label: ">1s", minUs: 1000000, maxUs: Infinity },
];

const durationBuckets = computed<DurationBucket[]>(() => {
  const buckets: DurationBucket[] = BUCKET_DEFS.map((d) => ({ ...d, count: 0 }));
  for (const entry of slowlogStore.entries) {
    for (const bucket of buckets) {
      if (entry.durationUs >= bucket.minUs && entry.durationUs < bucket.maxUs) {
        bucket.count++;
        break;
      }
    }
  }
  return buckets;
});

const maxBucketCount = computed(() =>
  Math.max(...durationBuckets.value.map((b) => b.count), 1)
);

// ---- Analytics: Trend data ----
const trendEntries = computed(() => {
  return [...filteredEntries.value].sort((a, b) => a.timestamp - b.timestamp);
});

// ---- Export functions ----
function getExportFilename(ext: string): string {
  const conn = connStore.activeConnection;
  const host = conn?.host || "unknown";
  const port = conn?.port || 0;
  const now = new Date();
  const ts = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}_${String(now.getHours()).padStart(2, "0")}${String(now.getMinutes()).padStart(2, "0")}${String(now.getSeconds()).padStart(2, "0")}`;
  return `slowlog_${host}_${port}_${ts}.${ext}`;
}

function formatLocalTime(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** Toast action that reveals the exported file in the system file manager */
function openLocationAction(path: string) {
  return {
    label: t("slowlog.openLocation"),
    onClick: () => {
      tauriApi.slowlog.openFileLocation(path).catch(() => toast.error(t("common.error")));
    },
  };
}

async function exportAsJson() {
  const data = filteredEntries.value.map((e) => ({
    id: e.id,
    timestamp: e.timestamp,
    time: formatLocalTime(e.timestamp),
    durationUs: e.durationUs,
    durationMs: +(e.durationUs / 1000).toFixed(2),
    command: e.command,
    argsCount: e.argsCount,
    clientAddr: e.clientAddr,
    clientName: e.clientName,
  }));
  try {
    const path = await tauriApi.slowlog.saveExport(JSON.stringify(data, null, 2), getExportFilename("json"));
    toast.success(t("slowlog.exportSuccess", { path }), undefined, openLocationAction(path));
  } catch {
    toast.error(t("common.error"));
  }
}

async function exportAsCsv() {
  // CSV headers follow current i18n language
  const headers = [
    t("slowlog.colId"),
    t("slowlog.colTimestamp"),
    t("slowlog.colTime"),
    t("slowlog.colDuration") + "(us)",
    t("slowlog.colDuration") + "(ms)",
    t("slowlog.colCommand"),
    t("slowlog.colArgsCount"),
    t("slowlog.colClient"),
    t("slowlog.colClientName"),
  ];
  const rows = filteredEntries.value.map((e) => {
    const cmd = `"${e.command.replace(/"/g, '""')}"`;
    return `${e.id},${e.timestamp},${formatLocalTime(e.timestamp)},${e.durationUs},${(e.durationUs / 1000).toFixed(2)},${cmd},${e.argsCount},${e.clientAddr || ""},${e.clientName || ""}`;
  });
  try {
    const content = "\uFEFF" + [headers.join(","), ...rows].join("\n"); // BOM for Excel
    const path = await tauriApi.slowlog.saveExport(content, getExportFilename("csv"));
    toast.success(t("slowlog.exportSuccess", { path }), undefined, openLocationAction(path));
  } catch {
    toast.error(t("common.error"));
  }
}

// ---- Analytics: Trend chart (Canvas) ----
function getCssColor(varName: string, fallback: string): string {
  const val = getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
  return val || fallback;
}

function drawTrendChart() {
  const canvas = trendCanvasRef.value;
  const container = trendContainerRef.value;
  if (!canvas || !container) return;

  const entries = trendEntries.value;
  if (entries.length === 0) return;

  const w = container.clientWidth;
  const h = 190;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w, h);

  const padding = { top: 16, right: 16, bottom: 24, left: 52 };
  const chartW = w - padding.left - padding.right;
  const chartH = h - padding.top - padding.bottom;

  const maxDur = Math.max(...entries.map((e) => e.durationUs)) * 1.1;

  // Index-based X positions: even spacing avoids clustering when the time span has large gaps
  const toX = (idx: number) =>
    padding.left + (entries.length === 1 ? chartW / 2 : (idx / (entries.length - 1)) * chartW);
  const toY = (dur: number) => padding.top + chartH - (dur / maxDur) * chartH;

  const gridColor = getCssColor("--color-border", "#e2e6ef");
  const labelColor = getCssColor("--color-text-muted", "#8b92ad");

  // Grid lines (4 horizontal)
  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 0.5;
  for (let i = 0; i <= 4; i++) {
    const y = padding.top + (chartH / 4) * i;
    ctx.beginPath();
    ctx.moveTo(padding.left, y);
    ctx.lineTo(w - padding.right, y);
    ctx.stroke();
    // Y labels
    ctx.fillStyle = labelColor;
    ctx.font = "10px Inter, sans-serif";
    ctx.textAlign = "right";
    const val = maxDur - (maxDur / 4) * i;
    ctx.fillText(formatDuration(val), padding.left - 6, y + 3);
  }

  // X-axis time labels at evenly spaced indexes (first / middle / last)
  ctx.fillStyle = labelColor;
  ctx.font = "9px Inter, sans-serif";
  const tickCount = Math.min(entries.length, 5);
  for (let tIdx = 0; tIdx < tickCount; tIdx++) {
    const entryIdx = tickCount === 1 ? 0 : Math.round((tIdx / (tickCount - 1)) * (entries.length - 1));
    const x = toX(entryIdx);
    ctx.textAlign = tIdx === 0 ? "left" : tIdx === tickCount - 1 ? "right" : "center";
    ctx.fillText(formatTime(entries[entryIdx].timestamp), x, h - 4);
    // Vertical grid line at each tick (skip edges)
    if (tIdx > 0 && tIdx < tickCount - 1) {
      ctx.strokeStyle = gridColor;
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(x, padding.top);
      ctx.lineTo(x, padding.top + chartH);
      ctx.stroke();
    }
  }

  // Draw connecting line
  if (entries.length > 1) {
    ctx.beginPath();
    for (let i = 0; i < entries.length; i++) {
      const x = toX(i);
      const y = toY(entries[i].durationUs);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.strokeStyle = getCssColor("--color-redis", "#DC382D") + "44";
    ctx.lineWidth = 1.5;
    ctx.stroke();
  }

  // Draw dots
  entries.forEach((entry, i) => {
    const x = toX(i);
    const y = toY(entry.durationUs);
    const color = entry.durationUs >= 1000000 ? "#ef4444"
      : entry.durationUs >= 100000 ? "#f59e0b"
      : entry.durationUs >= 10000 ? "#fbbf24"
      : "#22c55e";

    ctx.beginPath();
    ctx.arc(x, y, 4, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, Math.PI * 2);
    ctx.fillStyle = color + "33";
    ctx.fill();
  });
}

// Redraw immediately when the container is resized (window resize / resolution change)
let trendResizeObserver: ResizeObserver | null = null;
let trendObservedEl: HTMLElement | null = null;
let trendResizeRaf = 0;

function setupTrendResizeObserver() {
  const el = trendContainerRef.value;
  if (!el) return;
  // Container is recreated on view toggle; rebind when the element changes
  if (trendResizeObserver && trendObservedEl === el) return;
  trendResizeObserver?.disconnect();
  trendResizeObserver = new ResizeObserver(() => {
    cancelAnimationFrame(trendResizeRaf);
    trendResizeRaf = requestAnimationFrame(() => drawTrendChart());
  });
  trendObservedEl = el;
  trendResizeObserver.observe(el);
}

onBeforeUnmount(() => {
  trendResizeObserver?.disconnect();
  trendResizeObserver = null;
  trendObservedEl = null;
  cancelAnimationFrame(trendResizeRaf);
});
// Watch for data changes to redraw
watch([trendEntries, viewMode], () => {
  if (viewMode.value === "analytics") {
    nextTick(() => {
      drawTrendChart();
      setupTrendResizeObserver();
    });
  }
});

watch(() => slowlogStore.entries, () => {
  if (viewMode.value === "analytics") {
    nextTick(() => {
      drawTrendChart();
      setupTrendResizeObserver();
    });
  }
});

// ---- Common helpers ----
function formatDuration(us: number): string {
  if (us < 1000) return `${us}μs`;
  if (us < 1000000) return `${(us / 1000).toFixed(1)}ms`;
  return `${(us / 1000000).toFixed(2)}s`;
}

function durationColor(us: number): string {
  if (us >= 1000000) return "text-danger";
  if (us >= 100000) return "text-warning";
  if (us >= 10000) return "text-amber-400";
  return "text-success";
}

function bucketColor(index: number): string {
  const colors = ["bg-green-500", "bg-green-400", "bg-lime-400", "bg-yellow-400", "bg-amber-400", "bg-orange-400", "bg-red-400", "bg-red-600"];
  return colors[index] ?? "bg-gray-400";
}

function formatTime(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function cmdVerb(cmd: string): string {
  return cmd.split(/\s/)[0] ?? cmd;
}

function cmdArgs(cmd: string): string {
  const idx = cmd.indexOf(" ");
  return idx >= 0 ? cmd.slice(idx + 1) : "";
}

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
        <!-- View mode toggle -->
        <div class="flex items-center h-7 rounded-lg border border-border overflow-hidden">
          <button
            @click="viewMode = 'list'"
            class="px-2.5 h-full text-xs flex items-center gap-1 transition-colors"
            :class="viewMode === 'list' ? 'bg-redis/10 text-redis' : 'text-text-secondary hover:bg-bg-hover'"
          >
            <List :size="13" />
            {{ t("slowlog.viewList") }}
          </button>
          <button
            @click="viewMode = 'analytics'"
            class="px-2.5 h-full text-xs flex items-center gap-1 transition-colors border-l border-border"
            :class="viewMode === 'analytics' ? 'bg-redis/10 text-redis' : 'text-text-secondary hover:bg-bg-hover'"
          >
            <BarChart3 :size="13" />
            {{ t("slowlog.viewAnalytics") }}
          </button>
        </div>
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
        <!-- Export buttons -->
        <button
          @click="exportAsJson"
          :disabled="filteredEntries.length === 0"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
        >
          <Download :size="13" />
          JSON
        </button>
        <button
          @click="exportAsCsv"
          :disabled="filteredEntries.length === 0"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
        >
          <Download :size="13" />
          CSV
        </button>
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
    <div v-if="connStore.activeConnectionId" class="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-4 shrink-0">
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

    <!-- List View -->
    <template v-else-if="viewMode === 'list'">
      <div class="flex-1 overflow-y-auto">
        <!-- Active chart-linked filters -->
        <div v-if="hasChartFilters" class="flex items-center gap-2 flex-wrap px-3 py-2 border-b border-border bg-bg-secondary/40">
          <span class="text-[11px] text-text-muted">{{ t("slowlog.activeFilters") }}:</span>
          <span
            v-if="bucketFilterIdx !== null"
            class="flex items-center gap-1 px-2 py-0.5 rounded-full bg-redis/10 text-redis text-[11px]"
          >
            {{ BUCKET_DEFS[bucketFilterIdx]?.label }}
            <button @click="bucketFilterIdx = null" class="hover:opacity-70"><X :size="10" /></button>
          </span>
          <span
            v-if="cmdFilter"
            class="flex items-center gap-1 px-2 py-0.5 rounded-full bg-redis/10 text-redis text-[11px]"
          >
            {{ cmdFilter }}
            <button @click="cmdFilter = null" class="hover:opacity-70"><X :size="10" /></button>
          </span>
          <span
            v-if="keyFilter"
            class="flex items-center gap-1 px-2 py-0.5 rounded-full bg-redis/10 text-redis text-[11px] max-w-[240px]"
          >
            <span class="truncate font-mono">{{ keyFilter }}</span>
            <button @click="keyFilter = null" class="hover:opacity-70 shrink-0"><X :size="10" /></button>
          </span>
          <button @click="clearChartFilters" class="text-[11px] text-text-muted underline hover:text-text-primary transition-colors">
            {{ t("slowlog.clearFilters") }}
          </button>
        </div>
        <div class="sticky top-0 z-10 grid grid-cols-[60px_140px_100px_1fr_120px_40px] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
          <span>{{ t("slowlog.colId") }}</span>
          <span>{{ t("slowlog.colTime") }}</span>
          <span>{{ t("slowlog.colDuration") }}</span>
          <span>{{ t("slowlog.colCommand") }}</span>
          <span>{{ t("slowlog.colClient") }}</span>
          <span></span>
        </div>
        <div class="divide-y divide-border/50">
          <div v-for="entry in filteredEntries" :key="entry.id">
            <div
              @click="toggleExpand(entry.id)"
              class="group grid grid-cols-[60px_140px_100px_1fr_120px_40px] gap-2 px-3 py-2 hover:bg-bg-secondary/60 transition-colors items-center cursor-pointer"
              :class="expandedId === entry.id ? 'bg-bg-secondary/60' : ''"
            >
              <span class="text-xs font-mono text-text-muted">#{{ entry.id }}</span>
              <span class="text-xs font-mono text-text-secondary" :title="new Date(entry.timestamp * 1000).toLocaleString()">
                {{ formatTime(entry.timestamp) }}
              </span>
              <span class="text-xs font-mono font-semibold" :class="durationColor(entry.durationUs)">
                {{ formatDuration(entry.durationUs) }}
              </span>
              <div class="flex items-center min-w-0" :title="entry.command">
                <code class="text-xs font-mono truncate">
                  <span class="text-redis font-semibold">{{ cmdVerb(entry.command) }}</span>
                  <span v-if="cmdArgs(entry.command)" class="text-text-secondary ml-1">{{ cmdArgs(entry.command) }}</span>
                </code>
              </div>
              <span class="text-[11px] font-mono text-text-muted truncate" :title="entry.clientAddr || ''">
                {{ entry.clientAddr || '-' }}
              </span>
              <button
                @click.stop="copyCommand(entry.command)"
                class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded hover:bg-bg-hover"
                :title="t('slowlog.copyCommand')"
              >
                <Copy :size="12" class="text-text-muted" />
              </button>
            </div>
            <!-- Expanded detail panel -->
            <div
              v-if="expandedId === entry.id"
              class="px-3 py-3 bg-bg-secondary/50 border-l-2 border-redis mx-3 mb-1 rounded-r-lg"
            >
              <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-3">
                <div>
                  <p class="text-[10px] text-text-muted uppercase tracking-wider mb-0.5">{{ t("slowlog.colTimestamp") }}</p>
                  <p class="text-xs font-mono text-text-primary">{{ new Date(entry.timestamp * 1000).toLocaleString() }}</p>
                </div>
                <div>
                  <p class="text-[10px] text-text-muted uppercase tracking-wider mb-0.5">{{ t("slowlog.colArgsCount") }}</p>
                  <p class="text-xs font-mono text-text-primary">{{ entry.argsCount }}</p>
                </div>
                <div>
                  <p class="text-[10px] text-text-muted uppercase tracking-wider mb-0.5">{{ t("slowlog.colClient") }}</p>
                  <p class="text-xs font-mono text-text-primary">{{ entry.clientAddr || '-' }}</p>
                </div>
                <div>
                  <p class="text-[10px] text-text-muted uppercase tracking-wider mb-0.5">{{ t("slowlog.colClientName") }}</p>
                  <p class="text-xs font-mono text-text-primary">{{ entry.clientName || '-' }}</p>
                </div>
              </div>
              <p class="text-[10px] text-text-muted uppercase tracking-wider mb-1">{{ t("slowlog.fullCommand") }}</p>
              <div class="flex items-start gap-2">
                <pre class="flex-1 text-xs font-mono text-text-primary bg-bg-primary/60 rounded-lg p-3 whitespace-pre-wrap break-all border border-border">{{ entry.command }}</pre>
                <button
                  @click="copyCommand(entry.command)"
                  class="shrink-0 p-1.5 rounded-lg border border-border text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
                  :title="t('slowlog.copyCommand')"
                >
                  <Copy :size="13" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Analytics View -->
    <template v-else>
      <div class="flex-1 overflow-y-auto grid grid-cols-1 xl:grid-cols-2 gap-4 content-start items-start">
        <!-- Read/Write Classification -->
        <div class="rounded-lg border border-border p-4">
          <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-3">
            <BookOpen :size="15" class="text-redis" />
            {{ t("slowlog.readWriteStats") }}
          </h3>
          <div class="grid grid-cols-2 gap-3">
            <div class="flex items-center gap-3 p-3 rounded-lg bg-bg-secondary/50 border border-border">
              <BookOpen :size="18" class="text-info shrink-0" />
              <div class="min-w-0">
                <p class="text-[11px] text-text-muted">{{ t("slowlog.readCmds") }}</p>
                <p class="text-sm font-semibold text-text-primary">
                  {{ readWriteStats.readCount }}
                  <span class="text-[11px] font-normal text-text-muted ml-1">{{ t("slowlog.totalTime") }} {{ formatDuration(readWriteStats.readTotal) }}</span>
                </p>
              </div>
            </div>
            <div class="flex items-center gap-3 p-3 rounded-lg bg-bg-secondary/50 border border-border">
              <PenLine :size="18" class="text-warning shrink-0" />
              <div class="min-w-0">
                <p class="text-[11px] text-text-muted">{{ t("slowlog.writeCmds") }}</p>
                <p class="text-sm font-semibold text-text-primary">
                  {{ readWriteStats.writeCount }}
                  <span class="text-[11px] font-normal text-text-muted ml-1">{{ t("slowlog.totalTime") }} {{ formatDuration(readWriteStats.writeTotal) }}</span>
                </p>
              </div>
            </div>
          </div>
          <!-- Read vs Write ratio bar -->
          <div v-if="readWriteStats.readCount + readWriteStats.writeCount > 0" class="mt-3 flex h-2 rounded-full overflow-hidden bg-bg-secondary">
            <div
              class="bg-info transition-all duration-300"
              :style="{ width: `${(readWriteStats.readCount / (readWriteStats.readCount + readWriteStats.writeCount)) * 100}%` }"
            ></div>
            <div class="flex-1 bg-warning"></div>
          </div>
        </div>

        <!-- Duration Distribution -->
        <div class="rounded-lg border border-border p-4">
          <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-3">
            <Timer :size="15" class="text-redis" />
            {{ t("slowlog.durationDistribution") }}
          </h3>
          <div class="flex items-end gap-1 h-32">
            <div
              v-for="(bucket, idx) in durationBuckets"
              :key="bucket.label"
              @click="bucket.count > 0 && applyBucketFilter(idx)"
              class="flex-1 flex flex-col items-center gap-1 rounded-lg pb-1 transition-colors"
              :class="[bucket.count > 0 ? 'cursor-pointer hover:bg-bg-secondary/60' : 'cursor-default', bucketFilterIdx === idx ? 'bg-redis/10 ring-1 ring-redis/40' : '']"
              :title="bucket.count > 0 ? t('slowlog.clickToFilter') : ''"
            >
              <span class="text-[10px] text-text-secondary font-mono">{{ bucket.count }}</span>
              <div
                class="w-full rounded-t transition-all duration-300"
                :class="bucketColor(idx)"
                :style="{ height: `${(bucket.count / maxBucketCount) * 80}px`, minHeight: bucket.count > 0 ? '4px' : '2px', opacity: bucket.count > 0 ? 1 : 0.2 }"
              ></div>
              <span class="text-[9px] text-text-muted whitespace-nowrap">{{ bucket.label }}</span>
            </div>
          </div>
        </div>

        <!-- Command Type Distribution -->
        <div class="rounded-lg border border-border p-4">
          <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-3">
            <BarChart3 :size="15" class="text-redis" />
            {{ t("slowlog.commandDistribution") }}
          </h3>
          <div class="space-y-2 max-h-72 overflow-y-auto overflow-x-hidden pr-1">
            <div
              v-for="group in commandGroups"
              :key="group.name"
              @click="applyCmdFilter(group.name)"
              class="flex items-center gap-3 px-2 py-1 -mx-2 rounded-lg cursor-pointer transition-colors"
              :class="cmdFilter === group.name ? 'bg-redis/10' : 'hover:bg-bg-secondary/60'"
              :title="t('slowlog.clickToFilter')"
            >
              <span class="text-xs font-mono font-semibold text-redis w-20 shrink-0 text-right">{{ group.name }}</span>
              <div class="flex-1 h-5 bg-bg-secondary rounded overflow-hidden relative">
                <div
                  class="h-full bg-redis/20 rounded transition-all duration-300"
                  :style="{ width: `${(group.totalDurationUs / maxGroupTotal) * 100}%` }"
                ></div>
                <span class="absolute left-2 top-1/2 -translate-y-1/2 text-[10px] text-text-secondary">
                  {{ group.count }} · {{ t("slowlog.totalTime") }} {{ formatDuration(group.totalDurationUs) }}
                </span>
              </div>
              <span class="text-[10px] text-text-muted w-24 shrink-0 text-right">
                {{ t("slowlog.avgTime") }} {{ formatDuration(group.avgDurationUs) }}
              </span>
            </div>
          </div>
        </div>

        <!-- Key Hotspot -->
        <div class="rounded-lg border border-border p-4">
          <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-3">
            <KeyRound :size="15" class="text-redis" />
            {{ t("slowlog.keyHotspot") }}
            <span class="text-[10px] font-normal text-text-muted">{{ t("slowlog.keyHotspotHint") }}</span>
          </h3>
          <div v-if="hotKeys.length > 0" class="space-y-1.5 max-h-72 overflow-y-auto overflow-x-hidden pr-1">
            <div
              v-for="hk in hotKeys"
              :key="hk.key"
              @click="applyKeyFilter(hk.key)"
              class="flex items-center gap-3 px-2 py-1.5 rounded-lg cursor-pointer transition-colors"
              :class="keyFilter === hk.key ? 'bg-redis/10 border border-redis/30' : 'hover:bg-bg-secondary/60 border border-transparent'"
              :title="t('slowlog.clickToFilter')"
            >
              <span class="text-xs font-mono text-text-primary flex-1 min-w-0 truncate" :title="hk.key">{{ hk.key }}</span>
              <div class="w-24 shrink-0 h-4 bg-bg-secondary rounded overflow-hidden">
                <div
                  class="h-full bg-redis/25 rounded transition-all duration-300"
                  :style="{ width: `${(hk.count / (hotKeys[0]?.count || 1)) * 100}%` }"
                ></div>
              </div>
              <span class="text-[11px] font-mono text-text-secondary w-10 shrink-0 text-right">{{ hk.count }}×</span>
              <span class="text-[11px] font-mono text-text-muted w-16 shrink-0 text-right">{{ formatDuration(hk.avgDurationUs) }}</span>
            </div>
          </div>
          <div v-else class="text-xs text-text-muted text-center py-4">
            {{ t("slowlog.noEntries") }}
          </div>
        </div>

        <!-- Trend Chart (bottom, full width) -->
        <div class="rounded-lg border border-border p-4 xl:col-span-2">
          <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2 mb-3">
            <TrendingUp :size="15" class="text-redis" />
            {{ t("slowlog.trendChart") }}
          </h3>
          <div v-if="trendEntries.length > 0" ref="trendContainerRef" class="w-full">
            <canvas ref="trendCanvasRef" class="w-full" style="height: 190px;" />
          </div>
          <div v-else class="text-xs text-text-muted text-center py-4">
            {{ t("slowlog.noEntries") }}
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
