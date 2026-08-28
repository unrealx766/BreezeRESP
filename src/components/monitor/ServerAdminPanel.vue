<script setup lang="ts">
// Monitoring center — server administration panel.
// Sub-tabs: INFO viewer, CONFIG GET/SET (dangerous params blocked by backend),
// CLIENT LIST / KILL (confirm before kill).
import { ref, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Server, Settings, Users, RefreshCw, Search, Info } from "lucide-vue-next";
import type { ClientInfo, InfoNode } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";

const { t } = useI18n();
const connStore = useConnectionStore();
const confirmDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null);

const subTab = ref<"info" | "config" | "clients">("info");

// Reset when the connection changes
watch(() => connStore.activeConnectionId, () => {
  infoNodes.value = [];
  configPairs.value = [];
  clients.value = [];
  selectedNode.value = 0;
});

// ---- INFO ----
const infoNodes = ref<InfoNode[]>([]);
const selectedNode = ref(0);
const infoLoading = ref(false);
const infoError = ref("");

const currentNode = computed(() => infoNodes.value[selectedNode.value] ?? infoNodes.value[0] ?? null);

interface InfoSection {
  name: string;
  lines: Array<[string, string]>;
}

/** Parse raw INFO text into sections of key/value lines. */
const infoSections = computed<InfoSection[]>(() => {
  const text = currentNode.value?.info ?? "";
  const sections: InfoSection[] = [];
  let current: InfoSection | null = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;
    if (line.startsWith("#")) {
      current = { name: line.replace(/^#\s*/, ""), lines: [] };
      sections.push(current);
      continue;
    }
    const idx = line.indexOf(":");
    if (idx > 0) {
      if (!current) {
        current = { name: "-", lines: [] };
        sections.push(current);
      }
      current.lines.push([line.slice(0, idx), line.slice(idx + 1)]);
    }
  }
  return sections;
});

async function loadInfo() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  infoLoading.value = true;
  infoError.value = "";
  try {
    infoNodes.value = await tauriApi.serverAdmin.getInfo(connId);
    if (selectedNode.value >= infoNodes.value.length) selectedNode.value = 0;
  } catch (e) {
    infoError.value = e instanceof Error ? e.message : String(e);
  } finally {
    infoLoading.value = false;
  }
}

// ---- CONFIG ----
const configPattern = ref("*");
const configPairs = ref<Array<[string, string]>>([]);
const configLoading = ref(false);
const configSearch = ref("");


const filteredConfigs = computed(() => {
  const q = configSearch.value.trim().toLowerCase();
  if (!q) return configPairs.value;
  return configPairs.value.filter(([name]) => name.toLowerCase().includes(q));
});

async function loadConfig() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  configLoading.value = true;
  try {
    configPairs.value = await tauriApi.serverAdmin.configGet(connId, configPattern.value || "*");
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    configLoading.value = false;
  }
}

// ---- CLIENTS ----
const clients = ref<ClientInfo[]>([]);
const clientsLoading = ref(false);
const clientSearch = ref("");
const killingId = ref<number | null>(null);

const filteredClients = computed(() => {
  const q = clientSearch.value.trim().toLowerCase();
  if (!q) return clients.value;
  return clients.value.filter((c) =>
    c.addr.toLowerCase().includes(q) || c.name.toLowerCase().includes(q) || c.cmd.toLowerCase().includes(q)
  );
});

async function loadClients() {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  clientsLoading.value = true;
  try {
    clients.value = await tauriApi.serverAdmin.clientList(connId);
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    clientsLoading.value = false;
  }
}

