<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { tauriApi, type PubSubMessage } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";
import NumberedTextarea from "@/components/shared/NumberedTextarea.vue";

const connectionStore = useConnectionStore();
const activeConnection = computed(() => connectionStore.connections.find((c) => c.id === connectionStore.activeConnectionId));

// Channel related state
const channelInput = ref("");
const channels = ref<string[]>([]);
const subscriptions = ref<string[]>([]);
const messages = ref<PubSubMessage[]>([]);

// Publishing related state
const publishChannel = ref("");
const publishMessage = ref("");

// Loading states
const loadingChannels = ref(false);
const subscribing = ref(false);
const publishing = ref(false);

// Calculate whether currently connected
const isConnected = computed(() => !!activeConnection.value);

// Load channel list
const loadChannels = async (): Promise<void> => {
  if (!activeConnection.value) return;

  try {
    loadingChannels.value = true;
    const channelList = await tauriApi.pubsub.listChannels(activeConnection.value.id);
    channels.value = channelList;
  } catch (error: any) {
    toast.error(error.message || "获取频道列表失败");
  } finally {
    loadingChannels.value = false;
  }
};

// Subscribe to a channel
const handleSubscribe = async (): Promise<void> => {
  if (!activeConnection.value || !channelInput.value.trim()) return;

  const channel = channelInput.value.trim();
  if (subscriptions.value.includes(channel)) {
    toast.info("已订阅该频道");
    return;
  }

  try {
    subscribing.value = true;
    const result = await tauriApi.pubsub.subscribe(activeConnection.value.id, channel);

    if (result.includes("ready")) {
      subscriptions.value.push(channel);
      toast.success(`成功订阅频道：${channel}`);

      // Clear input field
      channelInput.value = "";
    }
  } catch (error: any) {
    toast.error(error.message || "订阅失败");
  } finally {
    subscribing.value = false;
  }
};

// Unsubscribe from a channel
const handleUnsubscribe = async (channel: string): Promise<void> => {
  if (!activeConnection.value) return;

  try {
    subscribing.value = true;
    const result = await tauriApi.pubsub.unsubscribe(activeConnection.value.id, channel);

    if (result.includes("ready")) {
      subscriptions.value = subscriptions.value.filter((c) => c !== channel);
      messages.value = messages.value.filter((m) => m.channel !== channel);
      toast.success(`已取消订阅：${channel}`);
    }
  } catch (error: any) {
    toast.error(error.message || "取消订阅失败");
  } finally {
    subscribing.value = false;
  }
};

// Unsubscribe from all channels
const handleUnsubscribeAll = async (): Promise<void> => {
  if (!activeConnection.value) return;

  try {
    subscribing.value = true;
    const result = await tauriApi.pubsub.unsubscribe(activeConnection.value.id);

    if (result.includes("ready")) {
      subscriptions.value = [];
      messages.value = [];
      toast.success("已取消所有订阅");
    }
  } catch (error: any) {
    toast.error(error.message || "取消订阅失败");
  } finally {
    subscribing.value = false;
  }
};

// Publish a message to a channel
const handlePublish = async (): Promise<void> => {
  if (!activeConnection.value || !publishChannel.value.trim() || !publishMessage.value.trim()) return;

  try {
    publishing.value = true;
    const numSubscribers = await tauriApi.pubsub.publish(
      activeConnection.value.id,
      publishChannel.value.trim(),
      publishMessage.value.trim()
    );

    toast.success(`消息已发布到频道 "${publishChannel.value}"，${numSubscribers} 个订阅者收到`);

    // Clear message content
    publishMessage.value = "";
  } catch (error: any) {
    toast.error(error.message || "发布失败");
  } finally {
    publishing.value = false;
  }
};

// Get subscriber count for a channel
const getNumSubs = async (channel: string): Promise<number> => {
  if (!activeConnection.value) return 0;

  try {
    return await tauriApi.pubsub.numSubs(activeConnection.value.id, channel);
  } catch {
    return 0;
  }
};

// Wrapper function for subscriber count (for templates)
const subsCount = ref<Record<string, number>>({});

