<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { useI18n } from "vue-i18n";
import { usePipelineStore } from "@/stores/pipelineStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { allCommandTemplates } from "@/utils/commandTemplates";
import QpsChart from "@/components/charts/QpsChart.vue";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";
import { toast } from "@/utils/toast";
import { truncateValue } from "@/utils/format";
import {
  Plus, Play, Trash2, Eraser, GripVertical, CheckCircle, XCircle,
  Clock, Zap, Layers, ArrowUpDown, X,
  Save, FolderOpen, Download,
} from "lucide-vue-next";

const { t } = useI18n();
const pipeline = usePipelineStore();
const metrics = useMetricsStore();
const connStore = useConnectionStore();

const confirmDialog = ref<InstanceType<typeof ConfirmDialog>>();
const isConnected = computed(() => connStore.activeConnection?.status === "connected");

// Save dialog state
const showSaveDialog = ref(false);
const saveName = ref("");
const showSavedList = ref(false);

onMounted(() => {
  pipeline.loadSavedPipelines();
  // Restore argsText from store (survives page navigation)
  const m = new Map<string, string>();
  for (const cmd of pipeline.commands) {
    if (cmd.args.length > 0) m.set(cmd.id, cmd.args.join(" "));
  }
  argsText.value = m;
});

function openSaveDialog() {
  saveName.value = "";
  showSaveDialog.value = true;
}

function closeSaveDialog() {
  showSaveDialog.value = false;
}

async function doSave() {
  const name = saveName.value.trim();
  if (!name) {
    toast.warning(t("connection.nameRequired"));
    return;
  }
  try {
    await pipeline.saveCurrentPipeline(name);
    toast.success(t("pipeline.saveSuccess"));
    closeSaveDialog();
  } catch (e) {
    toast.error(typeof e === "string" ? e : (e as Error)?.message || String(e));
  }
}

async function doLoad(id: string) {
  const saved = pipeline.savedPipelines.find((p) => p.id === id);
  if (!saved) return;
  pipeline.loadPipeline(saved);
  // Sync argsText map
  const m = new Map(argsText.value);
  for (const cmd of pipeline.commands) {
    m.set(cmd.id, cmd.args.join(" "));
  }
  argsText.value = m;
}

async function doDelete(id: string) {
  const saved = pipeline.savedPipelines.find((p) => p.id === id);
  if (!saved) return;
  const confirmed = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("pipeline.confirmDeletePipeline", { name: saved.name }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!confirmed) return;
  try {
    await pipeline.deletePipeline(id);
  } catch (e) {
    console.error("Delete pipeline failed:", e);
  }
}