async function killClient(client: ClientInfo) {
  const connId = connStore.activeConnectionId;
  if (!connId) return;
  const ok = await confirmDialog.value?.open({
    title: t("serverAdmin.clientKillConfirmTitle"),
    message: t("serverAdmin.clientKillConfirmMessage", { id: client.id, addr: client.addr || "-" }),
    confirmLabel: t("serverAdmin.kill"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;

  killingId.value = client.id;
  try {
    const killed = await tauriApi.serverAdmin.clientKill(connId, client.id);
    if (killed) {
      toast.success(t("serverAdmin.clientKillSuccess", { id: client.id }));
      await loadClients();
    } else {
      toast.error(t("serverAdmin.clientKillFailed", { id: client.id }));
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    killingId.value = null;
  }
}

/** CLIENT LIST flag letters → human description */
function flagLabel(flags: string): string {
  const map: Record<string, string> = {
    S: t("serverAdmin.flagSlave"), O: t("serverAdmin.flagSlaveMonitor"), M: t("serverAdmin.flagMaster"), P: t("serverAdmin.flagPubsub"),
    x: t("serverAdmin.flagMultiExec"), b: t("serverAdmin.flagBlocked"), d: t("serverAdmin.flagDirtyCas"), c: t("serverAdmin.flagClosing"),
    u: t("serverAdmin.flagUnblocked"), B: t("serverAdmin.flagBusyLoop"), A: t("serverAdmin.flagCloseAsap"), N: t("serverAdmin.flagNoFlag"),
  };
  return flags.split("").map((f) => map[f] ?? f).join(",");
}

function formatSeconds(s: number): string {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

function switchTab(tab: "info" | "config" | "clients") {
  subTab.value = tab;
  if (tab === "info" && infoNodes.value.length === 0) loadInfo();
  if (tab === "config" && configPairs.value.length === 0) loadConfig();
  if (tab === "clients" && clients.value.length === 0) loadClients();
}
</script>

<template>
  <div class="h-full flex flex-col min-w-[600px]">
    <ConfirmDialog ref="confirmDialog" />

    <!-- No connection state -->
    <div
      v-if="!connStore.activeConnectionId"
      class="flex-1 flex flex-col items-center justify-center text-text-muted"
    >
      <Server :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
      <p class="text-sm">{{ t("monitor.noConnection") }}</p>
    </div>

    <template v-else>
      <!-- Sub-tab bar -->
      <div class="flex items-center gap-1 mb-4 shrink-0 border-b border-border">
        <button
          @click="switchTab('info')"
          class="px-3 py-2 text-xs font-medium flex items-center gap-1.5 border-b-2 transition-colors -mb-px"
          :class="subTab === 'info' ? 'border-redis text-redis' : 'border-transparent text-text-secondary hover:text-text-primary'"
        >
          <Info :size="13" />
          {{ t("serverAdmin.tabInfo") }}
        </button>
        <button
          @click="switchTab('config')"
          class="px-3 py-2 text-xs font-medium flex items-center gap-1.5 border-b-2 transition-colors -mb-px"
          :class="subTab === 'config' ? 'border-redis text-redis' : 'border-transparent text-text-secondary hover:text-text-primary'"
        >
          <Settings :size="13" />
          {{ t("serverAdmin.tabConfig") }}
        </button>
        <button
          @click="switchTab('clients')"
          class="px-3 py-2 text-xs font-medium flex items-center gap-1.5 border-b-2 transition-colors -mb-px"
          :class="subTab === 'clients' ? 'border-redis text-redis' : 'border-transparent text-text-secondary hover:text-text-primary'"
        >
          <Users :size="13" />
          {{ t("serverAdmin.tabClient") }}
        </button>
      </div>

      <!-- ================= INFO ================= -->
      <div v-if="subTab === 'info'" class="flex-1 min-h-0 flex flex-col">
        <div class="flex items-center gap-2 mb-3 shrink-0 flex-wrap">
          <select
            v-if="infoNodes.length > 1"
            v-model.number="selectedNode"
            class="h-7 px-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary focus:outline-none focus:border-redis/50 transition-colors"
          >
            <option v-for="(node, idx) in infoNodes" :key="node.addr" :value="idx">{{ node.addr }}</option>
          </select>
          <button
            @click="loadInfo"
            :disabled="infoLoading"
            class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
          >
            <RefreshCw :size="13" :class="infoLoading ? 'animate-spin' : ''" />
            {{ t("common.refresh") }}
          </button>
        </div>

        <div v-if="infoLoading && infoNodes.length === 0" class="flex-1 flex items-center justify-center text-text-muted">
          <RefreshCw :size="24" class="animate-spin" />
        </div>
        <div v-else-if="infoError" class="px-3 py-2 rounded-lg border border-danger/30 bg-danger/10 text-xs text-danger">
          {{ infoError }}
        </div>
        <div v-else-if="currentNode" class="flex-1 min-h-0 overflow-y-auto space-y-4">
          <div v-for="section in infoSections" :key="section.name" class="rounded-lg border border-border">
            <h4 class="px-3 py-2 text-[11px] font-semibold text-redis uppercase tracking-wider bg-bg-secondary/50 border-b border-border">
              {{ section.name }}
            </h4>
            <div class="divide-y divide-border/40">
              <div v-for="[name, value] in section.lines" :key="name" class="grid grid-cols-[240px_1fr] gap-2 px-3 py-1.5 hover:bg-bg-secondary/40 transition-colors">
                <span class="text-[11px] font-mono text-text-muted truncate" :title="name">{{ name }}</span>
                <span class="text-[11px] font-mono text-text-primary break-all">{{ value }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- ================= CONFIG ================= -->
      <div v-if="subTab === 'config'" class="flex-1 min-h-0 flex flex-col">
        <div class="flex items-center gap-2 mb-3 shrink-0 flex-wrap">
          <input
            v-model="configPattern"
            type="text"
            :placeholder="t('serverAdmin.configPatternPlaceholder')"
            @keyup.enter="loadConfig"
            class="w-52 h-7 px-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
          />
          <button
            @click="loadConfig"
            :disabled="configLoading"
            class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
          >
            <RefreshCw :size="13" :class="configLoading ? 'animate-spin' : ''" />
            {{ t("serverAdmin.configGet") }}
          </button>
          <div class="relative flex-1 min-w-[160px] max-w-xs">
            <Search :size="13" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              v-model="configSearch"
              type="text"
              :placeholder="t('serverAdmin.configSearchPlaceholder')"
              class="w-full h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
            />
          </div>
        </div>

        <div v-if="configLoading && configPairs.length === 0" class="flex-1 flex items-center justify-center text-text-muted">
          <RefreshCw :size="24" class="animate-spin" />
        </div>
        <div v-else class="flex-1 min-h-0 overflow-y-auto rounded-lg border border-border">
          <div class="sticky top-0 z-10 grid grid-cols-[240px_1fr] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
            <span>{{ t("serverAdmin.configParam") }}</span>
            <span>{{ t("serverAdmin.configValue") }}</span>
          </div>
          <div class="divide-y divide-border/40">
            <div
              v-for="[name, value] in filteredConfigs"
              :key="name"
              class="grid grid-cols-[240px_1fr] gap-2 px-3 py-1.5 hover:bg-bg-secondary/40 transition-colors items-center"
            >
              <span class="text-[11px] font-mono text-text-secondary truncate" :title="name">{{ name }}</span>
              <span class="text-[11px] font-mono text-text-primary break-all">{{ value || t('serverAdmin.emptyValue') }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- ================= CLIENTS ================= -->
      <div v-if="subTab === 'clients'" class="flex-1 min-h-0 flex flex-col">
        <div class="flex items-center gap-2 mb-3 shrink-0 flex-wrap">
          <button
            @click="loadClients"
            :disabled="clientsLoading"
            class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1"
          >
            <RefreshCw :size="13" :class="clientsLoading ? 'animate-spin' : ''" />
            {{ t("common.refresh") }}
          </button>
          <div class="relative flex-1 min-w-[160px] max-w-xs">
            <Search :size="13" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              v-model="clientSearch"
              type="text"
              :placeholder="t('serverAdmin.clientSearchPlaceholder')"
              class="w-full h-7 pl-7 pr-2 text-xs rounded-lg border border-border bg-bg-secondary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
            />
          </div>
          <span class="text-[11px] text-text-muted">{{ t("serverAdmin.clientCount", { count: filteredClients.length }) }}</span>
        </div>

        <div v-if="clientsLoading && clients.length === 0" class="flex-1 flex items-center justify-center text-text-muted">
          <RefreshCw :size="24" class="animate-spin" />
        </div>
        <div v-else class="flex-1 min-h-0 overflow-y-auto rounded-lg border border-border">
          <div class="sticky top-0 z-10 grid grid-cols-[70px_140px_1fr_70px_70px_60px_90px_1fr_60px] gap-2 px-3 py-2 text-[11px] font-semibold text-text-muted uppercase tracking-wider bg-bg-primary border-b border-border">
            <span>{{ t("serverAdmin.colId") }}</span>
            <span>{{ t("serverAdmin.colAddr") }}</span>
            <span>{{ t("serverAdmin.colName") }}</span>
            <span>{{ t("serverAdmin.colAge") }}</span>
            <span>{{ t("serverAdmin.colIdle") }}</span>
            <span>{{ t("serverAdmin.colDb") }}</span>
            <span>{{ t("serverAdmin.colCmd") }}</span>
            <span>{{ t("serverAdmin.colFlags") }}</span>
            <span></span>
          </div>
          <div class="divide-y divide-border/40">
            <div
              v-for="client in filteredClients"
              :key="`${client.node}-${client.id}`"
              class="grid grid-cols-[70px_140px_1fr_70px_70px_60px_90px_1fr_60px] gap-2 px-3 py-1.5 hover:bg-bg-secondary/40 transition-colors items-center"
            >
              <span class="text-[11px] font-mono text-text-muted">{{ client.id }}</span>
              <span class="text-[11px] font-mono text-text-secondary truncate" :title="client.addr">{{ client.addr || '-' }}</span>
              <span class="text-[11px] font-mono text-text-primary truncate" :title="client.name">{{ client.name || '-' }}</span>
              <span class="text-[11px] font-mono text-text-muted">{{ formatSeconds(client.age) }}</span>
              <span class="text-[11px] font-mono text-text-muted">{{ formatSeconds(client.idle) }}</span>
              <span class="text-[11px] font-mono text-text-muted">{{ client.db }}</span>
              <span class="text-[11px] font-mono text-redis">{{ client.cmd || '-' }}</span>
              <span class="text-[10px] text-text-muted truncate" :title="flagLabel(client.flags)">{{ flagLabel(client.flags) }}</span>
              <div class="flex justify-end">
                <button
                  @click="killClient(client)"
                  :disabled="killingId === client.id"
                  class="text-[10px] px-1.5 py-0.5 rounded border border-danger/30 text-danger hover:bg-danger/10 transition-colors disabled:opacity-40"
                >
                  {{ killingId === client.id ? '...' : t("serverAdmin.kill") }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
