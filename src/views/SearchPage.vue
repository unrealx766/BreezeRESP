<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import {
  Radar, RefreshCw, Plus, Trash2, Search as SearchIcon, Play, Database,
  TriangleAlert, Inbox, X,
} from "lucide-vue-next";
import { useSearchStore } from "@/stores/searchStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCapabilityStore } from "@/stores/capabilityStore";
import { tauriApi } from "@/services/tauriApi";
import { toast } from "@/utils/toast";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";
import type { FtFieldSpec } from "@/types";

const { t } = useI18n();
const connStore = useConnectionStore();
const searchStore = useSearchStore();
const capStore = useCapabilityStore();

const isConnected = computed(() => connStore.activeConnection?.status === "connected");
const connId = computed(() => connStore.activeConnectionId ?? "");
const capability = computed(() => capStore.activeCapability);
const searchSupported = computed(() => capability.value?.searchSupported ?? true);
const vectorSupported = computed(() => capability.value?.vectorSearchSupported ?? true);

const confirmDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null);

// Query panel
const queryMode = ref<"text" | "knn">("text");
const textQuery = ref("*");
const knnField = ref("");
const knnK = ref(10);
const knnVector = ref("");

// Create index wizard
const showCreateModal = ref(false);
const createName = ref("");
const createOnType = ref<"HASH" | "JSON">("HASH");
const createPrefixes = ref("");
const createFields = ref<FtFieldSpec[]>([
  { identifier: "", fieldType: "TEXT" },
]);
const creating = ref(false);

const vectorFields = computed(() =>
  (searchStore.indexInfo?.fields ?? []).filter((f) => f.fieldType.toUpperCase() === "VECTOR"),
);

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

