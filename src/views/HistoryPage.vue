<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { History, Search, Trash2, Copy, CheckCircle, XCircle } from "lucide-vue-next";
import { useHistoryStore } from "@/stores/historyStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";

const { t } = useI18n();
const historyStore = useHistoryStore();
const connStore = useConnectionStore();

const searchQuery = ref("");
const selectedDb = ref<number | undefined>(undefined);
const showConfirmClear = ref(false);
const confirmRef = ref<HTMLElement | null>(null);

// Reset DB filter when active connection changes
watch(() => connStore.activeConnectionId, () => {
  selectedDb.value = undefined;
  showConfirmClear.value = false;
});

// Click-outside to close confirm dropdown
function onClickOutside(e: MouseEvent) {
  if (showConfirmClear.value && confirmRef.value && !confirmRef.value.contains(e.target as Node)) {
    showConfirmClear.value = false;
  }
}
onMounted(() => document.addEventListener("click", onClickOutside));
onBeforeUnmount(() => document.removeEventListener("click", onClickOutside));

const activeDbList = computed(() => {
  const connId = connStore.activeConnectionId;
  if (!connId) return [];
  return historyStore.getDbsForConnection(connId);
});

const filteredItems = computed(() => {
  const connId = connStore.activeConnectionId;
  if (!connId) return [];
  let items = historyStore.getFiltered(connId, selectedDb.value);
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.trim().toLowerCase();
    items = items.filter((h) => h.command.toLowerCase().includes(q));
  }
  return items;
});

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function formatDate(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Split a combined command string into sub-commands */
function splitCommands(cmd: string): string[] {
  return cmd.split(/ \+ /);
}

/** Extract the Redis command verb (first word) from a command string */
function cmdVerb(cmd: string): string {
  return cmd.split(/\s/)[0] ?? cmd;
}

/** Extract the args part (everything after the first word) */
function cmdArgs(cmd: string): string {
  const idx = cmd.indexOf(" ");
  return idx >= 0 ? cmd.slice(idx + 1) : "";
}

function sourceLabel(source: string): string {
  const map: Record<string, string> = {
    browser: t("history.sourceBrowser"),
    pipeline: t("history.sourcePipeline"),
    sandbox: t("history.sourceSandbox"),
  };
  return map[source] ?? source;
}

/** Format duration in ms to human-readable string */
function formatDuration(ms: number | undefined): string {
  if (ms == null) return "";
  if (ms < 10000) return `${ms}ms`;
  const totalSec = ms / 1000;
  if (totalSec < 60) return `${totalSec.toFixed(1)}s`;
  const totalMin = Math.floor(totalSec / 60);
  const sec = Math.round(totalSec % 60);
  if (totalMin < 60) return `${totalMin}m${sec}s`;
  const hours = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  if (hours < 24) return `${hours}h${min}m`;
  const days = Math.floor(hours / 24);
  const h = hours % 24;
  return `${days}d${h}h`;
}

function sourceColor(source: string): string {
  const map: Record<string, string> = {
    browser: "bg-blue-500/15 text-blue-400 border-blue-500/30",
    pipeline: "bg-purple-500/15 text-purple-400 border-purple-500/30",
    sandbox: "bg-amber-500/15 text-amber-400 border-amber-500/30",
  };
  return map[source] ?? "bg-gray-500/15 text-gray-400 border-gray-500/30";
}

/** Format command for copying: multi-commands become separate lines */
function formatCommandForCopy(cmd: string): string {
  const parts = splitCommands(cmd);
  return parts.join("\n");
}

async function copyCommand(cmd: string) {
  try {
    await navigator.clipboard.writeText(formatCommandForCopy(cmd));
    toast.success(t("common.copied"));
  } catch {
    toast.error(t("common.error"));
  }
}

function handleClear() {
  const connId = connStore.activeConnectionId;
  if (connId) {
    historyStore.clearHistory(connId);
  }
  showConfirmClear.value = false;
}

/** Group items by date for section headers (computed to avoid re-creating arrays on every render) */
const groupedItems = computed(() => {
  const items = filteredItems.value;
  const groups: Array<{ date: string; items: typeof items }> = [];
  let lastDate = "";
  for (const item of items) {
    const date = formatDate(item.timestamp);
    if (date !== lastDate) {
      groups.push({ date, items: [] });
      lastDate = date;
    }
    groups[groups.length - 1].items.push(item);
  }
  return groups;
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-auto min-w-[600px]">
    <!-- Header -->
    <div class="flex items-start justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <History :size="20" class="text-redis" />
          {{ t("history.title") }}
        </h2>
        <p v-if="filteredItems.length > 0" class="text-sm text-text-muted mt-1">
          {{ t("history.totalRecords", { count: filteredItems.length }) }}
        </p>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <!-- Search -->
        <div class="relative">
          <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            v-model="searchQuery"
            type="text"
            :placeholder="t('history.searchPlaceholder')"
            class="w-48 h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
          />
        </div>
        <!-- Clear button -->
        <div v-if="filteredItems.length > 0" class="relative" ref="confirmRef">
          <button
            @click="showConfirmClear = true"
            class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-danger/10 hover:text-danger hover:border-danger/30 transition-colors flex items-center gap-1"
          >
            <Trash2 :size="13" />
            {{ t("history.clearAll") }}
          </button>
          <!-- Confirm dropdown -->
          <div
            v-if="showConfirmClear"
            class="absolute right-0 top-full mt-1 z-50 w-52 p-2 rounded-lg border border-border bg-bg-secondary shadow-lg"
          >
            <p class="text-xs text-text-secondary mb-2">{{ t("history.clearConfirm") }}</p>
            <div class="flex gap-1.5 justify-end">
              <button
                @click="showConfirmClear = false"
                class="px-2 py-1 text-xs rounded border border-border text-text-secondary hover:bg-bg-hover transition-colors"
              >
                {{ t("common.cancel") }}
              </button>
              <button
                @click="handleClear"
                class="px-2 py-1 text-xs rounded bg-danger text-white hover:bg-danger/80 transition-colors"
              >
                {{ t("common.confirm") }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <History :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("history.noConnection") }}</p>
    </div>

    <!-- Empty state -->
    <div
      v-else-if="filteredItems.length === 0"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <History :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("history.noHistory") }}</p>
    </div>

    <!-- Content -->
    <template v-else>
      <!-- DB filter tabs -->
      <div v-if="activeDbList.length > 1" class="flex items-center gap-1.5 mb-3 flex-wrap">
        <button
          @click="selectedDb = undefined"
          class="px-2.5 py-1 text-xs rounded-md border transition-colors"
          :class="selectedDb === undefined
            ? 'bg-redis/10 text-redis border-redis/30'
            : 'border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary'"
        >
          {{ t("history.allDb") }}
        </button>
        <button
          v-for="db in activeDbList"
          :key="db"
          @click="selectedDb = db"
          class="px-2.5 py-1 text-xs rounded-md border transition-colors"
          :class="selectedDb === db
            ? 'bg-redis/10 text-redis border-redis/30'
            : 'border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary'"
        >
          DB{{ db }}
        </button>
      </div>

      <!-- Command list -->
      <div class="flex-1 overflow-y-auto space-y-3">
        <div v-for="group in groupedItems" :key="group.date">
          <!-- Date header -->
          <div class="flex items-center gap-2 mb-1.5">
            <span class="text-[11px] font-semibold text-text-muted uppercase tracking-wider">{{ group.date }}</span>
            <div class="flex-1 h-px bg-border"></div>
          </div>

          <!-- Items -->
          <div class="space-y-1">
            <div
              v-for="item in group.items"
              :key="item.id"
              class="group flex items-start gap-2.5 p-2 rounded-lg hover:bg-bg-secondary/60 transition-colors"
            >
              <!-- Status icon -->
              <div class="mt-0.5 shrink-0">
                <CheckCircle v-if="item.success" :size="14" class="text-success" />
                <XCircle v-else :size="14" class="text-danger" />
              </div>

              <!-- Content -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-0.5">
                  <!-- Time -->
                  <span class="text-[11px] font-mono text-text-muted">{{ formatTime(item.timestamp) }}</span>
                  <!-- Duration -->
                  <span v-if="item.durationMs != null" class="text-[10px] font-mono text-text-muted/70">
                    {{ formatDuration(item.durationMs) }}
                  </span>
                  <!-- DB badge -->
                  <span class="text-[10px] font-mono font-semibold text-redis/70 bg-redis/8 px-1.5 py-0.5 rounded">
                    DB{{ item.db }}
                  </span>
                  <!-- Source badge -->
                  <span
                    class="text-[10px] px-1.5 py-0.5 rounded border"
                    :class="sourceColor(item.source)"
                  >
                    {{ sourceLabel(item.source) }}
                  </span>
                </div>
                <!-- Command text -->
                <div class="flex items-start gap-1.5">
                  <div class="flex-1 min-w-0" :title="item.command">
                    <!-- Multi-command: stacked with left rail -->
                    <template v-if="item.command.includes(' + ')">
                      <div class="flex flex-col gap-0.5 pl-2 border-l-2 border-redis/20">
                        <div v-for="(sub, idx) in splitCommands(item.command)" :key="idx" class="flex items-baseline">
                          <span class="text-xs font-mono text-redis font-semibold shrink-0">{{ cmdVerb(sub) }}</span>
                          <span class="text-xs font-mono text-text-secondary truncate ml-1">{{ cmdArgs(sub) }}</span>
                        </div>
                      </div>
                    </template>
                    <!-- Single command: inline -->
                    <code v-else class="text-xs text-text-primary font-mono truncate block">
                      <span class="text-redis font-semibold">{{ cmdVerb(item.command) }}</span>
                      <span v-if="cmdArgs(item.command)" class="text-text-secondary ml-1">{{ cmdArgs(item.command) }}</span>
                    </code>
                  </div>
                  <!-- Copy button (visible on hover) -->
                  <button
                    @click="copyCommand(item.command)"
                    class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity p-0.5 rounded hover:bg-bg-hover mt-0.5"
                    :title="t('history.copyCommand')"
                  >
                    <Copy :size="12" class="text-text-muted" />
                  </button>
                </div>
                <!-- Error message -->
                <p v-if="!item.success && item.error" class="text-[11px] text-danger/80 mt-0.5 truncate" :title="item.error">
                  {{ item.error }}
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
