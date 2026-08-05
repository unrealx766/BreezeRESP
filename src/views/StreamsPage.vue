<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import {
  ListTree, RefreshCw, Search, Plus, Trash2, Check, Scissors, Users,
  MessageSquare, ListOrdered, ChevronDown, ChevronRight, TriangleAlert, Inbox,
} from "lucide-vue-next";
import { useStreamsStore } from "@/stores/streamsStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCapabilityStore } from "@/stores/capabilityStore";
import { tauriApi } from "@/services/tauriApi";
import { toast } from "@/utils/toast";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";

const { t } = useI18n();
const connStore = useConnectionStore();
const streamsStore = useStreamsStore();
const capStore = useCapabilityStore();

const isConnected = computed(() => connStore.activeConnection?.status === "connected");
const connId = computed(() => connStore.activeConnectionId ?? "");
const capability = computed(() => capStore.activeCapability);
const streamsSupported = computed(() => capability.value?.streamsSupported ?? true);
const extendedSupported = computed(() => capability.value?.streamExtendedSupported ?? false);

// Tabs
const activeTab = ref<"entries" | "groups" | "pending">("entries");

// Key list filter
const keyFilter = ref("");
const filteredKeys = computed(() => {
  const q = keyFilter.value.trim().toLowerCase();
  if (!q) return streamsStore.streamKeys;
  return streamsStore.streamKeys.filter((k) => k.toLowerCase().includes(q));
});

// Entries tab state
const rangeStart = ref("");
const rangeEnd = ref("");
const entryCount = ref(100);
const selectedIds = ref<Set<string>>(new Set());
const expandedId = ref("");

// Pending tab state
const selectedPendingIds = ref<Set<string>>(new Set());

// Modals
const confirmDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null);
const showAddModal = ref(false);
const addEntryId = ref("");
const addFields = ref<Array<{ field: string; value: string }>>([{ field: "", value: "" }]);
const sending = ref(false);

const showTrimModal = ref(false);
const trimMaxLen = ref(1000);
const trimApproximate = ref(true);
const trimming = ref(false);

const showClaimModal = ref(false);
const claimConsumer = ref("");
const claimMinIdle = ref(0);
const claiming = ref(false);

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

async function refreshAll() {
  if (!connId.value) return;
  await capStore.fetchCapability(connId.value);
  if (!streamsSupported.value) return;
  await streamsStore.loadKeys(connId.value);
  if (streamsStore.selectedKey && streamsStore.streamKeys.includes(streamsStore.selectedKey)) {
    await selectKey(streamsStore.selectedKey);
  } else if (streamsStore.streamKeys.length > 0) {
    await selectKey(streamsStore.streamKeys[0]);
  } else {
    streamsStore.reset();
  }
}

async function selectKey(key: string) {
  streamsStore.selectedKey = key;
  selectedIds.value = new Set();
  selectedPendingIds.value = new Set();
  try {
    await streamsStore.loadInfo(connId.value, key);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
    return;
  }
  await Promise.all([
    loadEntries(),
    loadGroups().then(() => {
      const first = streamsStore.groups[0];
      if (first) {
        streamsStore.loadConsumers(connId.value, key, first.name);
        streamsStore.loadPending(connId.value, key, first.name);
      } else {
        streamsStore.consumers = [];
        streamsStore.pendingEntries = [];
      }
    }),
  ]);
}

