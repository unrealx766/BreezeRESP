<script setup lang="ts">
// Monitoring center — cluster topology panel.
// Only meaningful for cluster connections; standalone connections see a hint.
import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { Network, RefreshCw, Server, Database, Users } from "lucide-vue-next";
import type { ClusterTopology } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";

const { t } = useI18n();
const connStore = useConnectionStore();

const topology = ref<ClusterTopology | null>(null);
const loading = ref(false);
const error = ref("");

const isClusterConnection = computed(() => connStore.activeConnection?.cluster === true);

const masters = computed(() => (topology.value?.nodes ?? []).filter((n) => n.role === "master"));
const replicas = computed(() => (topology.value?.nodes ?? []).filter((n) => n.role === "replica"));

const slotPercent = computed(() => {
  const topo = topology.value;
  if (!topo || !topo.totalSlots) return 0;
  return (topo.slotsAssigned / topo.totalSlots) * 100;
});

const totalMemory = computed(() =>
  masters.value.reduce((sum, n) => sum + n.usedMemory, 0)
);

async function loadTopology() {
  const connId = connStore.activeConnectionId;
  if (!connId || !isClusterConnection.value) return;
  loading.value = true;
  error.value = "";
  try {
    topology.value = await tauriApi.serverAdmin.getClusterTopology(connId);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    toast.error(error.value);
  } finally {
    loading.value = false;
  }
}

function nodeHealthClass(flags: string): string {
  if (flags.includes("fail")) return "bg-danger";
  if (flags.includes("disconnected") || flags.includes("noaddr")) return "bg-warning";
  return "bg-success";
}