// Format timestamp
const formatTime = (timestamp: number): string => {
  return new Date(timestamp).toLocaleTimeString("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

// Clear all messages
const clearMessages = (): void => {
  messages.value = [];
  toast.info("已清空消息");
};

onMounted(async () => {
  if (isConnected.value) {
    await loadChannels();

    // Load subscriber counts
    for (const channel of subscriptions.value) {
      const count = await getNumSubs(channel);
      subsCount.value[channel] = count;
    }
  }
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-auto min-w-[600px]">
    <!-- Header -->
    <div class="flex items-center justify-between mb-4 shrink-0 flex-wrap">
      <h2 class="text-xl font-semibold text-text-primary">发布/订阅</h2>
    </div>

    <div class="flex-1 flex min-h-0 gap-4">
      <!-- Left Panel: Subscription Management & Channel List -->
      <div class="w-1/2 flex flex-col min-h-0 rounded-lg border border-border-light bg-bg-secondary">
        <!-- Subscription Section -->
        <div class="p-4 border-b border-border">
          <h3 class="font-medium mb-2 text-text-primary">订阅频道</h3>
          <div class="flex gap-2 mb-2">
            <input
              v-model="channelInput"
              type="text"
              placeholder="输入频道名称..."
              @keyup.enter="handleSubscribe"
              class="flex-1 px-3 py-2 border border-[color:var(--border-color)] rounded bg-[color:var(--bg-secondary)] text-[color:var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[color:var(--primary-color)]"
              :disabled="subscribing"
            />
            <button
              @click="handleSubscribe"
              class="px-4 py-2 bg-[color:var(--primary-color)] text-white rounded hover:bg-[color:var(--primary-hover)] disabled:opacity-50 disabled:cursor-not-allowed"
              :disabled="subscribing || !channelInput.trim()"
            >
              {{ subscribing ? '订阅中...' : '订阅' }}
            </button>
          </div>

          <div class="flex items-center justify-between mb-2">
            <span class="text-sm text-[color:var(--text-secondary)]">
              已订阅：{{ subscriptions.length }} 个频道
            </span>
            <button
              v-if="subscriptions.length > 0"
              @click="handleUnsubscribeAll"
              class="text-sm text-[color:var(--text-secondary)] hover:text-[color:var(--danger-color)]"
              :disabled="subscribing"
            >
              取消全部订阅
            </button>
          </div>

          <!-- 订阅列表 -->
          <div v-if="subscriptions.length > 0" class="max-h-32 overflow-y-auto space-y-1">
            <div
              v-for="channel in subscriptions"
              :key="channel"
              class="flex items-center justify-between p-2 bg-[color:var(--bg-secondary)] rounded border border-[color:var(--border-color)]"
            >
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{{ channel }}</span>
                <span class="text-xs text-[color:var(--text-secondary)]">
                  {{ subsCount[channel] || 0 }} 订阅者
                </span>
              </div>
              <button
                @click="handleUnsubscribe(channel)"
                class="text-sm text-[color:var(--text-secondary)] hover:text-[color:var(--danger-color)]"
                :disabled="subscribing"
              >
                ✕
              </button>
            </div>
          </div>
          <div v-else class="text-sm text-[color:var(--text-secondary)] italic">
            暂无订阅
          </div>
        </div>

        <!-- Channels List -->
        <div class="flex-1 p-4 overflow-y-auto">
          <h3 class="font-medium mb-2 text-text-primary">可用频道</h3>
          <button
            @click="loadChannels"
            class="mb-2 px-3 py-1 text-sm border border-[color:var(--border-color)] rounded hover:bg-[color:var(--bg-secondary)]"
            :disabled="loadingChannels"
          >
            {{ loadingChannels ? '加载中...' : '刷新列表' }}
          </button>
          
          <div v-if="channels.length > 0" class="space-y-1 max-h-48 overflow-y-auto">
            <div
              v-for="channel in channels"
              :key="channel"
              class="flex items-center justify-between p-2 bg-[color:var(--bg-secondary)] rounded border border-[color:var(--border-color)]"
            >
              <span class="text-sm">{{ channel }}</span>
              <button
                @click="() => { publishChannel = channel; loadChannels(); }"
                class="text-sm text-[color:var(--primary-color)] hover:underline"
              >
                发布消息
              </button>
            </div>
          </div>
          <div v-else class="text-sm text-[color:var(--text-secondary)] italic">
            暂无可用频道
          </div>
        </div>
      </div>

      <!-- Right Panel: Message Reception & Publishing -->
      <div class="w-1/2 flex flex-col rounded-lg border border-border-light bg-bg-secondary">
        <!-- Publish Message Section -->
        <div class="p-4 border-b border-border">
          <h3 class="font-medium mb-2 text-text-primary">发布消息</h3>
          <div class="space-y-2">
            <div class="flex gap-2">
              <input
                v-model="publishChannel"
                type="text"
                placeholder="频道名称"
                class="flex-1 px-3 py-2 border border-[color:var(--border-color)] rounded bg-[color:var(--bg-secondary)] text-[color:var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[color:var(--primary-color)]"
              />
              <button
                @click="handlePublish"
                class="px-4 py-2 bg-[color:var(--primary-color)] text-white rounded hover:bg-[color:var(--primary-hover)] disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="publishing || !publishChannel.trim() || !publishMessage.trim()"
              >
                {{ publishing ? '发送中...' : '发送' }}
              </button>
            </div>
            <NumberedTextarea
              v-model="publishMessage"
              placeholder="输入消息内容..."
              class="w-full"
            />
          </div>
        </div>

        <!-- Message Display Section -->
        <div class="flex-1 p-4 overflow-hidden flex flex-col">
          <div class="flex items-center justify-between mb-2">
            <h3 class="font-medium text-text-primary">消息列表</h3>
            <button
              v-if="messages.length > 0"
              @click="clearMessages"
              class="text-sm text-[color:var(--text-secondary)] hover:text-[color:var(--danger-color)]"
            >
              清空消息
            </button>
          </div>
          
          <div class="flex-1 overflow-y-auto space-y-2 bg-[color:var(--bg-secondary)] rounded border border-[color:var(--border-color)] p-3">
            <div v-if="messages.length === 0" class="text-center text-[color:var(--text-secondary)] italic py-8">
              暂无消息
            </div>

            <div
              v-for="(msg, index) in messages"
              :key="index"
              class="p-3 bg-[color:var(--bg-primary)] rounded border border-[color:var(--border-color)]"
            >
              <div class="flex items-center justify-between mb-1">
                <span class="text-sm font-medium text-[color:var(--primary-color)]">
                  {{ msg.channel }}
                </span>
                <span class="text-xs text-[color:var(--text-secondary)]">
                  {{ formatTime(msg.timestamp) }}
                </span>
              </div>
              <div class="text-sm break-all whitespace-pre-wrap">
                {{ msg.message }}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 使用 CSS 变量支持主题 */
:deep(.numbered-textarea) {
  width: 100%;
}
</style>
