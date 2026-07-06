import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { MetricsData, QpsDataPoint } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

export const useMetricsStore = defineStore("metrics", () => {
  const metrics = ref<MetricsData>({
    qps: 0,
    qpsHistory: [],
    memoryUsed: 0,
    memoryTotal: 0,
    version: "",
    connectedClients: 0,
    uptimeSeconds: 0,
    usedCpuSys: 0,
    usedCpuUser: 0,
    keyspaceHits: 0,
    keyspaceMisses: 0,
  });
  const monitoring = ref(false);
  let monitorTimer: ReturnType<typeof setInterval> | null = null;

  const qps = computed(() => metrics.value.qps);
  const memoryUsed = computed(() => metrics.value.memoryUsed);
  const memoryTotal = computed(() => metrics.value.memoryTotal);
  const version = computed(() => metrics.value.version);
  const connectedClients = computed(() => metrics.value.connectedClients);
  const qpsHistory = computed<QpsDataPoint[]>(() => metrics.value.qpsHistory);
  const hitRate = computed(() => {
    const total = metrics.value.keyspaceHits + metrics.value.keyspaceMisses;
    return total > 0 ? ((metrics.value.keyspaceHits / total) * 100).toFixed(1) : "0.0";
  });
  const uptimeFormatted = computed(() => {
    const s = metrics.value.uptimeSeconds;
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  });

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(2)}K`;
    if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(2)}M`;
    return `${(bytes / 1073741824).toFixed(2)}G`;
  }

  const memoryFormatted = computed(() => formatBytes(metrics.value.memoryUsed));
  const memoryTotalFormatted = computed(() => formatBytes(metrics.value.memoryTotal));

  async function fetchMetrics() {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    const conn = connStore.activeConnection;
    if (!conn || conn.status !== "connected") return;

    try {
      const data = await tauriApi.metrics.get(connId);
      metrics.value.qps = data.instantaneousOpsPerSec;
      metrics.value.memoryUsed = data.usedMemory;
      metrics.value.memoryTotal = data.totalMemory;
      metrics.value.version = data.version;
      metrics.value.connectedClients = data.connectedClients;
      metrics.value.uptimeSeconds = data.uptimeSeconds;
      metrics.value.usedCpuSys = data.usedCpuSys;
      metrics.value.usedCpuUser = data.usedCpuUser;
      metrics.value.keyspaceHits = data.keyspaceHits;
      metrics.value.keyspaceMisses = data.keyspaceMisses;

      // Update QPS history
      metrics.value.qpsHistory.push({ timestamp: Date.now(), value: data.instantaneousOpsPerSec });
      if (metrics.value.qpsHistory.length > 60) {
        metrics.value.qpsHistory = metrics.value.qpsHistory.slice(-60);
      }
    } catch (e) {
      console.error("Failed to fetch metrics:", e);
    }
  }

  function startMonitoring() {
    if (monitoring.value) return;
    monitoring.value = true;
    // Fetch immediately, then poll every second
    fetchMetrics();
    monitorTimer = setInterval(fetchMetrics, 1000);
  }

  function stopMonitoring() {
    monitoring.value = false;
    if (monitorTimer) { clearInterval(monitorTimer); monitorTimer = null; }
  }

  return {
    metrics,
    monitoring,
    qps,
    memoryUsed,
    memoryTotal,
    version,
    connectedClients,
    qpsHistory,
    hitRate,
    uptimeFormatted,
    memoryFormatted,
    memoryTotalFormatted,
    formatBytes,
    startMonitoring,
    stopMonitoring,
  };
});
