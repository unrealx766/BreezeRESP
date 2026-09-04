<script setup lang="ts">
// Key export / import dialog, opened from the data browser toolbar.
// Export: selected keys (textarea) or SCAN by pattern; JSON (readable) or
// DUMP (lossless RESTORE payload). Import: paste the export file path,
// preview it, then import with skip / replace policy.
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { X, Download, Upload, RefreshCw, Eye, FolderOpen } from "lucide-vue-next";
import type { ExportResult, ImportPolicy, ImportResult, KeyExportFormat } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "@/stores/connectionStore";
import { toast } from "@/utils/toast";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

const { t } = useI18n();
const connStore = useConnectionStore();
const confirmDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null);

const visible = ref(false);
const tab = ref<"export" | "import">("export");

function open(options?: { tab?: "export" | "import"; keys?: string[] }) {
  tab.value = options?.tab ?? "export";
  if (options?.keys && options.keys.length > 0) {
    exportMode.value = "keys";
    keysText.value = options.keys.join("\n");
  }
  visible.value = true;
}

function close() {
  if (exporting.value || importing.value) return;
  visible.value = false;
}

defineExpose({ open });

// ---- Export state ----
const exportMode = ref<"pattern" | "keys">("pattern");
const pattern = ref("*");
const limit = ref(1000);
const keysText = ref("");
const format = ref<KeyExportFormat>("json");
const exporting = ref(false);
const lastExport = ref<ExportResult | null>(null);

const parsedKeys = computed(() =>
  keysText.value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean)
);

