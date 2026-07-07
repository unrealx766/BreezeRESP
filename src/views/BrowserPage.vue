<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { watch, onMounted, ref, computed, nextTick } from "vue";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { useConnectionStore } from "@/stores/connectionStore";
import type { RedisDataType } from "@/types";
import KeyTreeItem from "@/components/cascade/KeyTreeItem.vue";
import TtlGauge from "@/components/charts/TtlGauge.vue";
import {
  Search, RefreshCw, Trash2, Copy, Tag, Database,
  Type, Hash, List, CircleDot, BarChart3,
  AlertTriangle, X, Wifi, ChevronDown, Check,
} from "lucide-vue-next";

const { t } = useI18n();
const cascade = useCascadeStore();
const detail = useDetailStore();
const connStore = useConnectionStore();

const currentDb = ref(0);
const switchingDb = ref(false);
const connectionLost = ref(false);
const showDbDropdown = ref(false);

// Debounce search input: wait 300ms after user stops typing before triggering filter/scan
let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  () => cascade.searchQuery,
  (val) => {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      cascade.debouncedSearchQuery = val;
    }, 300);
  }
);

// When debounced search changes, re-scan from Redis with new pattern
watch(
  () => cascade.debouncedSearchQuery,
  () => {
    // Reset scroll position when search changes
    scrollTop.value = 0;
    nextTick(() => {
      const el = treeScrollRef.value;
      if (el) el.scrollTop = 0;
    });
    cascade.refreshKeys(true);
  }
);

// --- Virtual scroll ---
const ITEM_HEIGHT = 32;
const BUFFER = 10;
const treeScrollRef = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const containerHeight = ref(500);

const visibleItems = computed(() => {
  const nodes = cascade.visibleNodes;
  const start = Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER);
  const visibleCount = Math.ceil(containerHeight.value / ITEM_HEIGHT) + 2 * BUFFER;
  const end = Math.min(nodes.length, start + visibleCount);
  return nodes.slice(start, end).map((item, i) => ({
    ...item,
    index: start + i,
  }));
});

const topPadding = computed(() => {
  const start = Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER);
  return start * ITEM_HEIGHT;
});
const bottomPadding = computed(() => {
  const end = Math.min(
    cascade.visibleNodes.length,
    Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER) + Math.ceil(containerHeight.value / ITEM_HEIGHT) + 2 * BUFFER
  );
  return Math.max(0, (cascade.visibleNodes.length - end) * ITEM_HEIGHT);
});

function onTreeScroll(e: Event) {
  const el = e.target as HTMLElement;
  scrollTop.value = el.scrollTop;
  containerHeight.value = el.clientHeight;
}

// Immediately flush debounce when resetting search (e.g. DB switch, connection change)
function resetSearchImmediate() {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = null;
  }
  cascade.searchQuery = "";
  cascade.debouncedSearchQuery = "";
}

function selectDb(db: number) {
  currentDb.value = db;
  showDbDropdown.value = false;
  handleDbChange();
}

// Sync currentDb when active connection changes
const activeConn = computed(() => connStore.activeConnection);
watch(activeConn, (conn) => {
  if (conn) currentDb.value = conn.db;
}, { immediate: true });

// Sync currentDb when DB is switched externally (e.g. from sidebar)
watch(
  () => connStore.activeConnection?.db,
  (db) => {
    if (db !== undefined && db !== currentDb.value) {
      currentDb.value = db;
    }
  }
);

// Watch for connection loss
watch(
  () => connStore.activeConnection?.status,
  (newStatus, oldStatus) => {
    if (oldStatus === "connected" && newStatus !== "connected") {
      connectionLost.value = true;
    }
    if (newStatus === "connected") {
      connectionLost.value = false;
    }
  }
);

async function handleReconnect() {
  const id = connStore.activeConnectionId;
  if (!id) return;
  connectionLost.value = false;
  const ok = await connStore.connect(id);
  if (ok) {
    await cascade.refreshKeys(true);
  } else {
    connectionLost.value = true;
  }
}

