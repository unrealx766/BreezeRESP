<script setup lang="ts">
// Monitoring center — overview panel: key server metrics from metricsStore
// (already polled globally while connected) plus the live QPS chart.
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Activity, Cpu, Database, Gauge, MemoryStick, Timer, Users } from "lucide-vue-next";
import { useMetricsStore } from "@/stores/metricsStore";
import { useConnectionStore } from "@/stores/connectionStore";
import QpsChart from "@/components/charts/QpsChart.vue";

const { t } = useI18n();
const metricsStore = useMetricsStore();
const connStore = useConnectionStore();

const memoryUsagePercent = computed(() => {
  const { memoryUsed, memoryTotal } = metricsStore.metrics;
  if (!memoryTotal || memoryTotal <= 0) return null;
  return Math.min(100, (memoryUsed / memoryTotal) * 100);
});

const cpuTotal = computed(() =>
  metricsStore.metrics.usedCpuSys + metricsStore.metrics.usedCpuUser
);

function formatCpu(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0s";
  if (seconds < 60) return `${seconds.toFixed(2)}s`;
  if (seconds < 3600) return `${(seconds / 60).toFixed(1)}m`;
  return `${(seconds / 3600).toFixed(1)}h`;
}
</script>

<template>
  <div class="h-full flex flex-col min-w-[600px]">
    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Activity :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("monitor.noConnection") }}</p>
    </div>

    <template v-else>
      <!-- Metric cards -->
      <div class="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-3 mb-4 shrink-0">
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <Gauge :size="18" class="text-redis shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.qps") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ metricsStore.qps }}</p>
          </div>
        </div>
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <MemoryStick :size="18" class="text-info shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.memoryUsed") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ metricsStore.memoryFormatted }}</p>
          </div>
        </div>
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <Users :size="18" class="text-success shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.clients") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ metricsStore.connectedClients }}</p>
          </div>
        </div>
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <Timer :size="18" class="text-warning shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.uptime") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ metricsStore.uptimeFormatted }}</p>
          </div>
        </div>
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <Database :size="18" class="text-redis shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.hitRate") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ metricsStore.hitRate }}%</p>
          </div>
        </div>
        <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
          <Cpu :size="18" class="text-text-secondary shrink-0" />
          <div class="min-w-0">
            <p class="text-[11px] text-text-muted">{{ t("monitor.cpuTime") }}</p>
            <p class="text-sm font-semibold text-text-primary font-mono">{{ formatCpu(cpuTotal) }}</p>
          </div>
        </div>
      </div>

      <!-- Memory usage bar -->
      <div v-if="memoryUsagePercent !== null" class="rounded-lg border border-border p-4 mb-4 shrink-0">
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs font-semibold text-text-primary">{{ t("monitor.memoryUsage") }}</span>
          <span class="text-[11px] font-mono text-text-muted">
            {{ metricsStore.memoryFormatted }} / {{ metricsStore.memoryTotalFormatted }} ({{ memoryUsagePercent.toFixed(1) }}%)
          </span>
        </div>
        <div class="h-2 rounded-full bg-bg-secondary overflow-hidden">
          <div
            class="h-full rounded-full transition-all duration-500"
            :class="memoryUsagePercent > 90 ? 'bg-danger' : memoryUsagePercent > 75 ? 'bg-warning' : 'bg-info'"
            :style="{ width: `${memoryUsagePercent}%` }"
          ></div>
        </div>
      </div>

      <!-- QPS chart -->
      <div class="rounded-lg border border-border p-4 flex-1 min-h-0">
        <h3 class="text-sm font-semibold text-text-primary mb-2">{{ t("monitor.qpsTrend") }}</h3>
        <QpsChart :data="metricsStore.qpsHistory" :height="220" />
        <p v-if="metricsStore.qpsHistory.length < 2" class="text-[11px] text-text-muted text-center mt-2">
          {{ t("monitor.qpsHint") }}
        </p>
      </div>
    </template>
  </div>
</template>