async function refreshAll() {
  if (!connId.value) return;
  const cap = await capStore.fetchCapability(connId.value);
  if (cap && !cap.searchSupported) {
    searchStore.reset();
    return;
  }
  try {
    await searchStore.loadIndexes(connId.value);
    if (searchStore.selectedIndex && searchStore.indexes.includes(searchStore.selectedIndex)) {
      await selectIndex(searchStore.selectedIndex);
    } else if (searchStore.indexes.length > 0) {
      await selectIndex(searchStore.indexes[0]);
    } else {
      searchStore.indexInfo = null;
      searchStore.selectedIndex = "";
      searchStore.searchResult = null;
    }
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

async function selectIndex(index: string) {
  try {
    await searchStore.loadIndexInfo(connId.value, index);
    knnField.value = vectorFields.value[0]?.identifier ?? "";
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

async function runSearch() {
  if (!searchStore.selectedIndex) return;
  try {
    if (queryMode.value === "text") {
      await searchStore.search(connId.value, searchStore.selectedIndex, textQuery.value || "*", { withScores: true });
    } else {
      const field = knnField.value;
      const nums = knnVector.value
        .split(/[\s,]+/)
        .filter(Boolean)
        .map(Number);
      if (!field || nums.length === 0 || nums.some((n) => Number.isNaN(n))) {
        toast.error(t("search.knnVectorInvalid"));
        return;
      }
      // Pack FLOAT32 vector into raw bytes; a number[] survives the JSON
      // IPC bridge losslessly (JS strings would mangle bytes >= 0x80).
      const buf = new ArrayBuffer(nums.length * 4);
      const view = new DataView(buf);
      nums.forEach((n, i) => view.setFloat32(i * 4, n, true));
      const blob = Array.from(new Uint8Array(buf));
      const query = `*=>[KNN ${knnK.value} @${field} $vec AS score]`;
      await searchStore.search(connId.value, searchStore.selectedIndex, query, {
        params: [["vec", blob]],
        withScores: true,
      });
    }
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

// ---------------------------------------------------------------------------
// Create / drop index
// ---------------------------------------------------------------------------

function openCreateModal() {
  createName.value = "";
  createOnType.value = "HASH";
  createPrefixes.value = "";
  createFields.value = [{ identifier: "", fieldType: "TEXT" }];
  showCreateModal.value = true;
}

function addCreateField() {
  createFields.value.push({ identifier: "", fieldType: "TEXT" });
}

function removeCreateField(idx: number) {
  createFields.value.splice(idx, 1);
}

async function submitCreate() {
  const fields = createFields.value.filter((f) => f.identifier.trim() !== "");
  if (!createName.value.trim() || fields.length === 0) return;
  creating.value = true;
  try {
    await tauriApi.jsonsearch.ftCreate(connId.value, {
      name: createName.value.trim(),
      onType: createOnType.value,
      prefixes: createPrefixes.value.split(/\s+/).filter(Boolean),
      fields: fields.map((f) => ({
        ...f,
        identifier: f.identifier.trim(),
        vectorDim: f.fieldType === "VECTOR" ? Number(f.vectorDim) || 0 : null,
      })),
    });
    showCreateModal.value = false;
    toast.success(t("search.createSuccess", { name: createName.value.trim() }));
    await refreshAll();
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  } finally {
    creating.value = false;
  }
}

async function dropIndex(index: string) {
  const ok = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("search.dropConfirm", { name: index }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!ok) return;
  try {
    await tauriApi.jsonsearch.ftDropIndex(connId.value, index, false);
    toast.success(t("search.dropSuccess"));
    if (searchStore.selectedIndex === index) {
      searchStore.selectedIndex = "";
      searchStore.indexInfo = null;
      searchStore.searchResult = null;
    }
    await refreshAll();
  } catch (e: any) {
    toast.error(e?.toString() ?? t("common.error"));
  }
}

function formatDim(field: { vectorDim: number | null }): string {
  return field.vectorDim != null ? String(field.vectorDim) : "-";
}

onMounted(() => {
  if (connId.value) refreshAll();
});

watch(connId, (id, old) => {
  if (id !== old) {
    searchStore.reset();
    if (id) refreshAll();
  }
});
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-hidden min-w-[760px]">
    <!-- Header -->
    <div class="flex items-start justify-between gap-3 mb-4 shrink-0 flex-wrap">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Radar :size="20" class="text-redis" />
          {{ t("search.title") }}
        </h2>
        <p class="text-sm text-text-muted mt-1">{{ t("search.description") }}</p>
      </div>
      <button
        @click="refreshAll"
        :disabled="!isConnected || searchStore.loadingIndexes"
        class="h-7 px-2.5 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors flex items-center gap-1 disabled:opacity-40"
      >
        <RefreshCw :size="13" :class="searchStore.loadingIndexes ? 'animate-spin' : ''" />
        {{ t("streams.refresh") }}
      </button>
    </div>

    <!-- Not connected -->
    <div v-if="!isConnected" class="flex-1 flex items-center justify-center">
      <div class="text-center text-text-muted">
        <Inbox :size="40" class="mx-auto mb-3 opacity-40" />
        <p class="text-sm">{{ t("status.noConnection") }}</p>
      </div>
    </div>

    <!-- Module not installed guidance -->
    <div v-else-if="!searchSupported" class="flex-1 flex items-center justify-center">
      <div class="max-w-md text-center rounded-2xl border border-border bg-bg-secondary p-8">
        <TriangleAlert :size="36" class="mx-auto mb-4 text-warning" />
        <h3 class="text-base font-semibold text-text-primary mb-2">{{ t("search.moduleMissingTitle") }}</h3>
        <p class="text-sm text-text-muted leading-relaxed mb-4">{{ t("search.moduleMissingDesc") }}</p>
        <a
          href="https://redis.io/downloads/"
          target="_blank"
          rel="noopener"
          class="inline-flex items-center gap-1.5 px-4 py-2 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors"
        >
          {{ t("search.downloadStack") }}
        </a>
      </div>
    </div>

    <template v-else>
      <div class="flex-1 flex gap-4 min-h-0">
        <!-- Left: index list -->
        <div class="w-60 shrink-0 flex flex-col rounded-xl border border-border bg-bg-secondary overflow-hidden">
          <div class="p-2 border-b border-border shrink-0 flex items-center justify-between">
            <span class="text-xs font-medium text-text-secondary px-1">{{ t("search.indexList") }}</span>
            <button
              @click="openCreateModal"
              class="h-6 px-2 text-[11px] rounded-md bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1"
            >
              <Plus :size="12" />
              {{ t("search.createIndex") }}
            </button>
          </div>
          <div class="flex-1 overflow-y-auto p-1.5 space-y-0.5">
            <p v-if="searchStore.indexes.length === 0" class="text-xs text-text-muted text-center py-6">
              {{ searchStore.loadingIndexes ? t("common.loading") : t("search.noIndexes") }}
            </p>
            <div
              v-for="idx in searchStore.indexes"
              :key="idx"
              class="group flex items-center rounded-lg transition-colors"
              :class="searchStore.selectedIndex === idx ? 'bg-redis/10' : 'hover:bg-bg-hover'"
            >
              <button
                @click="selectIndex(idx)"
                class="flex-1 min-w-0 text-left px-2.5 py-1.5 text-xs font-mono truncate"
                :class="searchStore.selectedIndex === idx ? 'text-redis' : 'text-text-secondary hover:text-text-primary'"
                :title="idx"
              >
                {{ idx }}
              </button>
              <button
                @click="dropIndex(idx)"
                class="p-1 mr-1 rounded text-text-muted opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-red-500/10 transition-all"
                :title="t('common.delete')"
              >
                <Trash2 :size="12" />
              </button>
            </div>
          </div>
        </div>

        <!-- Right: index detail + query -->
        <div class="flex-1 min-w-0 flex flex-col gap-4 overflow-y-auto pr-1">
          <template v-if="searchStore.indexInfo">
            <!-- Index definition -->
            <div class="rounded-xl border border-border bg-bg-secondary p-4 shrink-0">
              <div class="flex items-center gap-2 mb-3 flex-wrap">
                <Database :size="15" class="text-redis" />
                <span class="text-sm font-semibold text-text-primary font-mono">{{ searchStore.indexInfo.name }}</span>
                <span class="text-[11px] px-2 py-0.5 rounded-full bg-redis/10 text-redis">
                  {{ t("search.numDocs", { count: searchStore.indexInfo.numDocs }) }}
                </span>
                <span
                  v-for="p in searchStore.indexInfo.prefixes"
                  :key="p"
                  class="text-[11px] px-2 py-0.5 rounded-full bg-bg-hover text-text-secondary font-mono"
                >
                  {{ p }}*
                </span>
              </div>
              <table class="w-full text-xs">
                <thead>
                  <tr class="text-left text-text-muted border-b border-border">
                    <th class="px-2 py-1.5 font-medium">{{ t("search.colField") }}</th>
                    <th class="px-2 py-1.5 font-medium">{{ t("search.colAlias") }}</th>
                    <th class="px-2 py-1.5 font-medium">{{ t("search.colType") }}</th>
                    <th class="px-2 py-1.5 font-medium">{{ t("search.colVectorInfo") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="f in searchStore.indexInfo.fields" :key="f.identifier" class="border-b border-border/50">
                    <td class="px-2 py-1.5 font-mono text-text-primary">{{ f.identifier }}</td>
                    <td class="px-2 py-1.5 font-mono text-text-muted">{{ f.attribute === f.identifier ? "-" : f.attribute }}</td>
                    <td class="px-2 py-1.5">
                      <span
                        class="text-[11px] px-1.5 py-0.5 rounded"
                        :class="f.fieldType.toUpperCase() === 'VECTOR' ? 'bg-purple-500/10 text-purple-400' : 'bg-bg-hover text-text-secondary'"
                      >
                        {{ f.fieldType }}
                      </span>
                    </td>
                    <td class="px-2 py-1.5 text-text-muted font-mono">
                      <template v-if="f.fieldType.toUpperCase() === 'VECTOR'">
                        {{ f.vectorAlgorithm ?? "-" }} · dim {{ formatDim(f) }} · {{ f.vectorDistanceMetric ?? "-" }}
                      </template>
                      <template v-else>-</template>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <!-- Query panel -->
            <div class="rounded-xl border border-border bg-bg-secondary p-4 shrink-0">
              <div class="flex items-center gap-2 mb-3">
                <div class="flex items-center h-7 rounded-lg border border-border overflow-hidden">
                  <button
                    @click="queryMode = 'text'"
                    class="px-2.5 h-full text-xs flex items-center gap-1 transition-colors"
                    :class="queryMode === 'text' ? 'bg-redis/10 text-redis' : 'text-text-secondary hover:bg-bg-hover'"
                  >
                    {{ t("search.textQuery") }}
                  </button>
                  <button
                    @click="queryMode = 'knn'"
                    :disabled="!vectorSupported || vectorFields.length === 0"
                    class="px-2.5 h-full text-xs flex items-center gap-1 transition-colors border-l border-border disabled:opacity-40"
                    :class="queryMode === 'knn' ? 'bg-redis/10 text-redis' : 'text-text-secondary hover:bg-bg-hover'"
                  >
                    {{ t("search.knnQuery") }}
                  </button>
                </div>
                <span v-if="!vectorSupported" class="text-[11px] text-warning flex items-center gap-1">
                  <TriangleAlert :size="12" />
                  {{ t("search.vectorUnsupported") }}
                </span>
                <span v-else-if="vectorFields.length === 0" class="text-[11px] text-text-muted">
                  {{ t("search.noVectorField") }}
                </span>
              </div>

              <!-- Text query -->
              <div v-if="queryMode === 'text'" class="flex items-center gap-2">
                <input
                  v-model="textQuery"
                  type="text"
                  placeholder="*  /  @field:(value)  /  hello world"
                  @keyup.enter="runSearch"
                  class="flex-1 h-8 px-2.5 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
                />
                <button
                  @click="runSearch"
                  :disabled="searchStore.searching"
                  class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1.5 disabled:opacity-50"
                >
                  <Play :size="13" />
                  {{ t("search.run") }}
                </button>
              </div>

              <!-- KNN query -->
              <div v-else class="space-y-2">
                <div class="flex items-center gap-2">
                  <select
                    v-model="knnField"
                    class="h-8 px-2 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none"
                  >
                    <option v-for="f in vectorFields" :key="f.identifier" :value="f.identifier">
                      @{{ f.identifier }} (dim {{ formatDim(f) }})
                    </option>
                  </select>
                  <label class="text-xs text-text-secondary">K =</label>
                  <input
                    v-model.number="knnK"
                    type="number"
                    min="1"
                    max="1000"
                    class="w-20 h-8 px-2 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none focus:border-redis/50"
                  />
                </div>
                <textarea
                  v-model="knnVector"
                  rows="2"
                  :placeholder="t('search.knnVectorPlaceholder')"
                  class="w-full px-2.5 py-2 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50 resize-none"
                />
                <button
                  @click="runSearch"
                  :disabled="searchStore.searching"
                  class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors flex items-center gap-1.5 disabled:opacity-50"
                >
                  <Play :size="13" />
                  {{ t("search.run") }}
                </button>
              </div>
            </div>

            <!-- Results -->
            <div class="rounded-xl border border-border bg-bg-secondary overflow-hidden shrink-0">
              <div class="px-4 py-2.5 border-b border-border flex items-center gap-2">
                <SearchIcon :size="14" class="text-text-muted" />
                <span class="text-xs font-medium text-text-primary">{{ t("search.results") }}</span>
                <span v-if="searchStore.searchResult" class="text-[11px] text-text-muted">
                  {{ t("search.totalFound", { count: searchStore.searchResult.total }) }}
                </span>
              </div>
              <div class="max-h-80 overflow-auto">
                <p v-if="!searchStore.searchResult" class="text-xs text-text-muted text-center py-6">
                  {{ t("search.noResults") }}
                </p>
                <p v-else-if="searchStore.searchResult.docs.length === 0" class="text-xs text-text-muted text-center py-6">
                  {{ searchStore.searching ? t("common.loading") : t("search.emptyResults") }}
                </p>
                <table v-else class="w-full text-xs">
                  <thead class="sticky top-0 bg-bg-secondary">
                    <tr class="text-left text-text-muted border-b border-border">
                      <th class="px-3 py-2 font-medium w-12">#</th>
                      <th class="px-2 py-2 font-medium">{{ t("search.colDocId") }}</th>
                      <th class="px-2 py-2 font-medium w-20">{{ t("search.colScore") }}</th>
                      <th class="px-2 py-2 font-medium">{{ t("streams.colFields") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="(doc, i) in searchStore.searchResult.docs" :key="doc.id" class="border-b border-border/50 align-top hover:bg-bg-hover/50">
                      <td class="px-3 py-1.5 text-text-muted">{{ i + 1 }}</td>
                      <td class="px-2 py-1.5 font-mono text-redis/90 break-all">{{ doc.id }}</td>
                      <td class="px-2 py-1.5 text-text-secondary">{{ doc.score != null ? doc.score.toFixed(4) : "-" }}</td>
                      <td class="px-2 py-1.5 font-mono text-text-secondary">
                        <div v-for="[f, v] in doc.fields" :key="f" class="truncate max-w-[420px]" :title="`${f}=${v}`">
                          <span class="text-redis/70">{{ f }}</span>=<span class="break-all">{{ v }}</span>
                        </div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          </template>

          <!-- No index selected -->
          <div v-else class="flex-1 flex items-center justify-center">
            <p class="text-sm text-text-muted">
              {{ searchStore.indexes.length === 0 ? t("search.noIndexes") : t("search.selectIndexHint") }}
            </p>
          </div>
        </div>
      </div>
    </template>

    <!-- Create index wizard -->
    <Teleport to="body">
      <div v-if="showCreateModal" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <div class="absolute inset-0 bg-black/40" @click="showCreateModal = false" />
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[560px] max-w-[92vw] p-5 max-h-[85vh] overflow-y-auto">
          <div class="flex items-center justify-between mb-4">
            <h3 class="text-sm font-semibold text-text-primary flex items-center gap-2">
              <Plus :size="15" class="text-redis" />
              {{ t("search.createTitle") }}
            </h3>
            <button @click="showCreateModal = false" class="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors">
              <X :size="16" />
            </button>
          </div>

          <label class="block text-xs text-text-secondary mb-1">{{ t("search.indexName") }}</label>
          <input
            v-model="createName"
            type="text"
            placeholder="idx:products"
            class="w-full h-8 px-2.5 mb-3 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
          />

          <div class="flex items-center gap-3 mb-3">
            <label class="text-xs text-text-secondary">{{ t("search.onType") }}</label>
            <select
              v-model="createOnType"
              class="h-8 px-2 text-xs rounded-lg border border-border bg-bg-primary text-text-primary focus:outline-none"
            >
              <option value="HASH">HASH</option>
              <option value="JSON">JSON</option>
            </select>
            <label class="text-xs text-text-secondary ml-2">{{ t("search.prefixes") }}</label>
            <input
              v-model="createPrefixes"
              type="text"
              placeholder="product: item:"
              class="flex-1 h-8 px-2.5 text-xs font-mono rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
            />
          </div>

          <p class="text-xs text-text-secondary mb-2">{{ t("search.schemaFields") }}</p>
          <div class="space-y-2 mb-3">
            <div v-for="(f, idx) in createFields" :key="idx" class="flex items-center gap-2 flex-wrap">
              <input
                v-model="f.identifier"
                type="text"
                :placeholder="t('search.colField')"
                class="flex-1 min-w-24 h-7 px-2 text-xs font-mono rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
              />
              <select
                v-model="f.fieldType"
                class="h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary focus:outline-none"
              >
                <option value="TEXT">TEXT</option>
                <option value="TAG">TAG</option>
                <option value="NUMERIC">NUMERIC</option>
                <option value="GEO">GEO</option>
                <option value="VECTOR" :disabled="!vectorSupported">VECTOR</option>
              </select>
              <!-- Vector options -->
              <template v-if="f.fieldType === 'VECTOR'">
                <select
                  v-model="f.vectorAlgorithm"
                  class="h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary focus:outline-none"
                >
                  <option value="FLAT">FLAT</option>
                  <option value="HNSW">HNSW</option>
                </select>
                <input
                  v-model.number="f.vectorDim"
                  type="number"
                  min="1"
                  placeholder="dim"
                  class="w-20 h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary placeholder:text-text-muted focus:outline-none focus:border-redis/50"
                />
                <select
                  v-model="f.vectorDistanceMetric"
                  class="h-7 px-2 text-xs rounded-md border border-border bg-bg-primary text-text-primary focus:outline-none"
                >
                  <option value="COSINE">COSINE</option>
                  <option value="L2">L2</option>
                  <option value="IP">IP</option>
                </select>
              </template>
              <button
                @click="removeCreateField(idx)"
                :disabled="createFields.length === 1"
                class="p-1 rounded text-text-muted hover:text-red-500 transition-colors disabled:opacity-30"
              >
                <Trash2 :size="13" />
              </button>
            </div>
          </div>
          <button @click="addCreateField" class="text-xs text-redis hover:underline mb-4 flex items-center gap-1">
            <Plus :size="12" />
            {{ t("search.addField") }}
          </button>

          <div class="flex justify-end gap-2">
            <button
              @click="showCreateModal = false"
              class="h-8 px-4 text-xs rounded-lg border border-border text-text-secondary hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
            <button
              @click="submitCreate"
              :disabled="creating || !createName.trim() || createFields.every((f) => !f.identifier.trim())"
              class="h-8 px-4 text-xs rounded-lg bg-redis text-white hover:bg-redis-dark transition-colors disabled:opacity-50"
            >
              {{ t("search.createIndex") }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <ConfirmDialog ref="confirmDialog" />
  </div>
</template>