function formatTimestamp(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString() + " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// Raw args text per command (keyed by command id) to allow free typing with spaces
const argsText = ref(new Map<string, string>());

function getArgsText(id: string): string {
  return argsText.value.get(id) ?? "";
}

function setArgsText(id: string, val: string) {
  const m = new Map(argsText.value);
  m.set(id, val);
  argsText.value = m;
}

function initArgsText(cmd: any) {
  if (!argsText.value.has(cmd.id) && cmd.args.length > 0) {
    const m = new Map(argsText.value);
    m.set(cmd.id, cmd.args.join(" "));
    argsText.value = m;
  }
}

const commandTemplates = allCommandTemplates;

function addFromTemplate(tpl: typeof commandTemplates[0]) {
  const argsStr = tpl.args.join(" ");
  pipeline.addCommand(tpl.cmd, [...tpl.args]);
  // Set argsText for the newly added command (it's the last one)
  const lastCmd = pipeline.commands[pipeline.commands.length - 1];
  if (lastCmd) {
    const m = new Map(argsText.value);
    m.set(lastCmd.id, argsStr);
    argsText.value = m;
  }
}

// Drag and drop
let dragIdx: number | null = null;
function onDragStart(idx: number) { dragIdx = idx; }
function onDragOver(e: DragEvent) { e.preventDefault(); }
function onDrop(idx: number) {
  if (dragIdx !== null && dragIdx !== idx) {
    pipeline.reorderCommands(dragIdx, idx);
  }
  dragIdx = null;
}
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-auto min-w-[600px]">
    <!-- Header + QPS -->
    <div class="flex items-start justify-between gap-3 mb-4 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Layers :size="20" class="text-redis" />
          {{ t("pipeline.title") }}
        </h2>
        <p class="text-sm text-text-muted mt-1">{{ t("pipeline.commandsQueued", { count: pipeline.commandCount }) }}</p>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <button @click="openSaveDialog()" :disabled="pipeline.commandCount === 0 || !isConnected"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
          <Save :size="13" /> {{ t("pipeline.saveAs") }}
        </button>
        <button @click="showSavedList = !showSavedList"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors"
          :class="showSavedList ? 'border-redis text-redis' : ''">
          <FolderOpen :size="13" /> {{ t("pipeline.savedPipelines") }}
          <span v-if="pipeline.savedPipelines.length" class="text-[10px] bg-redis/10 text-redis rounded-full px-1.5">{{ pipeline.savedPipelines.length }}</span>
        </button>
        <button @click="pipeline.clearResults()" :disabled="!pipeline.hasResults || !isConnected"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
          <Eraser :size="13" /> {{ t("pipeline.clearResults") }}
        </button>
        <button @click="pipeline.clearAll()" :disabled="pipeline.commandCount === 0 || !isConnected"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs text-danger bg-danger/5 border border-danger/20 rounded-lg hover:bg-danger/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
          <Trash2 :size="13" /> {{ t("pipeline.clearAll") }}
        </button>
        <button @click="pipeline.executeAll()" :disabled="pipeline.commandCount === 0 || pipeline.executing || !isConnected"
          class="inline-flex items-center justify-center gap-1.5 w-36 h-9 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
          :title="!isConnected ? t('status.noConnection') : ''">
          <Play :size="14" />
          <span>{{ pipeline.executing ? t("pipeline.executing") : t("pipeline.executeAll") }}</span>
        </button>
      </div>
    </div>

    <!-- Save Dialog -->
    <div v-if="showSaveDialog" class="card p-4 mb-4 flex items-end gap-3">
      <div class="flex-1">
        <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t("pipeline.pipelineName") }}</label>
        <input
          v-model="saveName"
          @keyup.enter="doSave"
          @keyup.escape="closeSaveDialog"
          :placeholder="t('pipeline.pipelineNamePlaceholder')"
          class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20"
          autofocus
        />
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <button @click="doSave" class="inline-flex items-center gap-1.5 px-4 py-2 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors shadow-sm">
          <Save :size="14" /> {{ t("pipeline.savePipeline") }}
        </button>
        <button @click="closeSaveDialog" class="px-3 py-2 text-sm text-text-muted hover:text-text-primary transition-colors">
          <X :size="14" />
        </button>
      </div>
    </div>

    <!-- QPS Chart -->
    <div class="card p-3 mb-4">
      <p class="text-xs font-medium text-text-secondary mb-2 flex items-center gap-1.5">
        <Zap :size="12" class="text-redis" /> {{ t("status.qps") }}: {{ metrics.qps }}
      </p>
      <QpsChart :data="metrics.qpsHistory" :height="80" />
    </div>

    <div class="flex gap-4 flex-1 min-h-0">
      <!-- Commands List -->
      <div class="flex-1 flex flex-col min-w-0">
        <!-- Template buttons -->
        <div class="flex flex-wrap gap-1.5 mb-3">
          <span class="text-xs text-text-muted self-center mr-1">{{ t("pipeline.templates") }}:</span>
          <button v-for="tpl in commandTemplates" :key="tpl.label"
            @click="addFromTemplate(tpl)"
            :disabled="!isConnected"
            class="px-2 py-0.5 text-[11px] font-mono bg-bg-primary border border-border rounded text-text-secondary hover:border-redis hover:text-redis transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
            {{ tpl.label }}
          </button>
        </div>

        <!-- Command nodes -->
        <div class="flex-1 space-y-2 overflow-y-auto pr-1">
          <div v-if="pipeline.commandCount === 0" class="flex flex-col items-center py-12 text-text-muted">
            <Layers :size="32" class="mb-2 opacity-30" />
            <p class="text-sm">{{ t("pipeline.emptyHint") }}</p>
          </div>

          <div v-for="(cmd, idx) in pipeline.commands" :key="cmd.id"
            class="card p-3 flex items-start gap-3 group"
            draggable="true"
            @dragstart="onDragStart(idx)"
            @dragover="onDragOver"
            @drop="onDrop(idx)"
          >
            <!-- Drag handle -->
            <div class="cursor-grab pt-1 text-text-muted hover:text-text-secondary">
              <GripVertical :size="14" />
            </div>

            <!-- Index -->
            <span class="text-xs font-mono text-text-muted w-5 text-right pt-1.5 shrink-0">{{ idx + 1 }}</span>

            <!-- Command input -->
            <div class="flex-1 min-w-0">
              <div class="flex gap-2">
                <input v-model="cmd.command" :placeholder="t('pipeline.command')" :disabled="!isConnected"
                  class="w-28 px-2 py-1.5 text-xs font-mono font-semibold bg-bg-primary border border-border rounded focus:outline-none focus:border-redis uppercase disabled:opacity-50 disabled:cursor-not-allowed" />
                <input :value="getArgsText(cmd.id)" @input="(e: any) => { setArgsText(cmd.id, e.target.value); cmd.args = (e.target.value as string).split(/\s+/).filter(Boolean); }" @focus="() => initArgsText(cmd)" :placeholder="t('pipeline.arguments')" :disabled="!isConnected"
                  class="flex-1 px-2 py-1.5 text-xs font-mono bg-bg-primary border border-border rounded focus:outline-none focus:border-redis disabled:opacity-50 disabled:cursor-not-allowed" />
              </div>
              <!-- Result -->
              <div v-if="cmd.result" class="mt-2">
                <div class="flex items-center gap-1.5 text-xs mb-1">
                  <CheckCircle v-if="cmd.result.success" :size="12" class="text-success" />
                  <XCircle v-else :size="12" class="text-danger" />
                  <span :class="cmd.result.success ? 'text-success' : 'text-danger'" class="font-medium">
                    {{ cmd.result.success ? t("pipeline.success") : t("pipeline.failed") }}
                  </span>
                  <span class="text-text-muted ml-auto flex items-center gap-1">
                    <Clock :size="10" /> {{ cmd.result.latencyMs }}ms
                  </span>
                </div>
                <div class="px-3 py-2 text-xs font-mono rounded-lg max-h-64 overflow-y-auto break-all whitespace-pre-wrap min-w-0 border"
                  :class="cmd.result.success
                    ? 'bg-info/5 border-info/20 text-text-primary'
                    : 'bg-danger/5 border-danger/20 text-danger'">
                  {{ cmd.result.success ? truncateValue(cmd.result.value) : truncateValue(cmd.result.error) }}
                </div>
              </div>
            </div>

            <!-- Delete -->
            <button @click="pipeline.removeCommand(cmd.id)" :disabled="!isConnected"
              class="w-6 h-6 flex items-center justify-center rounded hover:bg-danger/10 opacity-0 group-hover:opacity-100 transition-opacity shrink-0 mt-0.5 disabled:opacity-50 disabled:cursor-not-allowed">
              <Trash2 :size="12" class="text-danger" />
            </button>
          </div>
        </div>

        <!-- Add button (outside scrollable area, always visible at bottom) -->
        <button @click="pipeline.addCommand()" :disabled="!isConnected" class="w-full py-2 mt-2 border-2 border-dashed border-border rounded-lg text-xs text-text-muted hover:border-redis hover:text-redis transition-colors flex items-center justify-center gap-1.5 shrink-0 disabled:opacity-50 disabled:cursor-not-allowed">
          <Plus :size="14" /> {{ t("pipeline.addCommand") }}
        </button>
      </div>

      <!-- Saved Pipelines Panel -->
      <div class="w-64 shrink-0 flex flex-col min-h-0" v-if="showSavedList">
        <div class="card p-4 flex flex-col min-h-0">
          <div class="flex items-center justify-between mb-3 shrink-0">
            <h3 class="text-sm font-semibold text-text-primary">{{ t("pipeline.savedPipelines") }}</h3>
            <span class="text-[10px] text-text-muted">{{ pipeline.savedPipelines.length }}</span>
          </div>
          <div class="space-y-2 flex-1 overflow-y-auto min-h-0">
            <p v-if="pipeline.savedPipelines.length === 0" class="text-xs text-text-muted text-center py-6">
              {{ t("pipeline.noSavedPipelines") }}
            </p>
            <div
              v-for="saved in pipeline.savedPipelines"
              :key="saved.id"
              class="p-2.5 rounded-lg border border-border-light hover:border-redis/30 transition-colors group"
            >
              <p class="text-xs font-semibold text-text-primary truncate" :title="saved.name">{{ saved.name }}</p>
              <p class="text-[10px] text-text-muted mt-0.5">
                {{ saved.commands.length }} cmds · {{ formatTimestamp(saved.createdAt) }}
              </p>
              <div class="flex gap-1 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
                <button @click="doLoad(saved.id)"
                  class="flex-1 inline-flex items-center justify-center gap-1 px-2 py-1 text-[10px] font-medium text-redis bg-redis/8 rounded hover:bg-redis/15 transition-colors">
                  <Download :size="10" /> {{ t("pipeline.loadPipeline") }}
                </button>
                <button @click="doDelete(saved.id)"
                  class="inline-flex items-center justify-center px-2 py-1 text-[10px] text-text-muted hover:text-danger hover:bg-danger/8 rounded transition-colors">
                  <Trash2 :size="10" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Results Summary Panel -->
      <div class="w-64 shrink-0" v-if="pipeline.hasResults">
        <div class="card p-4 space-y-4">
          <h3 class="text-sm font-semibold text-text-primary">{{ t("pipeline.resultsSummary") }}</h3>

          <div class="space-y-3">
            <div class="flex justify-between text-xs">
              <span class="text-text-muted flex items-center gap-1"><Clock :size="11" /> {{ t("pipeline.totalLatency") }}</span>
              <span class="font-mono font-semibold text-text-primary">{{ pipeline.totalLatency?.toFixed(1) }}ms</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-text-muted">{{ t("pipeline.individualSum") }}</span>
              <span class="font-mono text-text-secondary">{{ pipeline.individualLatencySum?.toFixed(1) }}ms</span>
            </div>
            <div class="divider" />
            <div class="flex justify-between text-xs">
              <span class="text-text-muted flex items-center gap-1"><ArrowUpDown :size="11" /> {{ t("pipeline.rttSaving") }}</span>
              <span class="font-mono font-semibold" :class="(pipeline.rttSaving ?? 0) > 0 ? 'text-success' : 'text-text-muted'">
                {{ pipeline.rttSaving !== null ? `${pipeline.rttSaving}%` : '-' }}
              </span>
            </div>
          </div>

          <div class="bg-success/5 border border-success/20 rounded-lg p-3 text-center">
            <p class="text-lg font-bold text-success">{{ pipeline.executedCount }}/{{ pipeline.commandCount }}</p>
            <p class="text-[11px] text-text-muted">{{ t("pipeline.success") }}</p>
          </div>
        </div>
      </div>
    </div>

    <ConfirmDialog ref="confirmDialog" />
  </div>
</template>