async function handleDbChange() {
  if (switchingDb.value) return;
  const db = currentDb.value;
  switchingDb.value = true;
  try {
    await connStore.switchDb(db);
    cascade.selectedKey = null;
    resetSearchImmediate();
    cascade.typeFilter = "all";
    detail.clearDetail();
    await cascade.refreshKeys(true);
  } catch (e) {
    console.error("DB switch failed:", e);
  } finally {
    switchingDb.value = false;
  }
}

const typeColors: Record<RedisDataType, string> = {
  string: "bg-type-string/10 text-type-string",
  hash: "bg-type-hash/10 text-type-hash",
  list: "bg-type-list/10 text-type-list",
  set: "bg-type-set/10 text-type-set",
  zset: "bg-type-zset/10 text-type-zset",
};

const typeIcons: Record<RedisDataType, any> = {
  string: Type, hash: Hash, list: List, set: CircleDot, zset: BarChart3,
};

function formatTtl(ttl: number): string {
  if (ttl === -1) return t("detail.noExpiry");
  if (ttl === -2) return "N/A";
  if (ttl < 60) return `${ttl}s`;
  if (ttl < 3600) return `${Math.floor(ttl / 60)}m`;
  if (ttl < 86400) return `${Math.floor(ttl / 3600)}h`;
  return `${Math.floor(ttl / 86400)}d`;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / 1048576).toFixed(1)}M`;
}

function handleSelect(node: any) {
  if (node.key) cascade.selectKey(node.key.key);
  else cascade.toggleNode(node);
}

function copyKey(key: string) {
  navigator.clipboard.writeText(key);
}

async function deleteKey() {
  const key = detail.currentKey?.key;
  if (!key) return;
  const confirmed = window.confirm(t("browser.confirmDelete", { key }));
  if (!confirmed) return;
  try {
    await cascade.deleteKey(key);
    detail.clearDetail();
  } catch (e) {
    console.error("Delete key failed:", e);
  }
}

// Cell value popup for truncated content
const cellPopup = ref({ show: false, content: '', x: 0, y: 0, title: '' });

function showCellPopup(e: MouseEvent, content: string, title: string) {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  cellPopup.value = {
    show: true,
    content,
    title,
    x: Math.min(e.clientX + 8, vw - 420),
    y: Math.min(e.clientY + 8, vh - 260),
  };
}

function closeCellPopup() {
  cellPopup.value = { show: false, content: '', x: 0, y: 0, title: '' };
}

// Auto-load keys when connection changes or page mounts
watch(
  () => connStore.activeConnectionId,
  (id) => {
    if (id) {
      cascade.selectedKey = null;
      resetSearchImmediate();
      cascade.typeFilter = "all";
      detail.clearDetail();
      cascade.refreshKeys(true);
    }
  }
);

onMounted(() => {
  if (connStore.activeConnectionId) {
    cascade.refreshKeys();
  }
});
</script>

<template>
  <div class="h-full flex flex-col">
    <!-- Connection Lost Banner -->
    <div
      v-if="connectionLost"
      class="px-4 py-2.5 bg-danger/5 border-b border-danger/20 flex items-center gap-3 shrink-0"
    >
      <AlertTriangle :size="16" class="text-danger shrink-0" />
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium text-danger">{{ t("connection.connectionLost") }}</p>
        <p class="text-xs text-text-muted mt-0.5">{{ t("connection.connectionLostDesc") }}</p>
      </div>
      <button
        @click="handleReconnect"
        class="inline-flex items-center gap-1.5 px-3 py-1.5 bg-danger/10 text-danger rounded-lg text-xs font-medium hover:bg-danger/20 transition-colors shrink-0"
      >
        <Wifi :size="12" />
        {{ t("connection.reconnect") }}
      </button>
      <button @click="connectionLost = false" class="shrink-0 text-text-muted hover:text-text-primary">
        <X :size="14" />
      </button>
    </div>

    <div class="flex-1 flex min-h-0">
    <!-- Left Panel: Key Tree -->
    <div class="w-72 border-r border-border flex flex-col bg-white shrink-0">
      <!-- DB Selector -->
      <div class="px-3 pt-3 pb-2 border-b border-border-light">
        <div class="flex items-center gap-2">
          <Database :size="13" class="text-redis shrink-0" />
          <div class="relative flex-1">
            <button
              @click="showDbDropdown = !showDbDropdown"
              :disabled="switchingDb"
              class="w-full flex items-center justify-between px-2 py-1.5 text-xs font-mono font-semibold bg-bg-primary border border-border rounded-lg hover:border-redis/40 focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 transition-colors disabled:opacity-50"
            >
              <span>DB{{ currentDb }}</span>
              <div class="flex items-center gap-1">
                <RefreshCw v-if="switchingDb" :size="11" class="animate-spin text-text-muted" />
                <ChevronDown :size="12" class="text-text-muted transition-transform" :class="showDbDropdown ? 'rotate-180' : ''" />
              </div>
            </button>
            <!-- Backdrop -->
            <div v-if="showDbDropdown" class="fixed inset-0 z-40" @click="showDbDropdown = false" />
            <!-- Dropdown panel -->
            <div
              v-if="showDbDropdown"
              class="absolute top-full left-0 right-0 mt-1 bg-white border border-border rounded-lg shadow-lg py-1 z-50 max-h-52 overflow-y-auto"
            >
              <div class="px-2.5 py-1 border-b border-border-light mb-0.5">
                <span class="text-[9px] font-semibold text-text-muted uppercase tracking-wider">Database</span>
              </div>
              <button
                v-for="n in 16"
                :key="n - 1"
                @click="selectDb(n - 1)"
                class="w-full flex items-center justify-between px-2.5 py-1.5 text-xs font-mono transition-colors"
                :class="currentDb === n - 1
                  ? 'text-redis font-semibold bg-redis/5'
                  : 'text-text-secondary font-medium hover:bg-bg-hover hover:text-text-primary'"
              >
                <span>DB{{ n - 1 }}</span>
                <Check v-if="currentDb === n - 1" :size="11" class="text-redis" />
              </button>
            </div>
          </div>
        </div>
      </div>
      <div class="p-3 space-y-2 border-b border-border-light">
        <div class="relative">
          <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input v-model="cascade.searchQuery" :placeholder="t('browser.search')"
            class="w-full pl-8 pr-3 py-1.5 text-xs bg-bg-primary border border-border rounded-lg focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20" />
        </div>
        <div class="flex items-center gap-2">
          <select v-model="cascade.typeFilter" class="flex-1 px-2 py-1.5 text-xs bg-bg-primary border border-border rounded-lg focus:outline-none focus:border-redis">
            <option value="all">{{ t("browser.allTypes") }}</option>
            <option value="string">String</option>
            <option value="hash">Hash</option>
            <option value="list">List</option>
            <option value="set">Set</option>
            <option value="zset">ZSet</option>
          </select>
          <button @click="cascade.refreshKeys()" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover transition-colors">
            <RefreshCw :size="14" :class="cascade.loading ? 'animate-spin' : ''" class="text-text-muted" />
          </button>
        </div>
        <div class="text-[11px] text-text-muted">
          {{ cascade.totalKeyCount > cascade.loadedCount
            ? t("browser.keyCountWithTotal", { loaded: cascade.loadedCount, total: cascade.totalKeyCount })
            : t("browser.keyCount", { count: cascade.keyCount }) }}
        </div>
      </div>

      <div ref="treeScrollRef" class="flex-1 overflow-y-auto py-1" @scroll="onTreeScroll">
        <div v-if="cascade.filteredKeys.length === 0 && !cascade.loading" class="flex flex-col items-center py-8 text-text-muted">
          <Search :size="24" class="mb-2 opacity-40" />
          <span class="text-xs">{{ t("browser.noKeys") }}</span>
        </div>
        <div v-else :style="{ paddingTop: topPadding + 'px', paddingBottom: bottomPadding + 'px' }">
          <KeyTreeItem
            v-for="item in visibleItems"
            :key="item.node.fullPath"
            :node="item.node"
            :depth="item.depth"
            @select="handleSelect"
          />
          <!-- Load more button: in document flow, right after last rendered item -->
          <div v-if="cascade.hasMore" class="px-3 py-2">
            <button
              @click="cascade.loadMoreKeys()"
              :disabled="cascade.loading"
              class="w-full py-1.5 text-[11px] font-medium text-redis border border-dashed border-redis/30 rounded-lg hover:bg-redis/5 transition-colors disabled:opacity-50"
            >
              {{ cascade.loading ? "..." : t("browser.loadMore") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Center Panel: Key Detail -->
    <div class="flex-1 flex flex-col min-w-0">
      <template v-if="detail.currentDetail">
        <div class="px-4 py-3 border-b border-border-light flex items-center justify-between bg-white">
          <div class="flex items-center gap-2 min-w-0">
            <component :is="typeIcons[detail.currentKey!.type]" :size="16" :class="`text-type-${detail.currentKey!.type}`" />
            <span class="text-sm font-medium text-text-primary truncate">{{ detail.currentKey!.key }}</span>
            <span class="badge" :class="typeColors[detail.currentKey!.type]">{{ detail.currentKey!.type }}</span>
          </div>
          <div class="flex items-center gap-1.5 shrink-0">
            <button @click="copyKey(detail.currentKey!.key)" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover" :title="t('browser.copyKey')">
              <Copy :size="13" class="text-text-muted" />
            </button>
            <button @click="detail.refresh()" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover" :title="t('browser.refresh')">
              <RefreshCw :size="13" class="text-text-muted" />
            </button>
            <button @click="deleteKey" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-danger/10" :title="t('browser.deleteKey')">
              <Trash2 :size="13" class="text-danger" />
            </button>
          </div>
        </div>

        <div class="flex-1 overflow-auto p-4 flex flex-col min-h-0">
          <!-- String -->
          <div v-if="detail.currentValue?.type === 'string'" class="flex flex-col min-h-0">
            <label class="text-xs font-medium text-text-secondary mb-3 shrink-0">{{ t("detail.value") }}</label>
            <textarea :value="(detail.currentValue as any).value" readonly
              class="flex-1 w-full px-4 py-3 text-sm font-mono bg-bg-primary border border-border rounded-lg resize-none focus:outline-none focus:border-redis min-h-[200px]" />
          </div>

          <!-- Hash -->
          <div v-else-if="detail.currentValue?.type === 'hash'" class="flex flex-col min-h-0">
            <label class="text-xs font-medium text-text-secondary mb-3 shrink-0">{{ t("detail.value") }} ({{ (detail.currentValue as any).fields.length }} fields)</label>
            <div class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-1/3">{{ t("detail.field") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.value") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(f, i) in (detail.currentValue as any).fields" :key="f.field" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 font-mono text-xs text-text-primary font-medium">{{ f.field }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-secondary truncate max-w-0" @click="showCellPopup($event, f.value, f.field)"><span class="truncate block cursor-pointer">{{ f.value }}</span></td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>

          <!-- List -->
          <div v-else-if="detail.currentValue?.type === 'list'" class="flex flex-col min-h-0">
            <label class="text-xs font-medium text-text-secondary mb-3 shrink-0">{{ t("detail.value") }} ({{ (detail.currentValue as any).items.length }} items)</label>
            <div class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-16">{{ t("detail.index") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.value") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(item, i) in (detail.currentValue as any).items" :key="i" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs text-text-muted font-mono">{{ i }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary truncate max-w-0" @click="showCellPopup($event, item, `Index ${i}`)"><span class="truncate block cursor-pointer">{{ item }}</span></td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>

          <!-- Set -->
          <div v-else-if="detail.currentValue?.type === 'set'" class="flex flex-col min-h-0">
            <label class="text-xs font-medium text-text-secondary mb-3 shrink-0">{{ t("detail.value") }} ({{ (detail.currentValue as any).members.length }} members)</label>
            <div class="flex-1 min-h-0 overflow-y-auto space-y-1">
              <div v-for="(m, i) in (detail.currentValue as any).members" :key="m"
                class="px-3 py-2 text-xs font-mono bg-bg-primary border border-border-light rounded-lg flex items-center gap-2">
                <span class="text-text-muted w-6 text-right shrink-0">{{ i + 1 }}</span>
                <span class="text-text-primary truncate cursor-pointer" @click="showCellPopup($event, m, `Member ${i + 1}`)">{{ m }}</span>
              </div>
            </div>
          </div>

          <!-- ZSet -->
          <div v-else-if="detail.currentValue?.type === 'zset'" class="flex flex-col min-h-0">
            <label class="text-xs font-medium text-text-secondary mb-3 shrink-0">{{ t("detail.value") }} ({{ (detail.currentValue as any).members.length }} members)</label>
            <div class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-24">{{ t("detail.score") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.member") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(m, i) in (detail.currentValue as any).members" :key="m.member" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs font-mono text-redis font-semibold">{{ m.score.toLocaleString() }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary truncate max-w-0" @click="showCellPopup($event, m.member, `Score: ${m.score}`)"><span class="truncate block cursor-pointer">{{ m.member }}</span></td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div v-else class="flex-1 flex flex-col items-center justify-center text-text-muted">
        <Tag :size="36" class="mb-3 opacity-30" />
        <p class="text-sm">{{ t("browser.selectKey") }}</p>
      </div>
    </div>

    <!-- Right Panel: TTL Gauge + Meta -->
    <div class="w-56 border-l border-border bg-white p-4 flex flex-col gap-4 shrink-0" v-if="detail.currentDetail">
      <div class="text-center">
        <p class="text-xs font-medium text-text-secondary mb-3">{{ t("detail.ttl") }}</p>
        <TtlGauge :ttl-remaining="detail.ttlRemaining" :ttl-total="detail.ttlTotal" />
        <p class="text-xs text-text-muted mt-2">{{ formatTtl(detail.ttlRemaining) }}</p>
      </div>
      <div class="divider" />
      <div class="space-y-3">
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.type") }}</span>
          <span class="badge text-[10px]" :class="typeColors[detail.currentKey!.type]">{{ detail.currentKey!.type.toUpperCase() }}</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.size") }}</span>
          <span class="text-text-primary font-medium">{{ formatSize(detail.currentKey!.size) }}</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.encoding") }}</span>
          <span class="text-text-primary font-mono">{{ (detail.currentValue as any).encoding }}</span>
        </div>
      </div>
    </div>
    </div>

    <!-- Cell value popup (Teleport to body to avoid overflow clipping) -->
    <Teleport to="body">
      <div v-if="cellPopup.show" class="fixed inset-0 z-[9990]" @click="closeCellPopup" />
      <div
        v-if="cellPopup.show"
        class="fixed z-[9999] w-96 max-h-64 bg-white border border-border rounded-xl shadow-2xl flex flex-col overflow-hidden"
        :style="{ left: cellPopup.x + 'px', top: cellPopup.y + 'px' }"
      >
        <div class="flex items-center justify-between px-3 py-2 border-b border-border-light bg-bg-primary shrink-0">
          <span class="text-xs font-semibold text-text-secondary truncate">{{ cellPopup.title }}</span>
          <button @click="closeCellPopup" class="text-text-muted hover:text-text-primary shrink-0 ml-2">
            <X :size="12" />
          </button>
        </div>
        <div class="px-3 py-2.5 text-xs font-mono text-text-primary overflow-auto whitespace-pre-wrap break-all flex-1">
          {{ cellPopup.content }}
        </div>
      </div>
    </Teleport>
  </div>
</template>
