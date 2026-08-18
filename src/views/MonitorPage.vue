<script setup lang="ts">
// Unified monitoring center: overview / slow queries / big keys /
// server administration / cluster topology, switchable via top tabs.
import { ref, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Activity, Gauge, MemoryStick, Network, Server } from "lucide-vue-next";
import { useConnectionStore } from "@/stores/connectionStore";
import OverviewPanel from "@/components/monitor/OverviewPanel.vue";
import SlowlogPanel from "@/components/monitor/SlowlogPanel.vue";
import BigKeyPanel from "@/components/monitor/BigKeyPanel.vue";
import ServerAdminPanel from "@/components/monitor/ServerAdminPanel.vue";
import ClusterTopologyPanel from "@/components/monitor/ClusterTopologyPanel.vue";

type MonitorTab = "overview" | "slowlog" | "bigkey" | "server" | "cluster";

const { t } = useI18n();
const connStore = useConnectionStore();

const TAB_KEY = "breezeresp-monitor-tab";
const VALID_TABS: MonitorTab[] = ["overview", "slowlog", "bigkey", "server", "cluster"];
const stored = localStorage.getItem(TAB_KEY) as MonitorTab | null;
const activeTab = ref<MonitorTab>(stored && VALID_TABS.includes(stored) ? stored : "overview");

watch(activeTab, (v) => localStorage.setItem(TAB_KEY, v));

const tabs = computed(() => [
  { key: "overview" as MonitorTab, icon: Gauge, label: t("monitor.tabOverview") },
  { key: "slowlog" as MonitorTab, icon: Activity, label: t("monitor.tabSlowlog") },
  { key: "bigkey" as MonitorTab, icon: MemoryStick, label: t("monitor.tabBigKey") },
  { key: "server" as MonitorTab, icon: Server, label: t("monitor.tabServer") },
  { key: "cluster" as MonitorTab, icon: Network, label: t("monitor.tabCluster") },
]);

const panelComponent = computed(() => {
  switch (activeTab.value) {
    case "slowlog": return SlowlogPanel;
    case "bigkey": return BigKeyPanel;
    case "server": return ServerAdminPanel;
    case "cluster": return ClusterTopologyPanel;
    default: return OverviewPanel;
  }
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-hidden">
    <!-- Page header + tab bar -->
    <div class="flex items-center justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Gauge :size="20" class="text-redis" />
          {{ t("monitor.title") }}
        </h2>
        <p v-if="connStore.activeConnection" class="text-xs text-text-muted mt-1">
          {{ connStore.activeConnection.name }} · {{ connStore.activeConnection.host }}:{{ connStore.activeConnection.port }}
          <span v-if="connStore.activeConnection.cluster" class="ml-1 text-redis/80">Cluster</span>
        </p>
      </div>

      <!-- Tab switcher -->
      <div class="flex items-center h-8 rounded-lg border border-border overflow-hidden">
        <button
          v-for="(tab, idx) in tabs"
          :key="tab.key"
          @click="activeTab = tab.key"
          class="px-3 h-full text-xs flex items-center gap-1.5 transition-colors"
          :class="[
            activeTab === tab.key ? 'bg-redis/10 text-redis font-medium' : 'text-text-secondary hover:bg-bg-hover',
            idx > 0 ? 'border-l border-border' : ''
          ]"
        >
          <component :is="tab.icon" :size="13" />
          {{ tab.label }}
        </button>
      </div>
    </div>

    <!-- Active panel (kept alive to preserve scan/filter state across tab switches) -->
    <div class="flex-1 min-h-0">
      <KeepAlive>
        <component :is="panelComponent" :key="activeTab" />
      </KeepAlive>
    </div>
  </div>
</template>
