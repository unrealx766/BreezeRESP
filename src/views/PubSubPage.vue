<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { usePubsubStore } from "@/stores/pubsubStore";
import type { PubSubMessageItem } from "@/stores/pubsubStore";
import { toast } from "@/utils/toast";
import NumberedTextarea from "@/components/shared/NumberedTextarea.vue";
import SmartPayloadInspector from "@/components/shared/SmartPayloadInspector.vue";
import { Radio, Plus, Send, MessageSquare, RefreshCw, X, ArrowDown, History, Regex, Download, BellPlus, Bell } from "lucide-vue-next";

const { t } = useI18n();
const connStore = useConnectionStore();
const pubsubStore = usePubsubStore();
const isConnected = computed(() => connStore.activeConnection?.status === "connected");

// Subscriptions & received messages live in the store (keyed by connection id)
// so they persist across tab switches — this page unmounts on navigation.
const subscriptions = computed(() => pubsubStore.channelsOf(connStore.activeConnectionId));
const patterns = computed(() => pubsubStore.patternsOf(connStore.activeConnectionId));
const messages = computed(() => pubsubStore.messagesOf(connStore.activeConnectionId));

// A subscription entry, tagged so the UI can distinguish glob patterns from
// exact channels. Patterns are listed first.
interface SubItem {
  name: string;
  isPattern: boolean;
}
const allSubscriptions = computed<SubItem[]>(() => [
  ...patterns.value.map((name) => ({ name, isPattern: true })),
  ...subscriptions.value.map((name) => ({ name, isPattern: false })),
]);
const subscriptionCount = computed(() => subscriptions.value.length + patterns.value.length);

// Glob metacharacters imply a pattern subscription (PSUBSCRIBE).
const isPatternExpr = (s: string): boolean => /[*?[\]]/.test(s);

// Messages are stored newest-first; display oldest→newest (newest at bottom).
const displayMessages = computed(() => [...messages.value].reverse());

// Publish history (newest first) lives in the store, keyed by connection id.
const publishes = computed(() => pubsubStore.publishesOf(connStore.activeConnectionId));

// Scroll tracking for the message list.
const messagesContainerRef = ref<HTMLElement | null>(null);
// Whether the list is currently scrolled to the bottom (auto-follow mode).
const isAtBottom = ref(true);
// Count of messages received while the user is scrolled away from the bottom.
const newMessageCount = ref(0);

// Channel related state (available-channels list is ephemeral, re-loaded on demand)
const channelInput = ref("");
const channels = ref<string[]>([]);

// Publishing related state
const publishChannel = ref("");
const publishMessage = ref("");
const showChannelDropdown = ref(false);
const showPublishHistory = ref(false);

// Channel suggestions: merge subscribed channels + active channels (deduplicated)
const channelSuggestions = computed(() => {
  const all = new Set<string>([...subscriptions.value, ...channels.value]);
  return [...all].sort();
});

// Filtered suggestions based on current input
const filteredSuggestions = computed(() => {
  const q = publishChannel.value.trim().toLowerCase();
  if (!q) return channelSuggestions.value;
  return channelSuggestions.value.filter((c) => c.toLowerCase().includes(q));
});

function selectChannel(channel: string) {
  publishChannel.value = channel;
  showChannelDropdown.value = false;
}

function handleChannelBlur() {
  showChannelDropdown.value = false;
}

// Loading states
const loadingChannels = ref(false);
const subscribing = ref(false);
const publishing = ref(false);

// Whether we are actively listening to at least one channel or pattern
const isListening = computed(() => subscriptionCount.value > 0);

// Load channel list
const loadChannels = async (): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn) return;

  try {
    loadingChannels.value = true;
    channels.value = await tauriApi.pubsub.listChannels(conn.id);
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.loadChannelsFailed"));
  } finally {
    loadingChannels.value = false;
  }
};

// Subscribe to a channel
// Check if a channel is already subscribed
const isChannelSubscribed = (channel: string): boolean => {
  return subscriptions.value.includes(channel) || patterns.value.includes(channel);
};

// Subscribe directly from channel list
const handleSubscribeChannel = async (channel: string): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn) return;

  const isPattern = isPatternExpr(channel);
  const existing = isPattern ? patterns.value : subscriptions.value;
  if (existing.includes(channel)) return;

  try {
    subscribing.value = true;
    await pubsubStore.subscribe(conn.id, channel, isPattern);
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.subscribeFailed"));
  } finally {
    subscribing.value = false;
  }
};

