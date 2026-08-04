import { defineStore } from "pinia";
import { ref } from "vue";
import type { SlowlogEntry, SlowlogInfo } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

export const useSlowlogStore = defineStore("slowlog", () => {
  const entries = ref<SlowlogEntry[]>([]);
  const totalLen = ref(0);
  const slowlogLogSlowerThan = ref(10000); // default 10ms in microseconds
  const loading = ref(false);
  const autoRefresh = ref(false);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function fetchSlowlog(count = 128) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    const conn = connStore.activeConnection;
    if (!conn || conn.status !== "connected") return;

    loading.value = true;
    try {
      const data: SlowlogInfo = await tauriApi.slowlog.get(connId, count);
      entries.value = data.entries;
      totalLen.value = data.totalLen;
      slowlogLogSlowerThan.value = data.slowlogLogSlowerThan;
    } catch (e) {
      console.error("Failed to fetch slowlog:", e);
    } finally {
      loading.value = false;
    }
  }

  function startAutoRefresh(intervalMs = 3000, count = 128) {
    stopAutoRefresh();
    autoRefresh.value = true;
    fetchSlowlog(count);
    refreshTimer = setInterval(() => fetchSlowlog(count), intervalMs);
  }

  function stopAutoRefresh() {
    autoRefresh.value = false;
    if (refreshTimer) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  }

  function resetSlowlog() {
    entries.value = [];
    totalLen.value = 0;
    slowlogLogSlowerThan.value = 10000;
  }

  return {
    entries,
    totalLen,
    slowlogLogSlowerThan,
    loading,
    autoRefresh,
    fetchSlowlog,
    startAutoRefresh,
    stopAutoRefresh,
    resetSlowlog,
  };
});