async function loadEntries() {
  if (!streamsStore.selectedKey) return;
  try {
    await streamsStore.loadEntries(
      connId.value, streamsStore.selectedKey,
      rangeStart.value || undefined, rangeEnd.value || undefined, entryCount.value,
    );
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function loadGroups() {
  if (!streamsStore.selectedKey) return;
  try {
    await streamsStore.loadGroups(connId.value, streamsStore.selectedKey);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function selectGroup(group: string) {
  if (!streamsStore.selectedKey) return;
  try {
    await Promise.all([
      streamsStore.loadConsumers(connId.value, streamsStore.selectedKey, group),
      streamsStore.loadPending(connId.value, streamsStore.selectedKey, group),
    ]);
    selectedPendingIds.value = new Set();
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

// ---------------------------------------------------------------------------
// Entry selection helpers
// ---------------------------------------------------------------------------

function toggleId(id: string) {
  const next = new Set(selectedIds.value);
  if (next.has(id)) next.delete(id); else next.add(id);
  selectedIds.value = next;
}

function toggleAllIds() {
  if (selectedIds.value.size === streamsStore.entries.length) {
    selectedIds.value = new Set();
  } else {
    selectedIds.value = new Set(streamsStore.entries.map((e) => e.id));
  }
}

function togglePendingId(id: string) {
  const next = new Set(selectedPendingIds.value);
  if (next.has(id)) next.delete(id); else next.add(id);
  selectedPendingIds.value = next;
}

function toggleAllPending() {
  if (selectedPendingIds.value.size === streamsStore.pendingEntries.length) {
    selectedPendingIds.value = new Set();
  } else {
    selectedPendingIds.value = new Set(streamsStore.pendingEntries.map((p) => p.id));
  }
}

function fieldSummary(fields: Array<[string, string]>): string {
  if (fields.length === 0) return "-";
  const [f, v] = fields[0];
  const more = fields.length > 1 ? ` (+${fields.length - 1})` : "";
  return `${f}=${v}${more}`;
}

// ---------------------------------------------------------------------------
// Write / management operations
// ---------------------------------------------------------------------------

function openAddModal() {
  addEntryId.value = "";
  addFields.value = [{ field: "", value: "" }];
  showAddModal.value = true;
}

function addFieldRow() {
  addFields.value.push({ field: "", value: "" });
}

function removeFieldRow(idx: number) {
  addFields.value.splice(idx, 1);
}

async function submitAdd() {
  const fields = addFields.value
    .filter((r) => r.field.trim() !== "")
    .map((r) => [r.field.trim(), r.value] as [string, string]);
  if (fields.length === 0) return;
  sending.value = true;
  try {
    const id = await tauriApi.streams.addMessage(
      connId.value, streamsStore.selectedKey,
      addEntryId.value.trim() || null, fields,
    );
    showAddModal.value = false;
    toast.success(t("streams.addSuccess", { id }));
    await Promise.all([streamsStore.loadInfo(connId.value, streamsStore.selectedKey), loadEntries()]);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  } finally {
    sending.value = false;
  }
}

async function submitTrim() {
  trimming.value = true;
  try {
    const removed = await tauriApi.streams.trim(
      connId.value, streamsStore.selectedKey, trimMaxLen.value, trimApproximate.value,
    );
    showTrimModal.value = false;
    toast.success(t("streams.trimSuccess", { count: removed }));
    await Promise.all([streamsStore.loadInfo(connId.value, streamsStore.selectedKey), loadEntries()]);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  } finally {
    trimming.value = false;
  }
}

async function deleteSelectedEntries() {
  const ids = [...selectedIds.value];
  if (ids.length === 0) return;
  const ok = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("streams.deleteEntriesConfirm", { count: ids.length }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;
  try {
    const removed = await tauriApi.streams.deleteEntries(connId.value, streamsStore.selectedKey, ids);
    toast.success(t("streams.deleteSuccess", { count: removed }));
    selectedIds.value = new Set();
    await Promise.all([streamsStore.loadInfo(connId.value, streamsStore.selectedKey), loadEntries()]);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function deleteSingleEntry(id: string) {
  const ok = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("streams.deleteEntriesConfirm", { count: 1 }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;
  try {
    await tauriApi.streams.deleteEntries(connId.value, streamsStore.selectedKey, [id]);
    toast.success(t("streams.deleteSuccess", { count: 1 }));
    await Promise.all([streamsStore.loadInfo(connId.value, streamsStore.selectedKey), loadEntries()]);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function deleteGroup(group: string) {
  const ok = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("streams.deleteGroupConfirm", { name: group }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;
  try {
    await tauriApi.streams.deleteGroup(connId.value, streamsStore.selectedKey, group);
    toast.success(t("streams.groupDeleted"));
    if (streamsStore.selectedGroup === group) {
      streamsStore.selectedGroup = "";
      streamsStore.consumers = [];
      streamsStore.pendingEntries = [];
    }
    await loadGroups();
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function deleteConsumer(consumer: string) {
  const ok = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("streams.deleteConsumerConfirm", { name: consumer }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;
  try {
    await tauriApi.streams.deleteConsumer(connId.value, streamsStore.selectedKey, streamsStore.selectedGroup, consumer);
    toast.success(t("streams.consumerDeleted"));
    await selectGroup(streamsStore.selectedGroup);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function ackSelected() {
  const ids = [...selectedPendingIds.value];
  if (ids.length === 0 || !streamsStore.selectedGroup) return;
  try {
    const count = await tauriApi.streams.ack(connId.value, streamsStore.selectedKey, streamsStore.selectedGroup, ids);
    toast.success(t("streams.ackSuccess", { count }));
    selectedPendingIds.value = new Set();
    await selectGroup(streamsStore.selectedGroup);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

function openClaimModal() {
  if (selectedPendingIds.value.size === 0) return;
  claimConsumer.value = "";
  claimMinIdle.value = 0;
  showClaimModal.value = true;
}

async function submitClaim() {
  const consumer = claimConsumer.value.trim();
  if (!consumer) return;
  claiming.value = true;
  try {
    const claimed = await tauriApi.streams.claim(
      connId.value, streamsStore.selectedKey, streamsStore.selectedGroup,
      consumer, claimMinIdle.value, [...selectedPendingIds.value],
    );
    showClaimModal.value = false;
    toast.success(t("streams.claimSuccess", { count: claimed.length }));
    selectedPendingIds.value = new Set();
    await selectGroup(streamsStore.selectedGroup);
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  } finally {
    claiming.value = false;
  }
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

function formatIdle(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  if (ms < 3_600_000) return `${Math.floor(ms / 60_000)}m`;
  return `${(ms / 3_600_000).toFixed(1)}h`;
}

function formatNumber(n: number | null | undefined): string {
  if (n == null) return "-";
  return n.toLocaleString();
}

// Lifecycle
onMounted(() => {
  if (connId.value) refreshAll();
});

watch(connId, (id, old) => {
  if (id !== old) {
    streamsStore.reset();
    keyFilter.value = "";
    if (id) refreshAll();
  }
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-hidden min-w-[760px]">
    <!-- Header -->
    <div class="flex items-start justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <ListTree :size="20" class="text-redis" />
          {{ t("streams.title") }}
        </h2>
        <p class="text-sm text-text-muted mt-1">{{ t("streams.description") }}</p>
      </div>
      <button
        @click="refreshAll"
        :disabled="!isConnected || streamsStore.loadingKeys"
        class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
      >
        <RefreshCw :size="13" :class="streamsStore.loadingKeys ? 'animate-spin' : ''" />
        {{ t("streams.refresh") }}
      </button>
    </div>

    <!-- Not connected -->
    <div v-if="!isConnected" class="flex-1 flex items-center justify-center">
      <div class="text-center text-text-muted">
        <Inbox :size="40" class="mx-auto mb-3 opacity-40" />
        <p class="text-sm">{{ t("status.noConnection") }}</p>
      </div>
    </div>

    <!-- Version unsupported banner -->
    <div v-else-if="!streamsSupported" class="flex items-center gap-2 px-4 py-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-500 text-sm shrink-0">
      <TriangleAlert :size="16" class="shrink-0" />
      {{ t("streams.versionUnsupported", { version: capability?.redisVersion ?? "-" }) }}
    </div>

    <template v-else>
      <!-- Stats cards -->
      <div class="grid grid-cols-4 gap-3 mb-4 shrink-0">
        <div class="rounded-xl border border-border bg-bg-secondary px-4 py-3">
          <p class="text-[11px] text-text-muted mb-1">{{ t("streams.statStreams") }}</p>
          <p class="text-xl font-semibold text-text-primary">{{ streamsStore.streamKeys.length }}</p>
        </div>
        <div class="rounded-xl border border-border bg-bg-secondary px-4 py-3">
          <p class="text-[11px] text-text-muted mb-1">{{ t("streams.statEntries") }}</p>
          <p class="text-xl font-semibold text-text-primary">{{ formatNumber(streamsStore.streamInfo?.length) }}</p>
        </div>
        <div class="rounded-xl border border-border bg-bg-secondary px-4 py-3">
          <p class="text-[11px] text-text-muted mb-1">{{ t("streams.statGroups") }}</p>
          <p class="text-xl font-semibold text-text-primary">{{ formatNumber(streamsStore.streamInfo?.groups) }}</p>
        </div>
        <div class="rounded-xl border border-border bg-bg-secondary px-4 py-3">
          <p class="text-[11px] text-text-muted mb-1">{{ t("streams.statPending") }}</p>
          <p class="text-xl font-semibold text-warning">{{ formatNumber(streamsStore.pendingEntries.length) }}</p>
        </div>
      </div>

      <!-- Main: key list + tabs -->
      <div class="flex-1 flex gap-4 min-h-0">
        <!-- Left: stream keys -->
        <div class="w-60 shrink-0 flex flex-col rounded-xl border border-border bg-bg-secondary overflow-hidden">
          <div class="p-2 border-b border-border shrink-0">
            <div class="relative">
              <Search :size="13" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
              <input
                v-model="keyFilter"
                type="text"
                :placeholder="t('streams.keyFilterPlaceholder')"
                class="w-full h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
              />
            </div>
          </div>
          <div class="flex-1 overflow-y-auto p-1.5 space-y-0.5">
            <p v-if="filteredKeys.length === 0" class="text-xs text-text-muted text-center py-6">
              {{ streamsStore.loadingKeys ? t("common.loading") : t("streams.noStreams") }}
            </p>
            <button
              v-for="key in filteredKeys"
              :key="key"
              @click="selectKey(key)"
              class="w-full text-left px-2.5 py-1.5 rounded-lg text-xs font-mono truncate transition-colors"
              :class="streamsStore.selectedKey === key
                ? 'bg-redis/10 text-redis'
                : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'"
              :title="key"
            >
              {{ key }}
            </button>
          </div>
        </div>

        <!-- Right: detail tabs -->
        <div class="flex-1 min-w-0 flex flex-col rounded-xl border border-border bg-bg-secondary overflow-hidden">
          <template v-if="streamsStore.selectedKey">
            <!-- Tabs + actions -->
            <div class="flex items-center justify-between px-3 pt-2 border-b border-border shrink-0 gap-2 flex-wrap">
              <div class="flex items-center gap-1">
                <button
                  v-for="tab in (['entries', 'groups', 'pending'] as const)"
                  :key="tab"
                  @click="activeTab = tab"
                  class="px-3 py-1.5 text-xs rounded-t-lg border-b-2 transition-colors flex items-center gap-1.5"
                  :class="activeTab === tab
                    ? 'text-redis border-redis font-medium'
                    : 'text-text-secondary border-transparent hover:text-text-primary'"
                >
                  <MessageSquare v-if="tab === 'entries'" :size="13" />
                  <Users v-else-if="tab === 'groups'" :size="13" />
                  <ListOrdered v-else :size="13" />
                  {{ tab === "entries" ? t("streams.tabEntries") : tab === "groups" ? t("streams.tabGroups") : t("streams.tabPending") }}
                </button>
              </div>
              <div v-if="activeTab === 'entries'" class="flex items-center gap-1.5 pb-1.5">
                <button
                  @click="openAddModal"
                  class="h-6.5 px-2 text-[11px] rounded-md bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1"
                >
                  <Plus :size="12" />
                  {{ t("streams.addMessage") }}
                </button>
                <button
                  @click="showTrimModal = true"
                  class="h-6.5 px-2 text-[11px] rounded-md border border-border text-text-secondary hover:bg-bg-hover transition-colors flex items-center gap-1"
                >
                  <Scissors :size="12" />
                  {{ t("streams.trim") }}
                </button>
                <button
                  @click="deleteSelectedEntries"
                  :disabled="selectedIds.size === 0"
                  class="h-6.5 px-2 text-[11px] rounded-md border border-red-500/30 text-red-500 hover:bg-red-500/10 transition-colors flex items-center gap-1 disabled:opacity-40"
                >
                  <Trash2 :size="12" />
                  {{ t("streams.deleteSelected") }} ({{ selectedIds.size }})
                </button>
              </div>
              <div v-else-if="activeTab === 'pending'" class="flex items-center gap-1.5 pb-1.5">
                <select
                  :value="streamsStore.selectedGroup"
                  @change="selectGroup(($event.target as HTMLSelectElement).value)"
                  class="h-6.5 px-2 text-[11px] rounded-md border border-border bg-bg-primary text-text-primary focus:outline-none"
                >
                  <option v-for="g in streamsStore.groups" :key="g.name" :value="g.name">{{ g.name }}</option>
                </select>
                <button
                  @click="ackSelected"
                  :disabled="selectedPendingIds.size === 0"
                  class="h-6.5 px-2 text-[11px] rounded-md bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1 disabled:opacity-40"
                >
                  <Check :size="12" />
                  ACK ({{ selectedPendingIds.size }})
                </button>
                <button
                  @click="openClaimModal"
                  :disabled="selectedPendingIds.size === 0"
                  class="h-6.5 px-2 text-[11px] rounded-md border border-border text-text-secondary hover:bg-bg-hover transition-colors disabled:opacity-40"
                >
                  {{ t("streams.claim") }}
                </button>
              </div>
            </div>

            <!-- Entries tab -->
            <div v-if="activeTab === 'entries'" class="flex-1 min-h-0 flex flex-col">
              <!-- Range filter -->
              <div class="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0 flex-wrap">
                <input
                  v-model="rangeStart"
                  type="text"
                  :placeholder="t('streams.startId')"
                  class="w-40 h-6.5 px-2 text-[11px] font-mono rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
                />
                <span class="text-text-muted text-xs">~</span>
                <input
                  v-model="rangeEnd"
                  type="text"
                  :placeholder="t('streams.endId')"
                  class="w-40 h-6.5 px-2 text-[11px] font-mono rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
                />
                <select
                  v-model.number="entryCount"
                  class="h-6.5 px-2 text-[11px] rounded-md border border-border bg-bg-primary text-text-primary focus:outline-none"
                >
                  <option :value="50">50</option>
                  <option :value="100">100</option>
                  <option :value="200">200</option>
                  <option :value="500">500</option>
                </select>
                <button
                  @click="loadEntries"
                  :disabled="streamsStore.loadingEntries"
                  class="h-6.5 px-2.5 text-[11px] rounded-md border border-border text-text-secondary hover:bg-bg-hover transition-colors disabled:opacity-40"
                >
                  {{ t("streams.load") }}
                </button>
                <span class="ml-auto text-[11px] text-text-muted">
                  {{ t("streams.lastGeneratedId") }}:
                  <span class="font-mono">{{ streamsStore.streamInfo?.lastGeneratedId ?? "-" }}</span>
                </span>
              </div>

              <!-- Table -->
              <div class="flex-1 overflow-auto">
                <p v-if="streamsStore.entries.length === 0" class="text-xs text-text-muted text-center py-8">
                  {{ streamsStore.loadingEntries ? t("common.loading") : t("streams.noEntries") }}
                </p>
                <table v-else class="w-full text-xs">
                  <thead class="sticky top-0 bg-bg-secondary z-10">
                    <tr class="text-left text-text-muted border-b border-border">
                      <th class="px-3 py-2 w-8">
                        <input
                          type="checkbox"
                          :checked="selectedIds.size === streamsStore.entries.length && streamsStore.entries.length > 0"
                          @change="toggleAllIds"
                          class="accent-redis cursor-pointer"
                        />
                      </th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colId") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colFields") }}</th>
                      <th class="px-2 py-2 font-medium w-16">{{ t("streams.colAction") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <template v-for="entry in streamsStore.entries" :key="entry.id">
                      <tr
                        class="border-b border-border/50 hover:bg-bg-hover/50 cursor-pointer transition-colors"
                        @click="expandedId = expandedId === entry.id ? '' : entry.id"
                      >
                        <td class="px-3 py-1.5" @click.stop>
                          <input
                            type="checkbox"
                            :checked="selectedIds.has(entry.id)"
                            @change="toggleId(entry.id)"
                            class="accent-redis cursor-pointer"
                          />
                        </td>
                        <td class="px-2 py-1.5 font-mono text-redis/90 whitespace-nowrap">
                          <span class="inline-flex items-center gap-1">
                            <component :is="expandedId === entry.id ? ChevronDown : ChevronRight" :size="12" class="text-text-muted" />
                            {{ entry.id }}
                          </span>
                        </td>
                        <td class="px-2 py-1.5 text-text-secondary font-mono truncate max-w-0">
                          {{ fieldSummary(entry.fields) }}
                        </td>
                        <td class="px-2 py-1.5" @click.stop>
                          <button
                            @click="deleteSingleEntry(entry.id)"
                            class="p-1 rounded text-text-muted hover:text-red-500 hover:bg-red-500/10 transition-colors"
                            :title="t('common.delete')"
                          >
                            <Trash2 :size="13" />
                          </button>
                        </td>
                      </tr>
                      <tr v-if="expandedId === entry.id" class="bg-bg-primary/60">
                        <td :colspan="4" class="px-4 py-2">
                          <table class="w-full text-[11px] font-mono">
                            <tr v-for="[f, v] in entry.fields" :key="f" class="align-top">
                              <td class="py-0.5 pr-3 text-redis/80 whitespace-nowrap">{{ f }}</td>
                              <td class="py-0.5 text-text-secondary break-all">{{ v }}</td>
                            </tr>
                          </table>
                        </td>
                      </tr>
                    </template>
                  </tbody>
                </table>
              </div>
            </div>

            <!-- Groups tab -->
            <div v-else-if="activeTab === 'groups'" class="flex-1 min-h-0 overflow-auto p-3 space-y-4">
              <div>
                <p v-if="streamsStore.groups.length === 0" class="text-xs text-text-muted text-center py-8">
                  {{ streamsStore.loadingGroups ? t("common.loading") : t("streams.noGroups") }}
                </p>
                <table v-else class="w-full text-xs">
                  <thead>
                    <tr class="text-left text-text-muted border-b border-border">
                      <th class="px-2 py-2 font-medium">{{ t("streams.colGroup") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colConsumers") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colPendingCount") }}</th>
                      <th v-if="extendedSupported" class="px-2 py-2 font-medium">{{ t("streams.colLag") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colLastDelivered") }}</th>
                      <th class="px-2 py-2 font-medium w-16">{{ t("streams.colAction") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="g in streamsStore.groups"
                      :key="g.name"
                      class="border-b border-border/50 hover:bg-bg-hover/50 cursor-pointer transition-colors"
                      :class="streamsStore.selectedGroup === g.name ? 'bg-redis/5' : ''"
                      @click="selectGroup(g.name)"
                    >
                      <td class="px-2 py-1.5 font-mono text-text-primary">{{ g.name }}</td>
                      <td class="px-2 py-1.5 text-text-secondary">{{ g.consumers }}</td>
                      <td class="px-2 py-1.5" :class="g.pending > 0 ? 'text-warning' : 'text-text-secondary'">{{ g.pending }}</td>
                      <td v-if="extendedSupported" class="px-2 py-1.5 text-text-secondary">{{ g.lag ?? "-" }}</td>
                      <td class="px-2 py-1.5 font-mono text-text-muted">{{ g.lastDeliveredId }}</td>
                      <td class="px-2 py-1.5" @click.stop>
                        <button
                          @click="deleteGroup(g.name)"
                          class="p-1 rounded text-text-muted hover:text-red-500 hover:bg-red-500/10 transition-colors"
                          :title="t('common.delete')"
                        >
                          <Trash2 :size="13" />
                        </button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Consumers of the selected group -->
              <div v-if="streamsStore.selectedGroup">
                <p class="text-xs font-medium text-text-primary mb-2">
                  {{ t("streams.colConsumer") }} · <span class="font-mono text-redis">{{ streamsStore.selectedGroup }}</span>
                </p>
                <p v-if="streamsStore.consumers.length === 0" class="text-xs text-text-muted text-center py-4">
                  {{ streamsStore.loadingConsumers ? t("common.loading") : t("streams.noConsumers") }}
                </p>
                <table v-else class="w-full text-xs">
                  <thead>
                    <tr class="text-left text-text-muted border-b border-border">
                      <th class="px-2 py-2 font-medium">{{ t("streams.colConsumer") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colPendingCount") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colIdle") }}</th>
                      <th class="px-2 py-2 font-medium w-16">{{ t("streams.colAction") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="c in streamsStore.consumers" :key="c.name" class="border-b border-border/50">
                      <td class="px-2 py-1.5 font-mono text-text-primary">{{ c.name }}</td>
                      <td class="px-2 py-1.5 text-text-secondary">{{ c.pending }}</td>
                      <td class="px-2 py-1.5 text-text-muted">{{ formatIdle(c.idleMs) }}</td>
                      <td class="px-2 py-1.5">
                        <button
                          @click="deleteConsumer(c.name)"
                          class="p-1 rounded text-text-muted hover:text-red-500 hover:bg-red-500/10 transition-colors"
                          :title="t('common.delete')"
                        >
                          <Trash2 :size="13" />
                        </button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            <!-- Pending tab -->
            <div v-else class="flex-1 min-h-0 overflow-auto">
              <p v-if="streamsStore.pendingEntries.length === 0" class="text-xs text-text-muted text-center py-8">
                {{ streamsStore.loadingPending ? t("common.loading") : t("streams.noPending") }}
              </p>
              <table v-else class="w-full text-xs">
                <thead class="sticky top-0 bg-bg-secondary z-10">
                  <tr class="text-left text-text-muted border-b border-border">
                    <th class="px-3 py-2 w-8">
                      <input
                        type="checkbox"
                        :checked="selectedPendingIds.size === streamsStore.pendingEntries.length && streamsStore.pendingEntries.length > 0"
                        @change="toggleAllPending"
                        class="accent-redis cursor-pointer"
                      />
                    </th>
                    <th class="px-2 py-2 font-medium">{{ t("streams.colId") }}</th>
                    <th class="px-2 py-2 font-medium">{{ t("streams.colConsumer") }}</th>
                    <th class="px-2 py-2 font-medium">{{ t("streams.colIdle") }}</th>
                    <th class="px-2 py-2 font-medium">{{ t("streams.colDeliveredCount") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="p in streamsStore.pendingEntries" :key="p.id" class="border-b border-border/50 hover:bg-bg-hover/50 transition-colors">
                    <td class="px-3 py-1.5">
                      <input
                        type="checkbox"
                        :checked="selectedPendingIds.has(p.id)"
                        @change="togglePendingId(p.id)"
                        class="accent-redis cursor-pointer"
                      />
                    </td>
                    <td class="px-2 py-1.5 font-mono text-redis/90">{{ p.id }}</td>
                    <td class="px-2 py-1.5 font-mono text-text-secondary">{{ p.consumer }}</td>
                    <td class="px-2 py-1.5 text-text-muted">{{ formatIdle(p.idleMs) }}</td>
                    <td class="px-2 py-1.5 text-text-secondary">{{ p.deliveredCount }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </template>

          <!-- No stream selected -->
          <div v-else class="flex-1 flex items-center justify-center">
            <p class="text-sm text-text-muted">{{ t("streams.selectKeyHint") }}</p>
          </div>
        </div>
      </div>
    </template>

    <!-- XADD modal -->
    <Teleport to="body">
      <div v-if="showAddModal" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/40" @click="showAddModal = false" />
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[480px] max-w-[90vw] p-5">
          <h3 class="text-sm font-semibold text-text-primary mb-3 flex items-center gap-2">
            <Plus :size="15" class="text-redis" />
            {{ t("streams.addTitle") }} · <span class="font-mono text-redis">{{ streamsStore.selectedKey }}</span>
          </h3>
          <input
            v-model="addEntryId"
            type="text"
            :placeholder="t('streams.entryIdOptional')"
            class="w-full h-8 px-2.5 mb-3 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
          />
          <div class="space-y-2 max-h-60 overflow-y-auto mb-3">
            <div v-for="(row, idx) in addFields" :key="idx" class="flex items-center gap-2">
              <input
                v-model="row.field"
                type="text"
                :placeholder="t('streams.fieldName')"
                class="flex-1 h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
              />
              <input
                v-model="row.value"
                type="text"
                :placeholder="t('streams.fieldValue')"
                class="flex-[2] h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
              />
              <button
                @click="removeFieldRow(idx)"
                :disabled="addFields.length === 1"
                class="p-1 rounded text-text-muted hover:text-red-500 transition-colors disabled:opacity-30"
              >
                <Trash2 :size="13" />
              </button>
            </div>
          </div>
          <button
            @click="addFieldRow"
            class="text-xs text-redis hover:underline mb-4 flex items-center gap-1"
          >
            <Plus :size="12" />
            {{ t("streams.addFieldRow") }}
          </button>
          <div class="flex justify-end gap-2">
            <button
              @click="showAddModal = false"
              class="h-8 px-4 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
            <button
              @click="submitAdd"
              :disabled="sending || addFields.every((r) => !r.field.trim())"
              class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors disabled:opacity-50"
            >
              {{ t("streams.send") }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- XTRIM modal -->
    <Teleport to="body">
      <div v-if="showTrimModal" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/40" @click="showTrimModal = false" />
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[380px] max-w-[90vw] p-5">
          <h3 class="text-sm font-semibold text-text-primary mb-3 flex items-center gap-2">
            <Scissors :size="15" class="text-redis" />
            {{ t("streams.trimTitle") }}
          </h3>
          <label class="block text-xs text-text-secondary mb-1">{{ t("streams.maxLen") }}</label>
          <input
            v-model.number="trimMaxLen"
            type="number"
            min="0"
            class="w-full h-8 px-2.5 mb-3 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none focus:border-redis/50"
          />
          <label class="flex items-center gap-2 text-xs text-text-secondary mb-4 cursor-pointer">
            <input v-model="trimApproximate" type="checkbox" class="accent-redis" />
            {{ t("streams.approximate") }}
          </label>
          <div class="flex justify-end gap-2">
            <button
              @click="showTrimModal = false"
              class="h-8 px-4 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
            <button
              @click="submitTrim"
              :disabled="trimming || trimMaxLen < 0"
              class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors disabled:opacity-50"
            >
              {{ t("common.confirm") }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- XCLAIM modal -->
    <Teleport to="body">
      <div v-if="showClaimModal" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/40" @click="showClaimModal = false" />
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[380px] max-w-[90vw] p-5">
          <h3 class="text-sm font-semibold text-text-primary mb-3">
            {{ t("streams.claimTitle") }} ({{ selectedPendingIds.size }})
          </h3>
          <label class="block text-xs text-text-secondary mb-1">{{ t("streams.claimConsumer") }}</label>
          <input
            v-model="claimConsumer"
            type="text"
            class="w-full h-8 px-2.5 mb-3 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none focus:border-redis/50"
          />
          <label class="block text-xs text-text-secondary mb-1">{{ t("streams.claimMinIdle") }}</label>
          <input
            v-model.number="claimMinIdle"
            type="number"
            min="0"
            class="w-full h-8 px-2.5 mb-4 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none focus:border-redis/50"
          />
          <div class="flex justify-end gap-2">
            <button
              @click="showClaimModal = false"
              class="h-8 px-4 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
            <button
              @click="submitClaim"
              :disabled="claiming || !claimConsumer.trim()"
              class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors disabled:opacity-50"
            >
              {{ t("common.confirm") }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <ConfirmDialog ref="confirmDialog" />
  </div>
</template>