const handleSubscribe = async (): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn || !channelInput.value.trim()) return;

  const channel = channelInput.value.trim();
  const isPattern = isPatternExpr(channel);
  const existing = isPattern ? patterns.value : subscriptions.value;
  if (existing.includes(channel)) return;

  try {
    subscribing.value = true;
    await pubsubStore.subscribe(conn.id, channel, isPattern);
    channelInput.value = "";
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.subscribeFailed"));
  } finally {
    subscribing.value = false;
  }
};

// Unsubscribe from a channel or pattern
const handleUnsubscribe = async (channel: string, isPattern = false): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn) return;

  try {
    subscribing.value = true;
    await pubsubStore.unsubscribe(conn.id, channel, isPattern);
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.unsubscribeFailed"));
  } finally {
    subscribing.value = false;
  }
};

// Unsubscribe from all channels
const handleUnsubscribeAll = async (): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn) return;

  try {
    subscribing.value = true;
    await pubsubStore.unsubscribeAll(conn.id);
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.unsubscribeFailed"));
  } finally {
    subscribing.value = false;
  }
};

// Publish a message to a channel
const handlePublish = async (): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn || !publishChannel.value.trim() || !publishMessage.value.trim()) return;

  const channel = publishChannel.value.trim();
  const message = publishMessage.value.trim();

  try {
    publishing.value = true;
    const numSubscribers = await tauriApi.pubsub.publish(conn.id, channel, message);

    // Record the publish (with delivery count) into the per-connection history.
    pubsubStore.addPublish(conn.id, { channel, message, count: numSubscribers, timestamp: Date.now() });

    toast.success(t("pubsub.publishSuccess", { channel, count: numSubscribers }));
    publishMessage.value = "";
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.publishFailed"));
  } finally {
    publishing.value = false;
  }
};

// Clear the publish history for the active connection
const clearPublishHistory = (): void => {
  const conn = connStore.activeConnection;
  if (conn) pubsubStore.clearPublishes(conn.id);
};

// Re-fill the publish form from a history entry
const reusePublish = (item: { channel: string; message: string }): void => {
  publishChannel.value = item.channel;
  publishMessage.value = item.message;
};

// Format timestamp (aligned with other pages: manual HH:mm:ss)
const formatTime = (timestamp: number): string => {
  const d = new Date(timestamp);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
};

// Clear all received messages for the active connection
const clearMessages = (): void => {
  const conn = connStore.activeConnection;
  if (conn) pubsubStore.clearMessages(conn.id);
  toast.info(t("pubsub.messagesCleared"));
};

// ─── Export Functions ────────────────────────────────────────────────

const showExportMenu = ref(false);
const exportMenuRef = ref<HTMLElement | null>(null);

// Close export menu when clicking outside
function handleClickOutside(event: MouseEvent) {
  if (exportMenuRef.value && !exportMenuRef.value.contains(event.target as Node)) {
    showExportMenu.value = false;
  }
}

onMounted(() => {
  document.addEventListener("click", handleClickOutside);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleClickOutside);
});

async function exportAsJson() {
  const conn = connStore.activeConnection;
  if (!conn) return;

  const filePath = await save({
    filters: [{ name: "JSON Files", extensions: ["json"] }],
    defaultPath: `pubsub-${conn.name}-${Date.now()}.json`,
  });

  if (!filePath) return;

  const data = messages.value.map((msg) => ({
    channel: msg.channel,
    message: msg.message,
    timestamp: msg.timestamp,
    pattern: msg.pattern || null,
  }));

  const json = JSON.stringify(data, null, 2);
  await writeTextFile(filePath, json);
  showExportMenu.value = false;
  toast.success(t("pubsub.payloadInspector.exportJsonSuccess"));
}

