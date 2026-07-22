<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { usePubsubStore } from "@/stores/pubsubStore";
import type { PubSubMessageItem } from "@/stores/pubsubStore";
import { toast } from "@/utils/toast";
import NumberedTextarea from "@/components/shared/NumberedTextarea.vue";
import { Radio, Plus, Send, MessageSquare, RefreshCw, X, ArrowDown } from "lucide-vue-next";

const { t } = useI18n();
const connStore = useConnectionStore();
const pubsubStore = usePubsubStore();
const isConnected = computed(() => connStore.activeConnection?.status === "connected");

// Subscriptions & received messages live in the store (keyed by connection id)
// so they persist across tab switches — this page unmounts on navigation.
const subscriptions = computed(() => pubsubStore.channelsOf(connStore.activeConnectionId));
const messages = computed(() => pubsubStore.messagesOf(connStore.activeConnectionId));

// Messages are stored newest-first; display oldest→newest (newest at bottom).
const displayMessages = computed(() => [...messages.value].reverse());

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

// Loading states
const loadingChannels = ref(false);
const subscribing = ref(false);
const publishing = ref(false);

// Whether we are actively listening to at least one channel
const isListening = computed(() => subscriptions.value.length > 0);

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
const handleSubscribe = async (): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn || !channelInput.value.trim()) return;

  const channel = channelInput.value.trim();
  if (subscriptions.value.includes(channel)) {
    toast.info(t("pubsub.alreadySubscribed"));
    return;
  }

  try {
    subscribing.value = true;
    await pubsubStore.subscribe(conn.id, channel);
    toast.success(t("pubsub.subscribeSuccess", { channel }));
    channelInput.value = "";
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.subscribeFailed"));
  } finally {
    subscribing.value = false;
  }
};

// Unsubscribe from a channel
const handleUnsubscribe = async (channel: string): Promise<void> => {
  const conn = connStore.activeConnection;
  if (!conn) return;

  try {
    subscribing.value = true;
    await pubsubStore.unsubscribe(conn.id, channel);
    toast.success(t("pubsub.unsubscribeSuccess", { channel }));
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
    toast.success(t("pubsub.unsubscribeAllSuccess"));
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

  try {
    publishing.value = true;
    const numSubscribers = await tauriApi.pubsub.publish(
      conn.id,
      publishChannel.value.trim(),
      publishMessage.value.trim()
    );

    toast.success(t("pubsub.publishSuccess", { channel: publishChannel.value.trim(), count: numSubscribers }));
    publishMessage.value = "";
  } catch (error: any) {
    toast.error(error.message || t("pubsub.errors.publishFailed"));
  } finally {
    publishing.value = false;
  }
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
      <div class="w-2/5 flex flex-col min-h-0">
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
                class="flex-1 px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary text-text-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="!isConnected || subscribing"
              />
              <button
                @click="handleSubscribe"
                class="inline-flex items-center justify-center gap-1.5 px-4 py-2 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                :disabled="!isConnected || subscribing || !channelInput.trim()"
                :title="!isConnected ? t('status.noConnection') : ''"
              >
                <Plus :size="14" />
                {{ subscribing ? t("pubsub.subscribing") : t("pubsub.subscribe") }}
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-xs text-text-secondary">
                {{ t("pubsub.subscribedCount", { count: subscriptions.length }) }}
              </span>
              <button
                v-if="subscriptions.length > 0"
                @click="handleUnsubscribeAll"
                class="text-xs text-text-secondary hover:text-danger transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="subscribing"
              >
                {{ t("pubsub.unsubscribeAll") }}
              </button>
            </div>

            <!-- Subscription List -->
            <div v-if="subscriptions.length > 0" class="max-h-32 overflow-y-auto space-y-1.5">
              <div
                v-for="channel in subscriptions"
                :key="channel"
                class="flex items-center justify-between px-3 py-2 bg-bg-primary rounded-lg border border-border-light text-sm"
              >
                <span class="text-text-primary font-mono truncate" :title="channel">{{ channel }}</span>
                <button
                  @click="handleUnsubscribe(channel)"
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
        <div class="card p-4 flex-1 overflow-hidden flex flex-col">
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
              <button
                @click="publishChannel = channel"
                class="shrink-0 text-xs text-redis hover:text-redis-dark transition-colors"
              >
                {{ t("pubsub.publishMessage") }}
              </button>
            </div>
          </div>
          <div v-else class="flex-1 flex flex-col items-center justify-center text-text-muted">
            <Radio :size="32" class="mb-2 opacity-30" />
            <p class="text-xs italic">{{ t("pubsub.noAvailableChannels") }}</p>
          </div>
        </div>
      </div>

      <!-- Right Panel: Message Reception & Publishing -->
      <div class="w-3/5 flex flex-col min-h-0">
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
            <button
              v-if="messages.length > 0"
              @click="clearMessages"
              class="text-xs text-text-secondary hover:text-danger transition-colors"
            >
              {{ t("pubsub.clearMessages") }}
            </button>
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
                <div class="flex items-center justify-between mb-1">
                  <span class="text-xs font-mono font-semibold text-redis truncate" :title="msg.channel">
                    {{ msg.channel }}
                  </span>
                  <span class="text-[11px] font-mono text-text-muted shrink-0 ml-2">
                    {{ formatTime(msg.timestamp) }}
                  </span>
                </div>
                <div class="text-xs font-mono text-text-primary break-all whitespace-pre-wrap">
                  {{ msg.message }}
                </div>
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
              <input
                v-model="publishChannel"
                type="text"
                :placeholder="t('pubsub.channelNamePlaceholder')"
                class="flex-1 px-3 py-1.5 text-sm border border-border rounded-lg bg-bg-primary text-text-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="!isConnected"
              />
              <button
                @click="handlePublish"
                class="inline-flex items-center justify-center gap-1.5 px-4 py-1.5 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                :disabled="!isConnected || publishing || !publishChannel.trim() || !publishMessage.trim()"
                :title="!isConnected ? t('status.noConnection') : ''"
              >
                <Send :size="14" />
                {{ publishing ? t("pubsub.publishing") : t("pubsub.publish") }}
              </button>
            </div>
            <NumberedTextarea
              v-model="publishMessage"
              :placeholder="t('pubsub.messageContentPlaceholder')"
              :rows="2"
              class="w-full"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
