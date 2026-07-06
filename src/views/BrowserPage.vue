<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { watch, onMounted, ref, computed } from "vue";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { useConnectionStore } from "@/stores/connectionStore";
import type { RedisDataType } from "@/types";
import KeyTreeItem from "@/components/cascade/KeyTreeItem.vue";
import TtlGauge from "@/components/charts/TtlGauge.vue";
import {
  Search, RefreshCw, Trash2, Copy, Tag, Database,
  Type, Hash, List, CircleDot, BarChart3,
} from "lucide-vue-next";

const { t } = useI18n();
const cascade = useCascadeStore();
const detail = useDetailStore();
const connStore = useConnectionStore();

const currentDb = ref(0);
const switchingDb = ref(false);

// Sync currentDb when active connection changes
const activeConn = computed(() => connStore.activeConnection);
watch(activeConn, (conn) => {
  if (conn) currentDb.value = conn.db;
}, { immediate: true });

async function handleDbChange() {
  if (switchingDb.value) return;
  const db = currentDb.value;
  switchingDb.value = true;
  try {
    await connStore.switchDb(db);
    cascade.selectedKey = null;
    cascade.searchQuery = "";
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

// Auto-load keys when connection changes or page mounts
watch(
  () => connStore.activeConnectionId,
  (id) => {
    if (id) cascade.refreshKeys();
  }
);

onMounted(() => {
  if (connStore.activeConnectionId) {
    cascade.refreshKeys();
  }
});
</script>

<template>
  <div class="h-full flex">
    <!-- Left Panel: Key Tree -->
    <div class="w-72 border-r border-border flex flex-col bg-white shrink-0">
      <!-- DB Selector -->
      <div class="px-3 pt-3 pb-2 border-b border-border-light">
        <div class="flex items-center gap-2">
          <Database :size="13" class="text-redis shrink-0" />
          <select
            :value="currentDb"
            @change="(e) => { currentDb = Number((e.target as HTMLSelectElement).value); handleDbChange(); }"
            :disabled="switchingDb"
            class="flex-1 px-2 py-1 text-xs font-mono font-semibold bg-bg-primary border border-border rounded-lg focus:outline-none focus:border-redis disabled:opacity-50"
          >
            <option v-for="n in 16" :key="n - 1" :value="n - 1">DB{{ n - 1 }}</option>
          </select>
          <RefreshCw v-if="switchingDb" :size="12" class="animate-spin text-text-muted shrink-0" />
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
        <div class="text-[11px] text-text-muted">{{ t("browser.keyCount", { count: cascade.keyCount }) }}</div>
      </div>

      <div class="flex-1 overflow-y-auto py-1">
        <div v-if="cascade.filteredKeys.length === 0" class="flex flex-col items-center py-8 text-text-muted">
          <Search :size="24" class="mb-2 opacity-40" />
          <span class="text-xs">{{ t("browser.noKeys") }}</span>
        </div>
        <template v-else>
          <KeyTreeItem v-for="node in cascade.keyTree" :key="node.fullPath" :node="node" :depth="0" @select="handleSelect" />
        </template>
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
            <button class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-danger/10" :title="t('browser.deleteKey')">
              <Trash2 :size="13" class="text-danger" />
            </button>
          </div>
        </div>

        <div class="flex-1 overflow-auto p-4">
          <!-- String -->
          <div v-if="detail.currentValue?.type === 'string'" class="space-y-3">
            <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }}</label>
            <textarea :value="(detail.currentValue as any).value" readonly rows="12"
              class="w-full px-4 py-3 text-sm font-mono bg-bg-primary border border-border rounded-lg resize-none focus:outline-none focus:border-redis" />
          </div>

          <!-- Hash -->
          <div v-else-if="detail.currentValue?.type === 'hash'" class="space-y-3">
            <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ (detail.currentValue as any).fields.length }} fields)</label>
            <div class="border border-border rounded-lg overflow-hidden">
              <table class="w-full text-sm">
                <thead><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-1/3">{{ t("detail.field") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.value") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(f, i) in (detail.currentValue as any).fields" :key="f.field" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 font-mono text-xs text-text-primary font-medium">{{ f.field }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-secondary">{{ f.value }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- List -->
          <div v-else-if="detail.currentValue?.type === 'list'" class="space-y-3">
            <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ (detail.currentValue as any).items.length }} items)</label>
            <div class="border border-border rounded-lg overflow-hidden">
              <table class="w-full text-sm">
                <thead><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-16">{{ t("detail.index") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.value") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(item, i) in (detail.currentValue as any).items" :key="i" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs text-text-muted font-mono">{{ i }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary">{{ item }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- Set -->
          <div v-else-if="detail.currentValue?.type === 'set'" class="space-y-3">
            <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ (detail.currentValue as any).members.length }} members)</label>
            <div class="space-y-1">
              <div v-for="(m, i) in (detail.currentValue as any).members" :key="m"
                class="px-3 py-2 text-xs font-mono bg-bg-primary border border-border-light rounded-lg flex items-center gap-2">
                <span class="text-text-muted w-6 text-right">{{ i + 1 }}</span>
                <span class="text-text-primary">{{ m }}</span>
              </div>
            </div>
          </div>

          <!-- ZSet -->
          <div v-else-if="detail.currentValue?.type === 'zset'" class="space-y-3">
            <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ (detail.currentValue as any).members.length }} members)</label>
            <div class="border border-border rounded-lg overflow-hidden">
              <table class="w-full text-sm">
                <thead><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-24">{{ t("detail.score") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.member") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(m, i) in (detail.currentValue as any).members" :key="m.member" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs font-mono text-redis font-semibold">{{ m.score.toLocaleString() }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary">{{ m.member }}</td>
                  </tr>
                </tbody>
              </table>
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
</template>