async function exportAsCsv() {
  const conn = connStore.activeConnection;
  if (!conn) return;

  const filePath = await save({
    filters: [{ name: "CSV Files", extensions: ["csv"] }],
    defaultPath: `pubsub-${conn.name}-${Date.now()}.csv`,
  });

  if (!filePath) return;

  const headers = ["Channel", "Message", "Timestamp", "Pattern"];
  const rows = messages.value.map((msg) => [
    escapeCsv(msg.channel),
    escapeCsv(msg.message),
    escapeCsv(new Date(msg.timestamp).toISOString()),
    escapeCsv(msg.pattern || ""),
  ]);

  const csv = [headers.join(","), ...rows.map((r) => r.join(","))].join("\n");
  await writeTextFile(filePath, csv);
  showExportMenu.value = false;
  toast.success(t("pubsub.payloadInspector.exportCsvSuccess"));
}

function escapeCsv(value: string): string {
  if (value.includes(",") || value.includes('"') || value.includes("\n")) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

// Distance (px) from the bottom within which we consider the list "at bottom".
const BOTTOM_THRESHOLD = 40;

// Recompute whether the list is scrolled to the bottom; reset the new-message
// badge once the user reaches the bottom again.
const updateAtBottom = (): void => {
  const el = messagesContainerRef.value;
  if (!el) return;
  isAtBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_THRESHOLD;
  if (isAtBottom.value) newMessageCount.value = 0;
};

// Jump to the newest message at the bottom.
const scrollToBottom = (): void => {
  const el = messagesContainerRef.value;
  if (!el) return;
  el.scrollTop = el.scrollHeight;
  isAtBottom.value = true;
  newMessageCount.value = 0;
};

// Track the previous newest message + connection so we can tell how many new
// messages arrived and whether the connection was switched.
let prevConnId = connStore.activeConnectionId;
let prevTop: PubSubMessageItem | undefined = messages.value[0];

watch(messages, (list) => {
  const currConn = connStore.activeConnectionId;
  // Connection switched: reset state and stick to the bottom of the new list.
  if (currConn !== prevConnId) {
    prevConnId = currConn;
    prevTop = list[0];
    newMessageCount.value = 0;
    nextTick(scrollToBottom);
    return;
  }

  const newTop = list[0];
  if (newTop === prevTop) return;

  if (isAtBottom.value) {
    // Auto-follow: keep the newest message in view.
    nextTick(scrollToBottom);
  } else {
    // Count how many new messages appeared since the last known newest one.
    let added = 0;
    for (const m of list) {
      if (m === prevTop) break;
      added++;
    }
    newMessageCount.value += added > 0 ? added : 1;
  }
  prevTop = newTop;
});

onMounted(async () => {
  // Register the app-wide real-time listener (idempotent) so messages keep
  // arriving into the store even when this page is not mounted.
  await pubsubStore.init();
  if (isConnected.value) {
    await loadChannels();
  }
  nextTick(scrollToBottom);
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-auto min-w-[600px]">
    <!-- Header -->
    <div class="flex items-start justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Radio :size="20" class="text-redis" />
          {{ t("pubsub.title") }}
        </h2>
        <p class="text-sm text-text-muted mt-1">{{ t("pubsub.description") }}</p>
      </div>
    </div>

    <div class="flex-1 flex min-h-0 gap-4">
      <!-- Left Panel: Subscription Management & Channel List -->
      <div class="w-1/4 flex flex-col min-h-0">
        <!-- Subscription Section -->
        <div class="card p-4 mb-4">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold text-text-primary">{{ t("pubsub.subscribeChannel") }}</h3>
            <span
              v-if="isListening"
              class="inline-flex items-center gap-1.5 text-xs font-medium text-success"
            >
              <span class="relative flex h-2 w-2">
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2 w-2 bg-success"></span>
              </span>
              {{ t("pubsub.listening") }}
            </span>
          </div>
          <div class="space-y-3">
            <div class="flex gap-2">
              <input
                v-model="channelInput"
                type="text"
                :placeholder="t('pubsub.inputChannelPlaceholder')"
                @keyup.enter="handleSubscribe"
                class="flex-1 min-w-0 px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary text-text-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="!isConnected || subscribing"
              />
              <button
                @click="handleSubscribe"
                class="shrink-0 whitespace-nowrap inline-flex items-center justify-center gap-1.5 px-4 py-2 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                :disabled="!isConnected || subscribing || !channelInput.trim()"
                :title="!isConnected ? t('status.noConnection') : ''"
              >
                <Plus :size="14" />
                {{ subscribing ? t("pubsub.subscribing") : t("pubsub.subscribe") }}
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-xs text-text-secondary">
                {{
                  patterns.length > 0
                    ? t("pubsub.subscribedCountWithPatterns", {
                        channels: subscriptions.length,
                        patterns: patterns.length,
                      })
                    : t("pubsub.subscribedCount", { count: subscriptions.length })
                }}
              </span>
              <button
                v-if="subscriptionCount > 0"
                @click="handleUnsubscribeAll"
                class="text-xs text-text-secondary hover:text-danger transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="subscribing"
              >
                {{ t("pubsub.unsubscribeAll") }}
              </button>
            </div>

            <!-- Subscription List -->
            <div
              v-if="subscriptionCount > 0"
              class="space-y-1.5"
              :class="{ 'max-h-32 overflow-y-auto': subscriptionCount > 3 }"
              :style="subscriptionCount > 3 ? 'scrollbar-gutter: stable' : ''"
            >
              <div
                v-for="sub in allSubscriptions"
                :key="(sub.isPattern ? 'p:' : 'c:') + sub.name"
                class="flex items-center justify-between px-3 py-2 bg-bg-primary rounded-lg border border-border-light text-sm"
              >
                <span class="flex items-center gap-1.5 min-w-0">
                  <Regex
                    v-if="sub.isPattern"
                    :size="14"
                    class="shrink-0 text-redis"
                    :title="t('pubsub.patternHint')"
                  />
                  <Radio
                    v-else
                    :size="14"
                    class="shrink-0 text-redis"
                    :title="t('pubsub.channelHint')"
                  />
                  <span class="text-text-primary font-mono truncate" :title="sub.name">{{ sub.name }}</span>
                </span>
                <button
                  @click="handleUnsubscribe(sub.name, sub.isPattern)"
                  class="shrink-0 w-6 h-6 flex items-center justify-center rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  :disabled="subscribing"
                >
                  <X :size="13" />
                </button>
              </div>
            </div>
            <p v-else class="text-xs text-text-muted italic">
              {{ t("pubsub.noSubscriptions") }}
            </p>
          </div>
        </div>

        <!-- Channels List -->
        <div class="card p-4 flex-1 min-h-0 overflow-hidden flex flex-col">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold text-text-primary">{{ t("pubsub.availableChannels") }}</h3>
            <button
              @click="loadChannels"
              class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              :disabled="!isConnected || loadingChannels"
              :title="!isConnected ? t('status.noConnection') : ''"
            >
              <RefreshCw :size="13" :class="loadingChannels ? 'animate-spin' : ''" />
              {{ loadingChannels ? t("pubsub.loading") : t("pubsub.refreshList") }}
            </button>
          </div>

          <div v-if="channels.length > 0" class="flex-1 overflow-y-auto space-y-1.5">
            <div
              v-for="channel in channels"
              :key="channel"
              class="flex items-center justify-between px-3 py-2 bg-bg-primary rounded-lg border border-border-light text-sm"
            >
              <span class="text-text-primary font-mono truncate" :title="channel">{{ channel }}</span>
              <div class="flex items-center gap-1 shrink-0">
                <button
                  v-if="!isChannelSubscribed(channel)"
                  @click="handleSubscribeChannel(channel)"
                  class="w-6 h-6 flex items-center justify-center rounded text-text-muted hover:text-redis hover:bg-redis/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  :disabled="subscribing"
                  :title="t('pubsub.subscribe')"
                >
                  <BellPlus :size="13" />
                </button>
                <span v-else class="w-6 h-6 flex items-center justify-center text-redis" :title="t('pubsub.subscribed')">
                  <Bell :size="13" />
                </span>
                <button
                  @click="publishChannel = channel"
                  class="w-6 h-6 flex items-center justify-center rounded text-text-muted hover:text-redis hover:bg-redis/10 transition-colors"
                  :title="t('pubsub.publishMessage')"
                >
                  <Send :size="12" />
                </button>
              </div>
            </div>
          </div>
          <div v-else class="flex-1 flex flex-col items-center justify-center text-text-muted">
            <Radio :size="32" class="mb-2 opacity-30" />
            <p class="text-xs italic">{{ t("pubsub.noAvailableChannels") }}</p>
          </div>
        </div>

        <!-- Publish History (moved to right panel) -->
      </div>

      <!-- Right Panel: Message Reception & Publishing -->
      <div class="flex-1 flex flex-col min-h-0">
        <!-- Message Display Section -->
        <div class="card p-4 flex-1 overflow-hidden flex flex-col">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2">
              {{ t("pubsub.messageList") }}
              <span
                v-if="messages.length > 0"
                class="badge inline-flex items-center px-1.5 py-0.5 text-[11px] font-mono rounded-md bg-redis/10 text-redis"
              >
                {{ messages.length }}
              </span>
            </h3>
            <div class="flex items-center gap-3">
              <!-- TODO: 导出功能暂时隐藏
              <div v-if="messages.length > 0" ref="exportMenuRef" class="relative">
                <button
                  @click.stop="showExportMenu = !showExportMenu"
                  class="inline-flex items-center gap-1.5 text-xs text-text-secondary hover:text-redis transition-colors"
                >
                  <Download :size="13" />
                  {{ t("pubsub.payloadInspector.exportAll") }}
                </button>
                <div
                  v-if="showExportMenu"
                  class="absolute right-0 top-full mt-1 w-40 bg-bg-primary border border-border rounded-lg shadow-lg z-10"
                >
                  <button
                    @click="exportAsJson"
                    class="w-full text-left px-3 py-2 text-xs text-text-primary hover:bg-bg-hover transition-colors rounded-t-lg"
                  >
                    {{ t("pubsub.payloadInspector.exportJson") }}
                  </button>
                  <button
                    @click="exportAsCsv"
                    class="w-full text-left px-3 py-2 text-xs text-text-primary hover:bg-bg-hover transition-colors rounded-b-lg"
                  >
                    {{ t("pubsub.payloadInspector.exportCsv") }}
                  </button>
                </div>
              </div>
              -->
              <button
                v-if="messages.length > 0"
                @click="clearMessages"
                class="text-xs text-text-secondary hover:text-danger transition-colors"
              >
                {{ t("pubsub.clearMessages") }}
              </button>
            </div>
          </div>

          <div class="relative flex-1 min-h-0">
            <div
              ref="messagesContainerRef"
              @scroll="updateAtBottom"
              class="h-full overflow-y-auto space-y-2 bg-bg-primary rounded-lg border border-border-light p-3"
            >
              <div v-if="messages.length === 0" class="flex flex-col items-center justify-center py-12 text-text-muted">
                <MessageSquare :size="32" class="mb-2 opacity-30" />
                <p class="text-sm">{{ t("pubsub.noMessages") }}</p>
              </div>

              <div
                v-for="(msg, index) in displayMessages"
                :key="index"
                class="p-3 bg-bg-secondary rounded-lg border border-border-light text-sm"
              >
                <div class="flex items-center justify-between mb-2">
                  <span class="text-xs font-mono font-semibold text-redis truncate" :title="msg.channel">
                    {{ msg.channel }}
                  </span>
                  <span class="text-[11px] font-mono text-text-muted shrink-0 ml-2">
                    {{ formatTime(msg.timestamp) }}
                  </span>
                </div>
                <SmartPayloadInspector :payload="msg.message" :channel="msg.channel" :timestamp="msg.timestamp" />
              </div>
            </div>

            <!-- Floating jump-to-bottom button (shown when scrolled away) -->
            <button
              v-if="!isAtBottom"
              @click="scrollToBottom"
              class="absolute bottom-3 left-1/2 -translate-x-1/2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-white bg-redis rounded-full shadow-lg hover:bg-redis-dark transition-colors"
            >
              <ArrowDown :size="14" />
              {{ newMessageCount > 0 ? t("pubsub.newMessages", { count: newMessageCount }) : t("pubsub.scrollToBottom") }}
            </button>
          </div>
        </div>

        <!-- Publish Message Section -->
        <div class="card p-3 mt-4 shrink-0">
          <h3 class="text-sm font-semibold text-text-primary mb-2">{{ t("pubsub.publishMessage") }}</h3>
          <div class="space-y-2">
            <div class="flex gap-2">
              <div class="relative flex-1 min-w-0">
                <input
                  v-model="publishChannel"
                  type="text"
                  :placeholder="t('pubsub.channelNamePlaceholder')"
                  class="w-full px-3 py-1.5 text-sm border border-border rounded-lg bg-bg-primary text-text-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 disabled:opacity-50 disabled:cursor-not-allowed"
                  :disabled="!isConnected"
                  @focus="showChannelDropdown = true"
                  @blur="handleChannelBlur"
                />
                <div
                  v-if="showChannelDropdown && filteredSuggestions.length > 0"
                  class="absolute left-0 bottom-full mb-1 w-full max-h-[140px] overflow-y-auto bg-bg-primary border border-border rounded-lg shadow-lg z-20"
                >
                  <button
                    v-for="ch in filteredSuggestions"
                    :key="ch"
                    @mousedown.prevent="selectChannel(ch)"
                    class="w-full text-left px-3 py-1.5 text-xs font-mono text-text-primary hover:bg-bg-hover transition-colors truncate"
                    :title="ch"
                  >
                    {{ ch }}
                  </button>
                </div>
              </div>
              <button
                @click="handlePublish"
                class="shrink-0 whitespace-nowrap inline-flex items-center justify-center gap-1.5 px-4 py-1.5 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                :disabled="!isConnected || publishing || !publishChannel.trim() || !publishMessage.trim()"
                :title="!isConnected ? t('status.noConnection') : ''"
              >
                <Send :size="14" />
                {{ publishing ? t("pubsub.publishing") : t("pubsub.publish") }}
              </button>
              <button
                @click="showPublishHistory = !showPublishHistory"
                class="shrink-0 w-8 h-8 flex items-center justify-center rounded-lg border transition-colors"
                :class="showPublishHistory ? 'text-redis border-redis/40 bg-redis/10' : 'text-text-muted border-border hover:text-redis hover:border-redis/40'"
                :title="t('pubsub.publishHistory')"
              >
                <History :size="14" />
              </button>
            </div>
            <NumberedTextarea
              v-model="publishMessage"
              :placeholder="t('pubsub.messageContentPlaceholder')"
              :rows="2"
              max-height="150px"
              class="w-full"
            />
          </div>
        </div>
      </div>

      <!-- Third Column: Publish History (toggleable) -->
      <div v-if="showPublishHistory" class="w-1/4 flex flex-col min-h-0">
        <div class="card p-4 flex-1 flex flex-col min-h-0">
          <div class="flex items-center justify-between mb-3 shrink-0">
            <h3 class="text-sm font-semibold text-text-primary flex items-center gap-1.5">
              <History :size="13" />
              {{ t("pubsub.publishHistory") }}
              <span
                v-if="publishes.length > 0"
                class="badge inline-flex items-center px-1.5 py-0.5 text-[11px] font-mono rounded-md bg-redis/10 text-redis"
              >
                {{ publishes.length }}
              </span>
            </h3>
            <button
              v-if="publishes.length > 0"
              @click="clearPublishHistory"
              class="text-xs text-text-secondary hover:text-danger transition-colors"
            >
              {{ t("pubsub.clearPublishHistory") }}
            </button>
          </div>

          <div v-if="publishes.length > 0" class="flex-1 min-h-0 overflow-y-auto space-y-1.5" style="scrollbar-gutter: stable">
            <button
              v-for="(item, index) in publishes"
              :key="index"
              @click="reusePublish(item)"
              class="w-full text-left px-3 py-2 bg-bg-primary rounded-lg border border-border-light hover:border-redis/40 hover:bg-bg-hover transition-colors"
              :title="t('pubsub.publish')"
            >
              <div class="flex items-center justify-between gap-2 mb-1">
                <span class="text-xs font-mono font-semibold text-redis truncate" :title="item.channel">
                  {{ item.channel }}
                </span>
                <span class="flex items-center gap-2 shrink-0">
                  <span
                    class="badge inline-flex items-center px-1.5 py-0.5 text-[11px] font-mono rounded-md"
                    :class="item.count > 0 ? 'bg-success/10 text-success' : 'bg-bg-secondary text-text-muted'"
                  >
                    {{ t("pubsub.deliveredCount", { count: item.count }) }}
                  </span>
                  <span class="text-[11px] font-mono text-text-muted">{{ formatTime(item.timestamp) }}</span>
                </span>
              </div>
              <div class="text-xs font-mono text-text-secondary truncate" :title="item.message">
                {{ item.message }}
              </div>
            </button>
          </div>
          <p v-else class="text-xs text-text-muted italic">{{ t("pubsub.noPublishHistory") }}</p>
        </div>
      </div>
    </div>
  </div>
</template>
