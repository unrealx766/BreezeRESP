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
  <footer class="h-7 bg-white border-t border-border flex items-center px-4 text-[11px] text-text-muted shrink-0 gap-3 overflow-hidden">
    <!-- Connection Status -->
    <div class="flex items-center gap-1.5 min-w-0 shrink shrink-2">
      <span
        class="w-1.5 h-1.5 rounded-full shrink-0"
        :class="{
          'bg-success': connStore.activeConnection?.status === 'connected',
          'bg-danger': connStore.activeConnection?.status === 'error',
          'bg-warning animate-pulse': connStore.activeConnection?.status === 'connecting',
          'bg-text-muted': !connStore.activeConnection || connStore.activeConnection?.status === 'disconnected',
        }"
      />
      <span class="truncate" :title="connStore.activeConnection?.name">{{ connStore.activeConnection?.name || t("status.noConnection") }}</span>
    </div>

    <template v-if="connStore.activeConnection?.status === 'connected'">
      <div class="w-px h-3.5 bg-border shrink-0" />

      <!-- Memory -->
      <div class="flex items-center gap-1 shrink-0">
        <HardDrive :size="11" class="shrink-0" />
        <span class="whitespace-nowrap">{{ t("status.memory") }}: {{ metricsStore.memoryFormatted }}</span>
      </div>

      <div class="w-px h-3.5 bg-border shrink-0 hidden md:block" />

      <!-- Version -->
      <div class="flex items-center gap-1 shrink-0 hidden md:flex">
        <Cpu :size="11" class="shrink-0" />
        <span class="whitespace-nowrap">v{{ metricsStore.version }}</span>
      </div>

      <div class="w-px h-3.5 bg-border shrink-0" />

      <!-- QPS -->
      <div class="flex items-center gap-1 shrink-0">
        <Activity :size="11" class="shrink-0" />
        <span class="whitespace-nowrap">{{ t("status.qps") }}: {{ metricsStore.qps }}</span>
      </div>

      <div class="w-px h-3.5 bg-border shrink-0 hidden lg:block" />

      <!-- Clients -->
      <div class="flex items-center gap-1 shrink-0 hidden lg:flex">
        <Wifi :size="11" class="shrink-0" />
        <span class="whitespace-nowrap">{{ t("status.clients") }}: {{ metricsStore.connectedClients }}</span>
      </div>

      <div class="w-px h-3.5 bg-border shrink-0 hidden lg:block" />

      <!-- Uptime -->
      <div class="flex items-center gap-1 shrink-0 hidden lg:flex">
        <Clock :size="11" class="shrink-0" />
        <span class="whitespace-nowrap">{{ metricsStore.uptimeFormatted }}</span>
      </div>

      <div class="w-px h-3.5 bg-border shrink-0" />

      <!-- Hit Rate -->
      <span class="shrink-0 whitespace-nowrap">{{ t("status.hitRate") }}: {{ metricsStore.hitRate }}%</span>
    </template>

    <!-- App version & Copyright (right side) -->
    <span class="ml-auto text-text-muted/60 shrink-0 whitespace-nowrap hidden lg:block">{{ t("app.version") }} · {{ t("app.copyright") }}</span>
  </footer>
</template>
