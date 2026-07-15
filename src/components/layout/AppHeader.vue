<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { ref, computed, nextTick } from "vue";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { messageHistory, clearMessageHistory } from "@/utils/toast";
import SettingsDialog from "@/components/shared/SettingsDialog.vue";
import { Database, ChevronDown, Check, RefreshCw, Bell, BellDot, Trash2, CheckCircle, XCircle, AlertTriangle, Info, Settings } from "lucide-vue-next";

const { t, locale } = useI18n();
const connStore = useConnectionStore();
const cascade = useCascadeStore();
const detail = useDetailStore();

const settingsDialog = ref<InstanceType<typeof SettingsDialog> | null>(null);

function openSettings() {
  settingsDialog.value?.open();
}

// DB switching
const switchingDb = ref(false);
const showDbDropdown = ref(false);
const dbDropdownEl = ref<HTMLElement | null>(null);
const savedDbScrollTop = ref(0);

async function toggleDbDropdown() {
  if (showDbDropdown.value) {
    // Save scroll position before closing
    savedDbScrollTop.value = dbDropdownEl.value?.scrollTop ?? 0;
    showDbDropdown.value = false;
  } else {
    showDbDropdown.value = true;
    // Restore scroll position after DOM renders
    await nextTick();
    if (dbDropdownEl.value) {
      dbDropdownEl.value.scrollTop = savedDbScrollTop.value;
    }
  }
}

const activeConnDb = computed(() => connStore.activeConnection?.db ?? 0);

async function handleDbSwitch(db: number) {
  if (switchingDb.value) return;
  switchingDb.value = true;
  savedDbScrollTop.value = dbDropdownEl.value?.scrollTop ?? 0;
  showDbDropdown.value = false;
  try {
    await connStore.switchDb(db);
    cascade.selectedKey = null;
    cascade.searchQuery = "";
    cascade.debouncedSearchQuery = "";
    cascade.typeFilter = "all";
    detail.clearDetail();
    await cascade.refreshKeys(true);
  } catch (e) {
    console.error("Header DB switch failed:", e);
  } finally {
    switchingDb.value = false;
  }
}

// Notification panel
const showNotifications = ref(false);
const unreadCount = computed(() => messageHistory.value.length);

function toggleNotifications() {
  showNotifications.value = !showNotifications.value;
}

function closeNotifications() {
  showNotifications.value = false;
}

