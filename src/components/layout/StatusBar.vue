<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { Activity, HardDrive, Clock, Cpu, Wifi } from "lucide-vue-next";

const { t } = useI18n();
const connStore = useConnectionStore();
const metricsStore = useMetricsStore();
</script>

<template>
  <footer class="h-7 bg-white border-t border-border flex items-center px-4 text-[11px] text-text-muted shrink-0 gap-4">
    <!-- Connection Status -->
    <div class="flex items-center gap-1.5">
      <span
        class="w-1.5 h-1.5 rounded-full"
        :class="{
          'bg-success': connStore.activeConnection?.status === 'connected',
          'bg-danger': connStore.activeConnection?.status === 'error',
          'bg-warning animate-pulse': connStore.activeConnection?.status === 'connecting',
          'bg-text-muted': !connStore.activeConnection || connStore.activeConnection?.status === 'disconnected',
        }"
      />
      <span>{{ connStore.activeConnection?.name || t("status.noConnection") }}</span>
    </div>

    <template v-if="connStore.activeConnection?.status === 'connected'">
      <div class="w-px h-3.5 bg-border" />

      <!-- Memory -->
      <div class="flex items-center gap-1">
        <HardDrive :size="11" />
        <span>{{ t("status.memory") }}: {{ metricsStore.memoryFormatted }}</span>
      </div>

      <div class="w-px h-3.5 bg-border" />

      <!-- Version -->
      <div class="flex items-center gap-1">
        <Cpu :size="11" />
        <span>v{{ metricsStore.version }}</span>
      </div>

      <div class="w-px h-3.5 bg-border" />

      <!-- QPS -->
      <div class="flex items-center gap-1">
        <Activity :size="11" />
        <span>{{ t("status.qps") }}: {{ metricsStore.qps }}</span>
      </div>

      <div class="w-px h-3.5 bg-border" />

      <!-- Clients -->
      <div class="flex items-center gap-1">
        <Wifi :size="11" />
        <span>{{ t("status.clients") }}: {{ metricsStore.connectedClients }}</span>
      </div>

      <div class="w-px h-3.5 bg-border" />

      <!-- Uptime -->
      <div class="flex items-center gap-1">
        <Clock :size="11" />
        <span>{{ metricsStore.uptimeFormatted }}</span>
      </div>

      <div class="w-px h-3.5 bg-border" />

      <!-- Hit Rate -->
      <span>{{ t("status.hitRate") }}: {{ metricsStore.hitRate }}%</span>
    </template>

    <!-- App version & Copyright (right side) -->
    <span class="ml-auto text-text-muted/60">{{ t("app.version") }} · {{ t("app.copyright") }}</span>
  </footer>
</template>