function formatSlots(slots: Array<[number, number]>): string {
  if (slots.length === 0) return t("cluster.noSlots");
  return slots.map(([start, end]) => (start === end ? `${start}` : `${start}-${end}`)).join(", ");
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0B";
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(2)}KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(2)}MB`;
  return `${(bytes / 1073741824).toFixed(2)}GB`;
}

/** Find replica nodes replicating from the given master */
function replicasOf(masterId: string) {
  return replicas.value.filter((r) => r.masterId === masterId);
}

watch(() => connStore.activeConnectionId, () => {
  topology.value = null;
  error.value = "";
  if (isClusterConnection.value) loadTopology();
});

onMounted(() => {
  if (connStore.activeConnectionId && isClusterConnection.value) loadTopology();
});
</script>

<template>
  <div class="h-full flex flex-col min-w-[600px]">
    <!-- No connection -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Network :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("monitor.noConnection") }}</p>
    </div>

    <!-- Not a cluster connection -->
    <div
      v-else-if="!isClusterConnection"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Network :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("cluster.notCluster") }}</p>
    </div>

    <template v-else>
      <!-- Toolbar -->
      <div class="flex items-center gap-2 mb-4 shrink-0">
        <button
          @click="loadTopology"
          :disabled="loading"
          class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
        >
          <RefreshCw :size="13" :class="loading ? 'animate-spin' : ''" />
          {{ t("common.refresh") }}
        </button>
        <span v-if="loading" class="text-[11px] text-text-muted">{{ t("common.loading") }}</span>
      </div>

      <div v-if="error" class="px-3 py-2 mb-3 rounded-lg border border-danger/30 bg-danger/10 text-xs text-danger shrink-0">
        {{ error }}
      </div>

      <template v-if="topology && topology.clusterEnabled">
        <!-- Summary cards -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-4 shrink-0">
          <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
            <Server :size="18" class="text-redis shrink-0" />
            <div class="min-w-0">
              <p class="text-[11px] text-text-muted">{{ t("cluster.masterCount") }}</p>
              <p class="text-sm font-semibold text-text-primary font-mono">{{ masters.length }}</p>
            </div>
          </div>
          <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
            <Server :size="18" class="text-info shrink-0" />
            <div class="min-w-0">
              <p class="text-[11px] text-text-muted">{{ t("cluster.replicaCount") }}</p>
              <p class="text-sm font-semibold text-text-primary font-mono">{{ replicas.length }}</p>
            </div>
          </div>
          <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
            <Database :size="18" class="text-success shrink-0" />
            <div class="min-w-0">
              <p class="text-[11px] text-text-muted">{{ t("cluster.totalMemory") }}</p>
              <p class="text-sm font-semibold text-text-primary font-mono">{{ formatBytes(totalMemory) }}</p>
            </div>
          </div>
          <div class="flex items-center gap-2.5 p-3 rounded-lg border border-border bg-bg-secondary/50">
            <Users :size="18" class="text-warning shrink-0" />
            <div class="min-w-0">
              <p class="text-[11px] text-text-muted">{{ t("cluster.totalClients") }}</p>
              <p class="text-sm font-semibold text-text-primary font-mono">
                {{ topology.nodes.reduce((s, n) => s + n.connectedClients, 0) }}
              </p>
            </div>
          </div>
        </div>

        <!-- Slot coverage -->
        <div class="rounded-lg border border-border p-4 mb-4 shrink-0">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-semibold text-text-primary">{{ t("cluster.slotCoverage") }}</span>
            <span class="text-[11px] font-mono text-text-muted">
              {{ topology.slotsAssigned }} / {{ topology.totalSlots }} ({{ slotPercent.toFixed(1) }}%)
            </span>
          </div>
          <div class="h-2 rounded-full bg-bg-secondary overflow-hidden">
            <div
              class="h-full rounded-full transition-all duration-500"
              :class="slotPercent >= 100 ? 'bg-success' : 'bg-warning'"
              :style="{ width: `${Math.min(100, slotPercent)}%` }"
            ></div>
          </div>
        </div>

        <!-- Node cards grouped by master -->
        <div class="flex-1 min-h-0 overflow-y-auto space-y-3">
          <div v-for="master in masters" :key="master.id" class="rounded-lg border border-border">
            <!-- Master row -->
            <div class="px-4 py-3 flex items-center gap-3 flex-wrap">
              <span class="w-2.5 h-2.5 rounded-full shrink-0" :class="nodeHealthClass(master.flags)"></span>
              <span class="text-sm font-mono font-semibold text-text-primary">{{ master.addr }}</span>
              <span class="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-redis/10 text-redis">MASTER</span>
              <span class="text-[11px] font-mono text-text-muted" :title="master.id">#{{ master.id.slice(0, 8) }}</span>
              <span class="text-[11px] font-mono text-text-secondary">{{ formatBytes(master.usedMemory) }}</span>
              <span class="text-[11px] text-text-muted">{{ master.connectedClients }} {{ t("cluster.clientsUnit") }}</span>
              <span class="text-[11px] font-mono text-text-muted ml-auto max-w-[45%] truncate text-right" :title="formatSlots(master.slots)">
                {{ formatSlots(master.slots) }}
              </span>
            </div>
            <!-- Replicas of this master -->
            <div v-for="replica in replicasOf(master.id)" :key="replica.id" class="px-4 py-2 border-t border-border/50 flex items-center gap-3 flex-wrap bg-bg-secondary/30">
              <span class="w-2 h-2 rounded-full shrink-0 ml-3" :class="nodeHealthClass(replica.flags)"></span>
              <span class="text-xs font-mono text-text-secondary">{{ replica.addr }}</span>
              <span class="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-info/10 text-info">REPLICA</span>
              <span class="text-[11px] font-mono text-text-muted" :title="replica.id">#{{ replica.id.slice(0, 8) }}</span>
              <span class="text-[11px] font-mono text-text-secondary ml-auto">{{ formatBytes(replica.usedMemory) }}</span>
            </div>
          </div>

          <!-- Orphan replicas (master not in list) -->
          <div
            v-for="replica in replicas.filter((r) => !masters.some((m) => m.id === r.masterId))"
            :key="replica.id"
            class="rounded-lg border border-border px-4 py-2 flex items-center gap-3 flex-wrap"
          >
            <span class="w-2 h-2 rounded-full shrink-0" :class="nodeHealthClass(replica.flags)"></span>
            <span class="text-xs font-mono text-text-secondary">{{ replica.addr }}</span>
            <span class="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-info/10 text-info">REPLICA</span>
            <span class="text-[11px] text-text-muted">{{ t("cluster.orphanReplica") }}</span>
          </div>
        </div>
      </template>

      <!-- Cluster disabled on server side -->
      <div
        v-else-if="topology && !topology.clusterEnabled"
        class="flex-1 flex flex-col items-center justify-center text-text-muted"
      >
        <Network :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
        <p class="text-sm">{{ t("cluster.disabled") }}</p>
      </div>
    </template>
  </div>
</template>