function formatTime(ts: number) {
  const d = new Date(ts);
  return d.toLocaleTimeString(locale.value === "zh-CN" ? "zh-CN" : "en-US", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

const iconMap = { success: CheckCircle, error: XCircle, warning: AlertTriangle, info: Info };
const colorMap = {
  success: "text-success",
  error: "text-danger",
  warning: "text-warning",
  info: "text-info",
};
</script>

<template>
  <header class="h-12 bg-bg-secondary border-b border-border flex items-center justify-between px-4 shrink-0 overflow-visible">
    <!-- Left: Connection info -->
    <div class="flex items-center gap-3 min-w-0">
      <template v-if="connStore.activeConnection">
        <span
          class="w-2 h-2 rounded-full shrink-0"
          :class="{
            'bg-success': connStore.activeConnection.status === 'connected',
            'bg-text-muted': connStore.activeConnection.status === 'disconnected',
            'bg-warning animate-pulse': connStore.activeConnection.status === 'connecting',
            'bg-danger': connStore.activeConnection.status === 'error',
          }"
        />
        <span class="text-sm font-medium text-text-primary truncate max-w-[40%]" :title="connStore.activeConnection.name">{{ connStore.activeConnection.name }}</span>
        <span class="text-xs text-text-muted whitespace-nowrap shrink-0">{{ connStore.activeConnection.host }}:{{ connStore.activeConnection.port }}</span>
        <!-- DB switcher -->
        <div v-if="connStore.activeConnection.status === 'connected'" class="relative shrink-0">
          <button
            @click="toggleDbDropdown"
            :disabled="switchingDb"
            class="inline-flex items-center gap-1 px-2 py-1 text-[11px] font-mono font-semibold text-redis bg-redis/5 border border-redis/20 rounded-lg hover:border-redis/40 focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 transition-colors disabled:opacity-50"
          >
            <Database :size="11" class="shrink-0" />
            <span>DB{{ activeConnDb }}</span>
            <RefreshCw v-if="switchingDb" :size="10" class="animate-spin text-redis/60" />
            <ChevronDown v-else :size="11" class="text-redis/50 transition-transform" :class="showDbDropdown ? 'rotate-180' : ''" />
          </button>
          <!-- Backdrop -->
          <div v-if="showDbDropdown" class="fixed inset-0 z-40" @click="savedDbScrollTop = dbDropdownEl?.scrollTop ?? 0; showDbDropdown = false" />
          <!-- Dropdown -->
          <div
            v-if="showDbDropdown"
            ref="dbDropdownEl"
            class="absolute top-full left-0 mt-1 w-28 bg-bg-secondary border border-border rounded-lg shadow-lg py-1 z-50 max-h-52 overflow-y-auto"
          >
            <div class="px-2.5 py-1 border-b border-border-light mb-0.5">
              <span class="text-[9px] font-semibold text-text-muted uppercase tracking-wider">Database</span>
            </div>
            <button
              v-for="n in 16"
              :key="n - 1"
              @click="handleDbSwitch(n - 1)"
              class="w-full flex items-center justify-between px-2.5 py-1.5 text-xs font-mono transition-colors"
              :class="activeConnDb === n - 1
                ? 'text-redis font-semibold bg-redis/5'
                : 'text-text-secondary font-medium hover:bg-bg-hover hover:text-text-primary'"
            >
              <span>DB{{ n - 1 }}</span>
              <Check v-if="activeConnDb === n - 1" :size="11" class="text-redis" />
            </button>
          </div>
        </div>
      </template>
      <template v-else>
        <span class="text-sm text-text-muted whitespace-nowrap">{{ t("status.noConnection") }}</span>
      </template>
    </div>

    <!-- Right: Notifications + Language -->
    <div class="flex items-center gap-1">
      <!-- Notification bell -->
      <div class="relative">
        <button
          @click="toggleNotifications"
          class="relative flex items-center justify-center w-8 h-8 rounded-lg text-text-secondary hover:bg-bg-hover transition-colors"
          :title="t('notifications.title')"
        >
          <BellDot v-if="unreadCount > 0" :size="15" />
          <Bell v-else :size="15" />
          <span
            v-if="unreadCount > 0"
            class="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 flex items-center justify-center bg-danger text-white text-[9px] font-bold rounded-full"
          >{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
        </button>
        <!-- Backdrop -->
        <div v-if="showNotifications" class="fixed inset-0 z-40" @click="closeNotifications" />
        <!-- Notification panel -->
        <div
          v-if="showNotifications"
          class="absolute top-full right-0 mt-1 w-80 bg-bg-secondary border border-border rounded-xl shadow-xl z-50 overflow-hidden"
        >
          <!-- Header -->
          <div class="flex items-center justify-between px-3 py-2 border-b border-border-light">
            <span class="text-xs font-semibold text-text-primary">{{ t("notifications.title") }}</span>
            <div class="flex items-center gap-1">
              <button
                v-if="messageHistory.length > 0"
                @click="clearMessageHistory"
                class="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] text-text-muted hover:text-danger hover:bg-danger/5 transition-colors"
              >
                <Trash2 :size="10" />
                {{ t("notifications.clear") }}
              </button>
            </div>
          </div>
          <!-- List -->
          <div class="max-h-72 overflow-y-auto">
            <div
              v-for="msg in messageHistory"
              :key="msg.id"
              class="flex items-start gap-2 px-3 py-2 border-b border-border-light/50 last:border-b-0 hover:bg-bg-hover/50 transition-colors"
            >
              <component :is="iconMap[msg.type]" :size="13" :class="colorMap[msg.type]" class="shrink-0 mt-0.5" />
              <div class="flex-1 min-w-0">
                <p class="text-xs text-text-primary break-all leading-relaxed">{{ msg.message }}</p>
                <div class="flex items-center gap-2 mt-0.5">
                  <span class="text-[10px] font-medium truncate" :title="(msg.connectionName || t('notifications.noConnection')) + (msg.db !== undefined ? ` (DB ${msg.db})` : '')"
                    :class="msg.connectionName ? 'text-redis/60' : 'text-text-muted/60'">{{ msg.connectionName || t('notifications.noConnection') }}<template v-if="msg.db !== undefined">/{{ msg.db }}</template></span>
                  <span class="text-[10px] text-text-muted">{{ formatTime(msg.timestamp) }}</span>
                </div>
              </div>
            </div>
            <!-- Empty state -->
            <div v-if="messageHistory.length === 0" class="px-3 py-8 text-center">
              <p class="text-xs text-text-muted">{{ t("notifications.empty") }}</p>
            </div>
          </div>
        </div>
      </div>

      <!-- Settings -->
      <button
        @click="openSettings"
        class="flex items-center justify-center w-8 h-8 rounded-lg text-text-secondary hover:bg-bg-hover transition-colors"
        :title="t('settings.title')"
      >
        <Settings :size="15" />
      </button>
    </div>
  </header>
  <SettingsDialog ref="settingsDialog" />
</template>