async function doExport() {
  const connId = connStore.activeConnectionId;
  if (!connId || exporting.value) return;
  exporting.value = true;
  lastExport.value = null;
  try {
    let result: ExportResult;
    if (exportMode.value === "keys") {
      if (parsedKeys.value.length === 0) {
        toast.error(t("common.error"));
        return;
      }
      result = await tauriApi.keyTransfer.exportKeys(connId, parsedKeys.value, format.value);
    } else {
      result = await tauriApi.keyTransfer.exportKeysByPattern(connId, pattern.value || "*", format.value, limit.value);
    }
    lastExport.value = result;
    toast.success(
      t("keyTransfer.exportSuccess", { count: result.exported, path: result.path }),
      undefined,
      {
        label: t("keyTransfer.openLocation"),
        onClick: () => tauriApi.slowlog.openFileLocation(result.path).catch(() => toast.error(t("common.error"))),
      }
    );
    if (result.warnings.length > 0) {
      toast.warning(t("keyTransfer.exportWarnings", { count: result.warnings.length }));
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    exporting.value = false;
  }
}

// ---- Import state ----
const importPath = ref("");
const inspecting = ref(false);
const preview = ref<{ format: string; count: number } | null>(null);
const policy = ref<ImportPolicy>("skip");
const importing = ref(false);
const lastImport = ref<ImportResult | null>(null);

async function inspectFile() {
  const path = importPath.value.trim();
  if (!path) return;
  inspecting.value = true;
  preview.value = null;
  lastImport.value = null;
  try {
    const [fmt, count] = await tauriApi.keyTransfer.inspectImportFile(path);
    preview.value = { format: fmt, count };
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    inspecting.value = false;
  }
}

async function browseFile() {
  const selected = await openDialog({
    multiple: false,
    filters: [{ name: "Export Files", extensions: ["json", "dump"] }],
    title: t("keyTransfer.importBrowseTitle"),
  });
  if (selected && typeof selected === "string") {
    importPath.value = selected;
    // Auto-validate after file selection
    await inspectFile();
  }
}

async function doImport() {
  const connId = connStore.activeConnectionId;
  const path = importPath.value.trim();
  if (!connId || !path || importing.value) return;

  // Overwrite mode requires explicit confirmation
  if (policy.value === "replace") {
    const ok = await confirmDialog.value?.open({
      title: t("keyTransfer.importConfirmTitle"),
      message: t("keyTransfer.importConfirmMessage", {
        count: preview.value?.count ?? "?",
        policy: t("keyTransfer.policyReplace"),
      }),
      confirmLabel: t("common.confirm"),
      cancelLabel: t("common.cancel"),
      danger: true,
    });
    if (!ok) return;
  }

  importing.value = true;
  lastImport.value = null;
  try {
    const result = await tauriApi.keyTransfer.importKeys(connId, path, policy.value);
    lastImport.value = result;
    toast.success(t("keyTransfer.importSuccess", {
      succeeded: result.succeeded,
      skipped: result.skipped,
      failed: result.failed.length,
      total: result.total,
    }));
    if (result.failed.length > 0) {
      toast.warning(t("keyTransfer.importFailures", { count: result.failed.length }));
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e));
  } finally {
    importing.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="visible" class="fixed inset-0 z-[9000] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/40" @click="close" />

        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[560px] max-w-[92vw] max-h-[85vh] flex flex-col animate-in">
          <!-- Header -->
          <div class="flex items-center justify-between px-5 py-4 border-b border-border shrink-0">
            <div class="flex items-center gap-1 rounded-lg border border-border overflow-hidden">
              <button
                @click="tab = 'export'"
                class="px-3 h-7 text-xs flex items-center gap-1.5 transition-colors"
                :class="tab === 'export' ? 'bg-redis/10 text-redis font-medium' : 'text-text-secondary hover:bg-bg-hover'"
              >
                <Download :size="13" />
                {{ t("keyTransfer.exportTitle") }}
              </button>
              <button
                @click="tab = 'import'"
                class="px-3 h-7 text-xs flex items-center gap-1.5 transition-colors border-l border-border"
                :class="tab === 'import' ? 'bg-redis/10 text-redis font-medium' : 'text-text-secondary hover:bg-bg-hover'"
              >
                <Upload :size="13" />
                {{ t("keyTransfer.importTitle") }}
              </button>
            </div>
            <button @click="close" class="p-1 rounded hover:bg-bg-hover transition-colors">
              <X :size="16" class="text-text-muted" />
            </button>
          </div>

          <!-- Body -->
          <div class="px-5 py-4 overflow-y-auto">
            <!-- ============ Export tab ============ -->
            <template v-if="tab === 'export'">
              <!-- Mode selector -->
              <div class="flex items-center gap-3 mb-3">
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="exportMode" type="radio" value="pattern" class="accent-redis" />
                  {{ t("keyTransfer.exportPattern") }}
                </label>
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="exportMode" type="radio" value="keys" class="accent-redis" />
                  {{ t("keyTransfer.exportSelected") }}
                </label>
              </div>

              <!-- Pattern mode -->
              <div v-if="exportMode === 'pattern'" class="flex items-center gap-2 mb-3">
                <input
                  v-model="pattern"
                  type="text"
                  :placeholder="t('keyTransfer.exportPatternPlaceholder')"
                  class="flex-1 h-8 px-2.5 text-xs rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
                />
                <input
                  v-model.number="limit"
                  type="number"
                  min="1"
                  max="10000"
                  :title="t('keyTransfer.exportLimit')"
                  class="w-24 h-8 px-2.5 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none focus:border-redis/50 transition-colors"
                />
              </div>

              <!-- Keys mode -->
              <textarea
                v-else
                v-model="keysText"
                rows="4"
                placeholder="user:1001&#10;user:1002&#10;order:2001"
                class="w-full mb-3 px-2.5 py-2 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors resize-y"
              ></textarea>

              <!-- Format -->
              <div class="flex items-center gap-3 mb-4">
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="format" type="radio" value="json" class="accent-redis" />
                  {{ t("keyTransfer.formatJson") }}
                </label>
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="format" type="radio" value="dump" class="accent-redis" />
                  {{ t("keyTransfer.formatDump") }}
                </label>
              </div>

              <!-- Export action -->
              <button
                @click="doExport"
                :disabled="exporting || !connStore.activeConnectionId"
                class="w-full h-8 text-xs font-medium rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors flex items-center justify-center gap-1.5 disabled:opacity-50"
              >
                <RefreshCw v-if="exporting" :size="13" class="animate-spin" />
                <Download v-else :size="13" />
                {{ t("keyTransfer.export") }}
              </button>

              <!-- Last export result -->
              <div v-if="lastExport" class="mt-3 px-3 py-2 rounded-lg border border-border bg-bg-primary/60">
                <p class="text-[11px] text-text-secondary break-all font-mono">{{ lastExport.path }}</p>
                <div v-if="lastExport.warnings.length > 0" class="mt-2 max-h-24 overflow-y-auto space-y-0.5">
                  <p v-for="w in lastExport.warnings" :key="w.key" class="text-[10px] font-mono text-warning truncate" :title="`${w.key}: ${w.error}`">
                    {{ w.key }} — {{ w.error }}
                  </p>
                </div>
              </div>
            </template>

            <!-- ============ Import tab ============ -->
            <template v-else>
              <!-- File path -->
              <div class="mb-3">
                <p class="text-[11px] text-text-muted mb-1.5">{{ t("keyTransfer.importFilePath") }}</p>
                <div class="flex items-center gap-2">
                  <input
                    v-model="importPath"
                    type="text"
                    :placeholder="t('keyTransfer.importFilePathPlaceholder')"
                    @keyup.enter="inspectFile"
                    class="flex-1 h-8 px-2.5 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 transition-colors"
                  />
                  <button
                    @click="browseFile"
                    :disabled="inspecting"
                    :title="t('keyTransfer.importBrowse')"
                    class="h-8 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
                  >
                    <FolderOpen :size="12" />
                  </button>
                  <button
                    @click="inspectFile"
                    :disabled="inspecting || !importPath.trim()"
                    class="h-8 px-3 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
                  >
                    <RefreshCw v-if="inspecting" :size="12" class="animate-spin" />
                    <Eye v-else :size="12" />
                    {{ t("keyTransfer.inspect") }}
                  </button>
                </div>
              </div>

              <!-- Preview -->
              <div v-if="preview" class="px-3 py-2 mb-3 rounded-lg border border-info/30 bg-info/10 text-xs text-info">
                {{ t("keyTransfer.importPreview", { format: preview.format, count: preview.count }) }}
              </div>

              <!-- Policy -->
              <div class="flex flex-col gap-2 mb-4">
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="policy" type="radio" value="skip" class="accent-redis" />
                  {{ t("keyTransfer.policySkip") }}
                </label>
                <label class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
                  <input v-model="policy" type="radio" value="replace" class="accent-danger" />
                  <span :class="policy === 'replace' ? 'text-danger' : ''">{{ t("keyTransfer.policyReplace") }}</span>
                </label>
              </div>

              <!-- Import action -->
              <button
                @click="doImport"
                :disabled="importing || !preview || !connStore.activeConnectionId"
                class="w-full h-8 text-xs font-medium rounded-lg text-white transition-colors flex items-center justify-center gap-1.5 disabled:opacity-50"
                :class="policy === 'replace' ? 'bg-danger hover:bg-danger/90' : 'bg-redis hover:bg-redis-dark'"
              >
                <RefreshCw v-if="importing" :size="13" class="animate-spin" />
                <Upload v-else :size="13" />
                {{ t("keyTransfer.import") }}
              </button>

              <!-- Last import result -->
              <div v-if="lastImport" class="mt-3 px-3 py-2 rounded-lg border border-border bg-bg-primary/60">
                <div class="grid grid-cols-4 gap-2 text-center mb-1">
                  <div>
                    <p class="text-[10px] text-text-muted">Total</p>
                    <p class="text-sm font-semibold font-mono text-text-primary">{{ lastImport.total }}</p>
                  </div>
                  <div>
                    <p class="text-[10px] text-text-muted">OK</p>
                    <p class="text-sm font-semibold font-mono text-success">{{ lastImport.succeeded }}</p>
                  </div>
                  <div>
                    <p class="text-[10px] text-text-muted">Skip</p>
                    <p class="text-sm font-semibold font-mono text-warning">{{ lastImport.skipped }}</p>
                  </div>
                  <div>
                    <p class="text-[10px] text-text-muted">Fail</p>
                    <p class="text-sm font-semibold font-mono text-danger">{{ lastImport.failed.length }}</p>
                  </div>
                </div>
                <div v-if="lastImport.failed.length > 0" class="mt-2 max-h-24 overflow-y-auto space-y-0.5 border-t border-border pt-2">
                  <p v-for="f in lastImport.failed" :key="f.key" class="text-[10px] font-mono text-danger truncate" :title="`${f.key}: ${f.error}`">
                    {{ f.key }} — {{ f.error }}
                  </p>
                </div>
              </div>
            </template>
          </div>
        </div>
      </div>
    </Transition>
    <ConfirmDialog ref="confirmDialog" />
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
.animate-in {
  animation: dialog-in 0.2s ease-out;
}
@keyframes dialog-in {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(8px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
