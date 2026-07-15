<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { watch, onMounted, onBeforeUnmount, ref, reactive, computed, nextTick } from "vue";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { useMetricsStore } from "@/stores/metricsStore";
import type { RedisDataType, StringValue } from "@/types";
import KeyTreeItem from "@/components/cascade/KeyTreeItem.vue";
import TtlGauge from "@/components/charts/TtlGauge.vue";
import FloatingWindow from "@/components/shared/FloatingWindow.vue";
import ConfirmDialog from "@/components/shared/ConfirmDialog.vue";
import NumberedTextarea from "@/components/shared/NumberedTextarea.vue";
import { useCopyTip } from "@/utils/copyTip";
import { useSaveTip } from "@/utils/saveTip";
import { toast } from "@/utils/toast";
import {
  Search, RefreshCw, Trash2, Copy, Tag, Plus, Key,
  Type, Hash, List, CircleDot, BarChart3,
  AlertTriangle, X, Pencil, Save,
  ChevronLeft, ChevronRight, Clock, Database, Code2,
} from "lucide-vue-next";

const { t } = useI18n();
const cascade = useCascadeStore();
const detail = useDetailStore();
const connStore = useConnectionStore();
const metricsStore = useMetricsStore();

const confirmDialog = ref<InstanceType<typeof ConfirmDialog>>();

const isConnected = computed(() => connStore.activeConnection?.status === "connected");

// Debounce search input: wait 300ms after user stops typing before triggering filter/scan
let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  () => cascade.searchQuery,
  (val) => {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      cascade.debouncedSearchQuery = val;
    }, 300);
  }
);

// When debounced search changes, re-scan from Redis with new pattern
watch(
  () => cascade.debouncedSearchQuery,
  () => {
    // Reset scroll position when search changes
    scrollTop.value = 0;
    nextTick(() => {
      const el = treeScrollRef.value;
      if (el) el.scrollTop = 0;
    });
    cascade.refreshKeys(true);
  }
);

// Debounce filter input for composite types (hash/list/set/zset)
let filterDebounceTimer: ReturnType<typeof setTimeout> | null = null;
function debounceFilter(value: string) {
  if (filterDebounceTimer) clearTimeout(filterDebounceTimer);
  filterDebounceTimer = setTimeout(() => {
    detail.searchFilter(value);
  }, 400);
}

// --- Virtual scroll ---
const ITEM_HEIGHT = 32;
const BUFFER = 10;
const treeScrollRef = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const containerHeight = ref(500);

const visibleItems = computed(() => {
  const nodes = cascade.visibleNodes;
  const start = Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER);
  const visibleCount = Math.ceil(containerHeight.value / ITEM_HEIGHT) + 2 * BUFFER;
  const end = Math.min(nodes.length, start + visibleCount);
  return nodes.slice(start, end).map((item, i) => ({
    ...item,
    index: start + i,
  }));
});

const topPadding = computed(() => {
  const start = Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER);
  return start * ITEM_HEIGHT;
});
const bottomPadding = computed(() => {
  const end = Math.min(
    cascade.visibleNodes.length,
    Math.max(0, Math.floor(scrollTop.value / ITEM_HEIGHT) - BUFFER) + Math.ceil(containerHeight.value / ITEM_HEIGHT) + 2 * BUFFER
  );
  return Math.max(0, (cascade.visibleNodes.length - end) * ITEM_HEIGHT);
});

function onTreeScroll(e: Event) {
  const el = e.target as HTMLElement;
  scrollTop.value = el.scrollTop;
  containerHeight.value = el.clientHeight;
}

// Auto-load more keys when visible nodes don't fill the viewport (e.g. tree collapsed)
watch(
  [() => cascade.visibleNodes.length, () => cascade.keys.length],
  ([visibleLen]) => {
    if (!cascade.hasMore || cascade.loading || !isConnected.value) return;
    const viewportCapacity = Math.ceil(containerHeight.value / ITEM_HEIGHT);
    if (visibleLen < viewportCapacity) {
      cascade.loadMoreKeys();
    }
  }
);

// Immediately flush debounce when resetting search (e.g. DB switch, connection change)
function resetSearchImmediate() {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = null;
  }
  cascade.searchQuery = "";
  cascade.debouncedSearchQuery = "";
}


const typeColors: Record<RedisDataType, string> = {
  string: "bg-type-string/10 text-type-string",
  hash: "bg-type-hash/10 text-type-hash",
  list: "bg-type-list/10 text-type-list",
  set: "bg-type-set/10 text-type-set",
  zset: "bg-type-zset/10 text-type-zset",
};

const typeIcons: Record<RedisDataType, any> = {
  string: Type, hash: Hash, list: List, set: CircleDot, zset: BarChart3,
};

const typeBorderColors: Record<RedisDataType, string> = {
  string: '#8b5cf6', hash: '#0ea5e9', list: '#10b981', set: '#f59e0b', zset: '#ef4444',
};

function formatTtl(ttl: number): string {
  if (ttl === -1) return t("detail.noExpiry");
  if (ttl === -2) return "N/A";
  if (ttl < 60) return `${ttl}s`;
  return `${ttl}s`;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / 1048576).toFixed(1)}M`;
}

function handleSelect(node: any) {
  if (node.key) {
    cascade.selectKey(node.key.key);
    // Close non-pinned floating windows on key switch
    for (let i = floatingWindows.length - 1; i >= 0; i--) {
      if (!floatingWindows[i].pinned) floatingWindows.splice(i, 1);
    }
  } else {
    cascade.toggleNode(node);
  }
}

const { copyWithTip } = useCopyTip();
const { handleSave } = useSaveTip();

async function copyKey(key: string, e: Event) {
  await copyWithTip(key, e);
}

async function deleteKey() {
  const key = detail.currentKey?.key;
  if (!key) return;
  const confirmed = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("browser.confirmDelete", { key }),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!confirmed) return;
  try {
    await cascade.deleteKey(key);
    detail.clearDetail();
  } catch (e) {
    console.error("Delete key failed:", e);
    toast.error(typeof e === 'string' ? e : `Failed to delete key: ${key}`);
  }
}

// ====== EDITING STATE ======
// Key rename
const editingKey = ref(false);
const editKeyTemp = ref('');
async function startEditKey() {
  editKeyTemp.value = detail.currentKey?.key ?? '';
  editingKey.value = true;
}
async function saveEditKey(e: Event) {
  const newKey = editKeyTemp.value.trim();
  if (!newKey || newKey === detail.currentKey?.key) { editingKey.value = false; return; }
  const ok = await handleSave(() => detail.renameKey(newKey), e);
  if (ok) { cascade.refreshKeys(true); editingKey.value = false; }
}
function cancelEditKey() { editingKey.value = false; }

// String value edit
const editingString = ref(false);
const stringTemp = ref('');
function startEditString() {
  stringTemp.value = displayStringValue.value;
  editingString.value = true;
}
async function saveEditString(e: Event) {
  // JSON mode: validate and minify before saving
  if (viewMode.value === 'json') {
    try {
      const parsed = JSON.parse(stringTemp.value);
      stringTemp.value = JSON.stringify(parsed);
    } catch {
      toast.error(t('detail.invalidJson'));
      return;
    }
  }
  const ok = await handleSave(() => detail.saveStringValue(stringTemp.value), e);
  if (ok) editingString.value = false;
}
function cancelEditString() { editingString.value = false; }

// Hash field value edit: track which row is being edited
const editingHashField = ref<string | null>(null);
const hashFieldTemp = ref('');
function startEditHash(field: string, value: string) {
  editingHashField.value = field;
  hashFieldTemp.value = value;
}
async function saveEditHash(e: Event) {
  if (editingHashField.value === null) return;
  const ok = await handleSave(() => detail.saveHashField(editingHashField.value!, hashFieldTemp.value), e);
  if (ok) editingHashField.value = null;
}
function cancelEditHash() { editingHashField.value = null; }

// Hash field NAME rename
const renamingHashField = ref<string | null>(null);
const hashFieldRenameTemp = ref('');
function startRenameHashField(field: string) {
  renamingHashField.value = field;
  hashFieldRenameTemp.value = field;
}
async function saveRenameHashField(e: Event) {
  if (renamingHashField.value === null) return;
  const oldName = renamingHashField.value;
  const newName = hashFieldRenameTemp.value.trim();
  if (!newName || newName === oldName) { renamingHashField.value = null; return; }
  const ok = await handleSave(() => detail.renameHashField(oldName, newName), e);
  if (ok) {
    renamingHashField.value = null;
    // Sync floating window cellId after hash field rename
    for (const win of floatingWindows) {
      if (win.cellType === 'hash' && win.cellId === oldName) {
        win.cellId = newName;
        win.title = newName;
      }
    }
  }
}
function cancelRenameHashField() { renamingHashField.value = null; }

// Hash field TTL edit (Redis >= 7.4.0)
const editingFieldTtl = ref<string | null>(null);
const fieldTtlTemp = ref('');
function startEditFieldTtl(field: string, currentTtl?: number) {
  editingFieldTtl.value = field;
  fieldTtlTemp.value = currentTtl && currentTtl > 0 ? String(currentTtl) : '-1';
}
async function saveEditFieldTtl(e: Event) {
  if (editingFieldTtl.value === null) return;
  const val = parseInt(fieldTtlTemp.value, 10);
  if (isNaN(val)) { editingFieldTtl.value = null; return; }
  const ok = await handleSave(() => detail.setHashFieldTtl(editingFieldTtl.value!, val), e);
  if (ok) editingFieldTtl.value = null;
}
function cancelEditFieldTtl() { editingFieldTtl.value = null; }

function formatFieldTtl(ttl?: number): string {
  if (ttl === undefined || ttl === -1) return t("detail.noExpiry");
  if (ttl === -2) return "N/A";
  if (ttl < 60) return `${ttl}s`;
  if (ttl < 3600) return `${Math.floor(ttl / 60)}m ${ttl % 60}s`;
  if (ttl < 86400) return `${Math.floor(ttl / 3600)}h ${Math.floor((ttl % 3600) / 60)}m`;
  return `${Math.floor(ttl / 86400)}d ${Math.floor((ttl % 86400) / 3600)}h`;
}

// List item edit
const editingListIndex = ref<number | null>(null);
const listItemTemp = ref('');
function startEditList(index: number, value: string) {
  editingListIndex.value = index;
  listItemTemp.value = value;
}
async function saveEditList(e: Event) {
  if (editingListIndex.value === null) return;
  const listValue = detail.currentValue as any;
  const originalIndices = listValue?.originalIndices as number[] | undefined;
  const globalIndex = originalIndices
    ? originalIndices[editingListIndex.value]
    : detail.currentPage * detail.pageSize + editingListIndex.value;
  const ok = await handleSave(() => detail.saveListItem(globalIndex, listItemTemp.value), e);
  if (ok) editingListIndex.value = null;
}
function cancelEditList() { editingListIndex.value = null; }

// Set member edit
const editingSetMember = ref<string | null>(null);
const setMemberTemp = ref('');
function startEditSet(member: string) {
  editingSetMember.value = member;
  setMemberTemp.value = member;
}
async function saveEditSet(e: Event) {
  if (editingSetMember.value === null) return;
  const oldMember = editingSetMember.value;
  const newMember = setMemberTemp.value;
  const ok = await handleSave(() => detail.saveSetMember(oldMember, newMember), e);
  if (ok) {
    editingSetMember.value = null;
    // Sync floating window cellId/content after set member rename
    if (oldMember !== newMember) {
      for (const win of floatingWindows) {
        if (win.cellType === 'set' && win.cellId === oldMember) {
          win.cellId = newMember;
          win.content = newMember;
        }
      }
    }
  }
}
function cancelEditSet() { editingSetMember.value = null; }

// ZSet member edit
const editingZSetMember = ref<string | null>(null);
const zsetMemberTemp = ref('');
const zsetScoreTemp = ref(0);
function startEditZSet(member: string, score: number) {
  editingZSetMember.value = member;
  zsetMemberTemp.value = member;
  zsetScoreTemp.value = score;
}
async function saveEditZSet(e: Event) {
  if (editingZSetMember.value === null) return;
  const oldMember = editingZSetMember.value;
  const newMember = zsetMemberTemp.value;
  const ok = await handleSave(() => detail.saveZSetMember(oldMember, newMember, zsetScoreTemp.value), e);
  if (ok) {
    editingZSetMember.value = null;
    // Sync floating window cellId/content after zset member rename
    if (oldMember !== newMember) {
      for (const win of floatingWindows) {
        if (win.cellType === 'zset' && win.cellId === oldMember) {
          win.cellId = newMember;
          win.content = newMember;
        }
      }
    }
  }
}
function cancelEditZSet() { editingZSetMember.value = null; }

// Delete a sub-element (hash field / list item / set member / zset member)
async function deleteSubField(keyType: string, params: { field?: string; value?: string }) {
  const confirmed = await confirmDialog.value?.open({
    title: t("common.confirmDeleteTitle"),
    message: t("detail.confirmDeleteField"),
    confirmLabel: t("common.delete"),
    cancelLabel: t("common.cancel"),
    danger: true,
  });
  if (!confirmed) return;
  try {
    const ok = await detail.deleteField({ keyType, ...params });
    if (ok) toast.success(t("detail.saveSuccess"));
    else toast.error(t("detail.saveFailed"));
  } catch (e) {
    console.error("Delete field failed:", e);
    toast.error(typeof e === 'string' ? e : "Failed to delete");
  }
}

// TTL edit
const editingTtl = ref(false);
const ttlTemp = ref('');
function startEditTtl() {
  ttlTemp.value = detail.ttlRemaining > 0 ? String(detail.ttlRemaining) : '-1';
  editingTtl.value = true;
}
async function saveEditTtl(e: Event) {
  const val = parseInt(ttlTemp.value, 10);
  if (isNaN(val)) { editingTtl.value = false; return; }
  const ok = await handleSave(() => detail.setTtl(val), e);
  if (ok) editingTtl.value = false;
}
function cancelEditTtl() { editingTtl.value = false; }

// ====== NEW KEY DIALOG ======
const showNewKeyDialog = ref(false);
const newKeyType = ref<RedisDataType>('string');
const newKeyName = ref('');
const newKeyTtl = ref('');
const newKeyBatchData = ref('');
const newKeyLoading = ref(false);

function openNewKeyDialog() {
  newKeyType.value = 'string';
  newKeyName.value = '';
  newKeyTtl.value = '';
  newKeyBatchData.value = '';
  showNewKeyDialog.value = true;
}
function closeNewKeyDialog() { showNewKeyDialog.value = false; }

/** Strip surrounding quotes from a string */
function stripQuotes(s: string): string {
  const t = s.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    return t.slice(1, -1);
  }
  return t;
}

/** Parse batch text data into the format expected by the backend */
function parseBatchData(type: RedisDataType, text: string): any {
  const lines = text.split('\n').filter(l => l.trim());
  switch (type) {
    case 'string':
      return text; // raw string
    case 'hash':
      return lines.map(line => {
        const idx = line.indexOf(':');
        if (idx <= 0) return [stripQuotes(line), ''];
        return [stripQuotes(line.slice(0, idx)), stripQuotes(line.slice(idx + 1))];
      }).filter(([f]) => f);
    case 'list':
      return lines.map(l => l.trim()).filter(Boolean);
    case 'set':
      return lines.map(l => l.trim()).filter(Boolean);
    case 'zset':
      return lines.map((line, i) => {
        const idx = line.indexOf(':');
        if (idx <= 0) {
          const raw = stripQuotes(line);
          return [raw, 0];
        }
        const scoreStr = stripQuotes(line.slice(0, idx));
        const score = Number(scoreStr);
        if (!Number.isFinite(score)) throw new Error(t('browser.zsetScoreInvalid', { line: i + 1, value: scoreStr }));
        const member = stripQuotes(line.slice(idx + 1));
        return [member, score];
      }).filter(([m]) => m);
    default:
      return undefined;
  }
}

const newKeyBatchHint = computed(() => {
  switch (newKeyType.value) {
    case 'string': return t('browser.batchHintString');
    case 'hash': return t('browser.batchHintHash');
    case 'list': return t('browser.batchHintList');
    case 'set': return t('browser.batchHintSet');
    case 'zset': return t('browser.batchHintZset');
    default: return '';
  }
});

const newKeyBatchPlaceholder = computed(() => {
  switch (newKeyType.value) {
    case 'hash': return '"name":"John"\n"age":"30"\n"city":"Beijing"';
    case 'zset': return '"100":"player1"\n"200":"player2"\n"300":"player3"';
    case 'list': return 'item1\nitem2\nitem3';
    case 'set': return 'member1\nmember2\nmember3';
    default: return '';
  }
});

async function submitNewKey() {
  const name = newKeyName.value.trim();
  if (!name) { toast.error(t('browser.keyNameRequired')); return; }
  newKeyLoading.value = true;
  try {
    const ttl = newKeyTtl.value.trim() ? parseInt(newKeyTtl.value.trim(), 10) : undefined;
    const batchText = newKeyBatchData.value.trim();
    const initialData = batchText ? parseBatchData(newKeyType.value, batchText) : undefined;
    const ok = await detail.createKey({
      keyName: name,
      keyType: newKeyType.value,
      ttl: ttl && ttl > 0 ? ttl : undefined,
      initialData,
    });
    if (ok) {
      toast.success(t('detail.saveSuccess'));
      showNewKeyDialog.value = false;
    } else {
      toast.error(t('detail.saveFailed'));
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : typeof e === 'string' ? e : 'Failed to create key');
  } finally {
    newKeyLoading.value = false;
  }
}

// ====== ADD FIELD DIALOG (hash/list/set/zset) ======
const showAddFieldDialog = ref(false);
const addFieldBatchData = ref('');
const addFieldTtl = ref('');
const addFieldLoading = ref(false);

function openAddFieldDialog() {
  addFieldBatchData.value = '';
  addFieldTtl.value = '';
  showAddFieldDialog.value = true;
}
function closeAddFieldDialog() { showAddFieldDialog.value = false; }

const addFieldBatchHint = computed(() => {
  const type = detail.currentValue?.type;
  switch (type) {
    case 'hash': return t('detail.batchAddPlaceholderHash');
    case 'list': return t('detail.batchAddPlaceholderList');
    case 'set': return t('detail.batchAddPlaceholderSet');
    case 'zset': return t('detail.batchAddPlaceholderZset');
    default: return '';
  }
});

const addFieldBatchPlaceholder = computed(() => {
  const type = detail.currentValue?.type;
  switch (type) {
    case 'hash': return '"name":"John"\n"age":"30"';
    case 'zset': return '"100":"player1"\n"200":"player2"';
    case 'list': return 'item1\nitem2';
    case 'set': return 'member1\nmember2';
    default: return '';
  }
});

const addFieldSupportsTtl = computed(() => {
  return detail.currentValue?.type === 'hash' && metricsStore.supportsFieldTtl;
});

/** Parse batch add data based on current key type */
function parseAddBatchData(type: string, text: string): any {
  const lines = text.split('\n').filter(l => l.trim());
  switch (type) {
    case 'hash':
      return lines.map(line => {
        const idx = line.indexOf(':');
        if (idx <= 0) return [stripQuotes(line), ''];
        return [stripQuotes(line.slice(0, idx)), stripQuotes(line.slice(idx + 1))];
      }).filter(([f]) => f);
    case 'list':
      return lines.map(l => l.trim()).filter(Boolean);
    case 'set':
      return lines.map(l => l.trim()).filter(Boolean);
    case 'zset':
      return lines.map((line, i) => {
        const idx = line.indexOf(':');
        if (idx <= 0) {
          const raw = stripQuotes(line);
          return [raw, 0];
        }
        const scoreStr = stripQuotes(line.slice(0, idx));
        const score = Number(scoreStr);
        if (!Number.isFinite(score)) throw new Error(t('browser.zsetScoreInvalid', { line: i + 1, value: scoreStr }));
        const member = stripQuotes(line.slice(idx + 1));
        return [member, score];
      }).filter(([m]) => m);
    default:
      return [];
  }
}

async function submitAddFieldDialog() {
  const type = detail.currentValue?.type;
  if (!type || !['hash', 'list', 'set', 'zset'].includes(type)) return;
  const batchText = addFieldBatchData.value.trim();
  if (!batchText) { toast.error(t('browser.keyNameRequired')); return; }
  addFieldLoading.value = true;
  try {
    const items = parseAddBatchData(type, batchText);
    if (!items.length) { toast.error(t('browser.keyNameRequired')); addFieldLoading.value = false; return; }
    const fieldTtl = addFieldTtl.value.trim() ? parseInt(addFieldTtl.value.trim(), 10) : undefined;
    const ok = await detail.batchAddFields({
      keyType: type,
      items,
      fieldTtl: fieldTtl && fieldTtl > 0 ? fieldTtl : undefined,
    });
    if (ok) {
      toast.success(t('detail.batchAddSuccess'));
      showAddFieldDialog.value = false;
    } else {
      toast.error(t('detail.batchAddFailed'));
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : typeof e === 'string' ? e : 'Failed to add');
  } finally {
    addFieldLoading.value = false;
  }
}

// Reset all editing state when switching keys
function resetAllEditingState() {
  editingKey.value = false;
  editKeyTemp.value = '';
  editingString.value = false;
  stringTemp.value = '';
  editingHashField.value = null;
  hashFieldTemp.value = '';
  renamingHashField.value = null;
  hashFieldRenameTemp.value = '';
  editingFieldTtl.value = null;
  fieldTtlTemp.value = '';
  editingListIndex.value = null;
  listItemTemp.value = '';
  editingSetMember.value = null;
  setMemberTemp.value = '';
  editingZSetMember.value = null;
  zsetMemberTemp.value = '';
  zsetScoreTemp.value = 0;
  editingTtl.value = false;
  ttlTemp.value = '';
  showAddFieldDialog.value = false;
}

// Watch selected key change to reset editing state
watch(
  () => cascade.selectedKey,
  () => {
    resetAllEditingState();
    // Auto-detect JSON content and switch to JSON view
    viewMode.value = isJsonContent.value ? 'json' : 'text';
  }
);

// ====== VALUE VIEW MODE (Text / Hex / JSON / ASCII) ======
type ValueViewMode = 'text' | 'hex' | 'json' | 'ascii';
const viewMode = ref<ValueViewMode>('text');

/** Whether the current string value is valid JSON */
const isJsonContent = computed(() => {
  if (detail.currentValue?.type !== 'string') return false;
  const val = (detail.currentValue as StringValue).value;
  if (!val.trim()) return false;
  try { JSON.parse(val); return true; } catch { return false; }
});

/** Format value as pretty-printed JSON */
const formattedJson = computed(() => {
  if (detail.currentValue?.type !== 'string') return '';
  const val = (detail.currentValue as StringValue).value;
  try { return JSON.stringify(JSON.parse(val), null, 2); } catch { return val; }
});

/** Convert raw bytes to ASCII representation (non-printable → '.') */
const asciiDisplay = computed(() => {
  if (detail.currentValue?.type !== 'string') return '';
  const hex = (detail.currentValue as StringValue).valueHex || '';
  let result = '';
  for (let i = 0; i < hex.length; i += 2) {
    const byte = parseInt(hex.substring(i, i + 2), 16);
    result += (byte >= 0x20 && byte <= 0x7E) ? String.fromCharCode(byte) : '.';
  }
  return result;
});

/** Convert hex string to ASCII display (non-printable → '.') */
function hexToAscii(hex: string): string {
  let result = '';
  for (let i = 0; i < hex.length; i += 2) {
    const byte = parseInt(hex.substring(i, i + 2), 16);
    result += (byte >= 0x20 && byte <= 0x7E) ? String.fromCharCode(byte) : '.';
  }
  return result;
}

/** Convert text to ASCII using lossy byte conversion */
function textToAscii(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    result += (code >= 0x20 && code <= 0x7E) ? text[i] : '.';
  }
  return result;
}

/** Composite type JSON serialization (entire structure) */
const compositeJsonStr = computed(() => {
  const val = detail.currentValue;
  if (!val || val.type === 'string') return '';
  try {
    switch (val.type) {
      case 'hash': {
        const obj: Record<string, string> = {};
        for (const f of (val as any).fields) obj[f.field] = f.value;
        return JSON.stringify(obj, null, 2);
      }
      case 'list': return JSON.stringify((val as any).items, null, 2);
      case 'set': return JSON.stringify((val as any).members, null, 2);
      case 'zset': {
        const arr = (val as any).members.map((m: any) => ({ member: m.member, score: m.score }));
        return JSON.stringify(arr, null, 2);
      }
      default: return '';
    }
  } catch { return ''; }
});

/** Whether current view mode allows editing (string only for text/json) */
const canEditString = computed(() => viewMode.value === 'text' || viewMode.value === 'json');

/** Display value based on current view mode */
const displayStringValue = computed(() => {
  if (detail.currentValue?.type !== 'string') return '';
  const sv = detail.currentValue as StringValue;
  switch (viewMode.value) {
    case 'hex': return sv.valueHex || '';
    case 'json': return formattedJson.value;
    case 'ascii': return asciiDisplay.value;
    default: return sv.value;
  }
});

// Cancel editing when switching to a read-only view mode
watch(viewMode, () => {
  if (editingString.value && !canEditString.value) {
    editingString.value = false;
  }
  // Close all floating cell popups when switching view mode
  floatingWindows.length = 0;
});

// Floating windows for cell value display
interface FloatingWin {
  id: string;
  title: string;
  content: string;
  redisKey: string;
  x: number;
  y: number;
  width: number;
  height: number;
  pinned: boolean;
  zIndex: number;
  /** Cell type for edit routing: 'hash' | 'list' | 'set' | 'zset' */
  cellType?: string;
  /** Identifier for the cell: field name (hash), index (list), member (set/zset) */
  cellId?: string;
}

const floatingWindows = reactive<FloatingWin[]>([]);
let winIdCounter = 0;
let topZIndex = 9999;

function getNextZIndex() {
  return ++topZIndex;
}

function showCellPopup(e: MouseEvent, content: string, title: string, cellType?: string, cellId?: string) {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const x = Math.min(e.clientX + 8, vw - 420);
  const y = Math.min(e.clientY + 8, vh - 260);
  const redisKey = detail.currentKey?.key ?? '';

  // Find existing non-pinned window and update it
  const existing = floatingWindows.find((w) => !w.pinned);
  if (existing) {
    existing.title = title;
    existing.content = content;
    existing.redisKey = redisKey;
    existing.x = x;
    existing.y = y;
    existing.zIndex = getNextZIndex();
    existing.cellType = cellType;
    existing.cellId = cellId;
    return;
  }

  // Create new window
  floatingWindows.push({
    id: `fw-${++winIdCounter}`,
    title,
    content,
    redisKey,
    x,
    y,
    width: 380,
    height: 240,
    pinned: false,
    zIndex: getNextZIndex(),
    cellType,
    cellId,
  });
}

function closeFloatingWin(id: string) {
  const idx = floatingWindows.findIndex((w) => w.id === id);
  if (idx !== -1) floatingWindows.splice(idx, 1);
}

function togglePin(id: string) {
  const win = floatingWindows.find((w) => w.id === id);
  if (win) {
    win.pinned = !win.pinned;
    if (win.pinned) {
      win.zIndex = getNextZIndex();
    }
  }
}

function updateWinPosition(id: string, x: number, y: number) {
  const win = floatingWindows.find((w) => w.id === id);
  if (win) {
    win.x = x;
    win.y = y;
  }
}

function updateWinSize(id: string, w: number, h: number) {
  const win = floatingWindows.find((win) => win.id === id);
  if (win) {
    win.width = w;
    win.height = h;
  }
}

function focusWin(id: string) {
  const win = floatingWindows.find((w) => w.id === id);
  if (win) {
    win.zIndex = getNextZIndex();
  }
}

/** Sync floating window content when detail data changes (e.g. after inline save) */
watch(
  () => detail.currentValue,
  (val) => {
    if (!val || floatingWindows.length === 0) return;
    for (const win of floatingWindows) {
      if (!win.cellType || !win.cellId) continue;
      switch (win.cellType) {
        case 'hash': {
          const field = (val as any).fields?.find((f: any) => f.field === win.cellId);
          if (field) win.content = field.value;
          break;
        }
        case 'list': {
          const globalIdx = parseInt(win.cellId, 10);
          const origIdx = (val as any).originalIndices as number[] | undefined;
          let localIdx: number;
          if (origIdx) {
            localIdx = origIdx.indexOf(globalIdx);
          } else {
            localIdx = globalIdx - detail.currentPage * detail.pageSize;
          }
          if (localIdx >= 0 && localIdx < ((val as any).items?.length ?? 0)) {
            win.content = (val as any).items[localIdx];
          }
          break;
        }
        case 'set': {
          if ((val as any).members?.includes(win.cellId)) {
            win.content = win.cellId;
          }
          break;
        }
        case 'zset': {
          const member = (val as any).members?.find((m: any) => m.member === win.cellId);
          if (member) win.content = member.member;
          break;
        }
      }
    }
  },
  { deep: true }
);

/** Handle content save from floating window edit mode */
async function handleSaveContent(id: string, newContent: string) {
  const win = floatingWindows.find((w) => w.id === id);
  if (!win || !win.cellType) return false;

  let ok = false;
  try {
    switch (win.cellType) {
      case 'hash':
        ok = await detail.saveHashField(win.cellId!, newContent);
        break;
      case 'list':
        ok = await detail.saveListItem(parseInt(win.cellId!), newContent);
        break;
      case 'set':
        ok = await detail.saveSetMember(win.cellId!, newContent);
        break;
      case 'zset': {
        const zval = detail.currentValue as any;
        const member = zval?.members?.find((m: any) => m.member === win.cellId);
        ok = await detail.saveZSetMember(win.cellId!, newContent, member?.score ?? 0);
        break;
      }
    }
  } catch (e) {
    console.error("Failed to save from floating window:", e);
  }

  if (ok) {
    win.content = newContent;
    toast.success(t("detail.saveSuccess"));
  } else {
    toast.error(t("detail.saveFailed"));
  }
  return ok;
}

// Auto-load keys when connection changes or page mounts
watch(
  () => connStore.activeConnectionId,
  (id) => {
    if (id) {
      cascade.selectedKey = null;
      resetSearchImmediate();
      resetAllEditingState();
      cascade.typeFilter = "all";
      detail.clearDetail();
      cascade.refreshKeys(true);
    }
  }
);

onMounted(() => {
  if (connStore.activeConnectionId) {
    cascade.refreshKeys();
  }
});

onBeforeUnmount(() => {
  if (searchDebounceTimer) {
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = null;
  }
  if (filterDebounceTimer) {
    clearTimeout(filterDebounceTimer);
    filterDebounceTimer = null;
  }
});
</script>

<template>
  <div class="h-full flex flex-col min-w-[700px]">
    <div class="flex-1 flex min-h-0">
    <!-- Left Panel: Key Tree -->
    <div class="w-72 border-r border-border flex flex-col bg-bg-secondary shrink-0">
      <!-- Search & Filter -->
      <div class="p-3 space-y-2 border-b border-border-light">
        <div class="relative">
          <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
          <input v-model="cascade.searchQuery" :placeholder="t('browser.search')" :disabled="!isConnected"
            class="w-full pl-8 pr-3 py-1.5 text-xs bg-bg-primary border border-border rounded-lg focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 disabled:opacity-50 disabled:cursor-not-allowed" />
        </div>
        <div class="flex items-center gap-2">
          <select v-model="cascade.typeFilter" :disabled="!isConnected" class="flex-1 px-2 py-1.5 text-xs bg-bg-primary border border-border rounded-lg focus:outline-none focus:border-redis disabled:opacity-50 disabled:cursor-not-allowed">
            <option value="all">{{ t("browser.allTypes") }}</option>
            <option value="string">String</option>
            <option value="hash">Hash</option>
            <option value="list">List</option>
            <option value="set">Set</option>
            <option value="zset">ZSet</option>
          </select>
          <button @click="cascade.refreshKeys()" :disabled="!isConnected" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
            <RefreshCw :size="14" :class="cascade.loading ? 'animate-spin' : ''" class="text-text-muted" />
          </button>
          <button @click="openNewKeyDialog" :disabled="!isConnected" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-redis/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed" :title="t('browser.addKey')">
            <Plus :size="14" class="text-redis" />
          </button>
        </div>
        <div class="text-[11px] text-text-muted flex items-center justify-between">
          <span>
            {{ cascade.totalKeyCount > cascade.loadedCount
              ? t("browser.keyCountWithTotal", { loaded: cascade.loadedCount, total: cascade.totalKeyCount })
              : t("browser.keyCount", { count: cascade.keyCount }) }}
          </span>
          <button
            v-if="cascade.hasMore && isConnected"
            @click="cascade.loadMoreKeys()"
            :disabled="cascade.loading"
            class="text-[11px] text-redis hover:text-redis/80 disabled:opacity-50 transition-colors shrink-0"
          >
            {{ cascade.loading ? "..." : t("browser.loadMore") }}
          </button>
          <span v-else-if="!cascade.hasMore && cascade.keys.length > 0" class="text-[11px] text-text-muted/60">
            {{ t("browser.allLoaded") }}
          </span>
        </div>
      </div>

      <div ref="treeScrollRef" class="flex-1 overflow-y-auto py-1" @scroll="onTreeScroll">
        <div v-if="cascade.filteredKeys.length === 0 && !cascade.loading" class="flex flex-col items-center py-8 text-text-muted">
          <Search :size="24" class="mb-2 opacity-40" />
          <span class="text-xs">{{ t("browser.noKeys") }}</span>
        </div>
        <div v-else :style="{ paddingTop: topPadding + 'px', paddingBottom: bottomPadding + 'px' }">
          <KeyTreeItem
            v-for="item in visibleItems"
            :key="item.node.fullPath"
            :node="item.node"
            :depth="item.depth"
            @select="handleSelect"
          />
        </div>
      </div>

    </div>

    <!-- Center Panel: Key Detail -->
    <div class="flex-1 flex flex-col min-w-0 relative">
      <!-- Loading overlay -->
      <div
        v-if="detail.loading"
        class="absolute inset-0 z-20 flex items-center justify-center bg-bg-primary/60 backdrop-blur-[1px] transition-opacity"
      >
        <div class="flex flex-col items-center gap-2">
          <RefreshCw :size="20" class="animate-spin text-redis" />
          <span class="text-xs text-text-muted">{{ t("common.loading") }}</span>
        </div>
      </div>
      <template v-if="detail.currentDetail">
        <!-- Header with editable key name -->
        <div class="px-4 py-3 border-b border-border-light flex items-center justify-between bg-bg-secondary">
          <div class="flex items-center gap-2 min-w-0">
            <component :is="typeIcons[detail.currentKey!.type]" :size="16" :class="`text-type-${detail.currentKey!.type}`" />
            <!-- Inline key rename -->
            <template v-if="editingKey">
              <input
                v-model="editKeyTemp"
                @keyup.enter="saveEditKey($event)"
                @keyup.escape="cancelEditKey"
                ref="editKeyInputRef"
                class="text-sm font-medium font-mono text-text-primary bg-bg-primary border border-redis rounded px-2 py-0.5 focus:outline-none focus:ring-1 focus:ring-redis/30 min-w-0"
                :placeholder="t('detail.renameKeyPlaceholder')"
              />
              <button @click="saveEditKey($event)" class="text-success hover:text-success/80 shrink-0"><Save :size="13" /></button>
              <button @click="cancelEditKey" class="text-text-muted hover:text-text-primary shrink-0"><X :size="13" /></button>
            </template>
            <template v-else>
              <span
                class="text-sm font-medium text-text-primary truncate max-w-[40%] cursor-pointer hover:text-redis transition-colors"
                @dblclick="startEditKey"
                :title="detail.currentKey!.key"
              >{{ detail.currentKey!.key }}</span>
              <button @click="startEditKey" class="text-text-muted hover:text-text-primary shrink-0">
                <Pencil :size="11" />
              </button>
            </template>
            <span class="badge" :class="typeColors[detail.currentKey!.type]">{{ detail.currentKey!.type }}</span>
            <span v-if="detail.isExpired" class="badge bg-danger/10 text-danger animate-pulse">
              <AlertTriangle :size="10" class="mr-0.5" />{{ t("detail.expired") }}
            </span>
          </div>
          <div class="flex items-center gap-1.5 shrink-0">
            <button @click="copyKey(detail.currentKey!.key, $event)" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover" :title="t('browser.copyKey')">
              <Copy :size="13" class="text-text-muted" />
            </button>
            <button @click="detail.refresh()" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover" :title="t('browser.refresh')">
              <RefreshCw :size="13" class="text-text-muted" />
            </button>
            <button v-if="detail.currentKey && ['hash','list','set','zset'].includes(detail.currentKey.type)" @click="openAddFieldDialog" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover" :title="t('detail.addFieldTitle')">
              <Plus :size="13" class="text-redis" />
            </button>
            <button @click="deleteKey" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-danger/10" :title="t('browser.deleteKey')">
              <Trash2 :size="13" class="text-danger" />
            </button>
          </div>
        </div>

        <div class="flex-1 overflow-auto p-4 flex flex-col min-h-0">
          <!-- String -->
          <div v-if="detail.currentValue?.type === 'string'" class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-3 shrink-0">
              <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }}</label>
              <div class="flex items-center gap-1.5">
                <!-- View mode toggle: Text / Hex / JSON / ASCII -->
                <div class="flex items-center border border-border rounded-lg overflow-hidden mr-1">
                  <button
                    @click="viewMode = 'text'"
                    class="px-2 py-0.5 text-[11px] transition-colors"
                    :class="viewMode === 'text' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Text</button>
                  <button
                    @click="viewMode = 'hex'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'hex' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Hex</button>
                  <button
                    @click="viewMode = 'json'"
                    :disabled="!isJsonContent"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border disabled:opacity-30 disabled:cursor-not-allowed"
                    :class="viewMode === 'json' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >JSON</button>
                  <button
                    @click="viewMode = 'ascii'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'ascii' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >ASCII</button>
                </div>
                <template v-if="editingString">
                  <button @click="saveEditString($event)" class="inline-flex items-center gap-1 px-2.5 py-1 text-[11px] font-medium bg-success/10 text-success rounded-lg hover:bg-success/20 transition-colors">
                    <Save :size="11" /> {{ t("detail.save") }}
                  </button>
                  <button @click="cancelEditString" class="inline-flex items-center gap-1 px-2.5 py-1 text-[11px] font-medium text-text-muted rounded-lg hover:bg-bg-hover transition-colors">
                    <X :size="11" /> {{ t("detail.cancel") }}
                  </button>
                </template>
                <button v-else @click="startEditString" :disabled="!canEditString" :title="!canEditString ? t('detail.hexReadOnly') : ''" class="inline-flex items-center gap-1 px-2.5 py-1 text-[11px] font-medium text-text-secondary rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
                  <Pencil :size="11" /> {{ t("detail.edit") }}
                </button>
              </div>
            </div>
            <textarea
              :value="editingString ? stringTemp : displayStringValue"
              @input="editingString && (stringTemp = ($event.target as HTMLTextAreaElement).value)"
              :readonly="!editingString"
              :class="[
                'flex-1 w-full px-4 py-3 text-sm font-mono border rounded-lg resize-none focus:outline-none min-h-[200px]',
                editingString
                  ? 'bg-bg-secondary border-redis focus:ring-1 focus:ring-redis/30'
                  : 'bg-bg-primary border-border'
              ]"
            />
          </div>

          <!-- Hash -->
          <div v-else-if="detail.currentValue?.type === 'hash'" class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-3 shrink-0">
              <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ t("detail.fieldCount", (detail.currentValue as any).totalCount || (detail.currentValue as any).fields.length) }})</label>
              <div class="flex items-center gap-2">
                <!-- View mode toggle: Text / Hex / JSON / ASCII -->
                <div class="flex items-center border border-border rounded-lg overflow-hidden">
                  <button
                    @click="viewMode = 'text'"
                    class="px-2 py-0.5 text-[11px] transition-colors"
                    :class="viewMode === 'text' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Text</button>
                  <button
                    @click="viewMode = 'hex'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'hex' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Hex</button>
                  <button
                    @click="viewMode = 'json'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'json' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >JSON</button>
                  <button
                    @click="viewMode = 'ascii'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'ascii' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >ASCII</button>
                </div>
                <input
                  type="text"
                  :placeholder="t('detail.searchPlaceholder')"
                  :value="detail.filterPattern"
                  @input="debounceFilter(($event.target as HTMLInputElement).value)"
                  class="w-32 px-2 py-1 text-xs border border-border rounded-lg bg-bg-primary focus:outline-none focus:ring-1 focus:ring-redis/30"
                />
                <div v-if="((detail.currentValue as any).totalCount || 0) > detail.pageSize" class="flex items-center gap-1">
                  <button @click="detail.loadPage(detail.currentPage - 1)" :disabled="detail.currentPage === 0" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronLeft :size="14" />
                  </button>
                  <span class="text-xs text-text-muted">{{ detail.currentPage + 1 }} / {{ Math.ceil((detail.currentValue as any).totalCount / detail.pageSize) }}</span>
                  <button @click="detail.loadPage(detail.currentPage + 1)" :disabled="(detail.currentPage + 1) * detail.pageSize >= ((detail.currentValue as any).totalCount || 0)" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronRight :size="14" />
                  </button>
                </div>
              </div>
            </div>
            <!-- JSON view -->
            <textarea v-if="viewMode === 'json'" :value="compositeJsonStr" readonly class="flex-1 w-full px-4 py-3 text-sm font-mono border border-border rounded-lg resize-none bg-bg-primary focus:outline-none min-h-[200px]" />
            <!-- Table view for Text / Hex / ASCII -->
            <div v-else class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm table-fixed">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border" style="width:35%;max-width:260px">{{ t("detail.field") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border" :style="(detail.currentValue as any)?.hasFieldTtl ? 'width:55%' : ''">{{ t("detail.value") }}</th>
                  <th v-if="(detail.currentValue as any)?.hasFieldTtl" class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border" style="width:15%;min-width:100px">{{ t("detail.fieldTtl") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(f, i) in (detail.currentValue as any).fields" :key="f.field" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 font-mono text-xs text-text-primary font-medium overflow-hidden">
                      <!-- Field name editing -->
                      <div v-if="renamingHashField === f.field" class="flex items-center gap-1.5">
                        <input v-model="hashFieldRenameTemp" @keyup.enter="saveRenameHashField($event)" @keyup.escape="cancelRenameHashField"
                          class="flex-1 text-xs font-mono font-medium px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary min-w-0" />
                        <button @click="saveRenameHashField($event)" class="shrink-0 text-success hover:text-success/80"><Save :size="11" /></button>
                        <button @click="cancelRenameHashField" class="shrink-0 text-text-muted hover:text-text-primary"><X :size="11" /></button>
                      </div>
                      <!-- Field name display with hover edit button -->
                      <div v-else class="flex items-center gap-1 group/field min-w-0">
                        <span class="truncate" :title="viewMode === 'hex' ? (f.fieldHex || f.field) : viewMode === 'ascii' ? (f.fieldHex ? hexToAscii(f.fieldHex) : textToAscii(f.field)) : f.field">{{ viewMode === 'hex' ? (f.fieldHex || f.field) : viewMode === 'ascii' ? (f.fieldHex ? hexToAscii(f.fieldHex) : textToAscii(f.field)) : f.field }}</span>
                        <button v-if="viewMode === 'text'" @click="startRenameHashField(f.field)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/field:opacity-100 transition-opacity">
                          <Pencil :size="10" />
                        </button>
                      </div>
                    </td>
                    <td class="px-3 py-2 font-mono text-xs text-text-secondary truncate max-w-0">
                      <!-- Editing mode -->
                      <div v-if="editingHashField === f.field" class="flex items-center gap-1.5">
                        <input v-model="hashFieldTemp" @keyup.enter="saveEditHash($event)" @keyup.escape="cancelEditHash"
                          class="flex-1 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary" />
                        <button @click="saveEditHash($event)" class="text-success hover:text-success/80"><Save :size="11" /></button>
                        <button @click="cancelEditHash" class="text-text-muted hover:text-text-primary"><X :size="11" /></button>
                      </div>
                      <!-- Display mode: click to view popup, button or double-click to edit -->
                      <div v-else class="flex items-center gap-1 group/cell min-w-0">
                        <span
                          class="truncate cursor-pointer hover:bg-bg-hover rounded px-1 -mx-1 flex-1 min-w-0"
                          @click="showCellPopup($event, viewMode === 'hex' ? (f.valueHex || f.value) : viewMode === 'ascii' ? (f.valueHex ? hexToAscii(f.valueHex) : textToAscii(f.value)) : f.value, f.field, 'hash', f.field)"
                          @dblclick.stop="viewMode === 'text' && startEditHash(f.field, f.value)"
                        >{{ viewMode === 'hex' ? (f.valueHex || f.value) : viewMode === 'ascii' ? (f.valueHex ? hexToAscii(f.valueHex) : textToAscii(f.value)) : f.value }}</span>
                        <button v-if="viewMode === 'text'" @click="startEditHash(f.field, f.value)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/cell:opacity-100 transition-opacity">
                          <Pencil :size="10" />
                        </button>
                        <button v-if="viewMode === 'text'" @click="deleteSubField('hash', { field: f.field })" class="shrink-0 text-text-muted hover:text-danger opacity-0 group-hover/cell:opacity-100 transition-opacity" :title="t('detail.deleteField')">
                          <Trash2 :size="10" />
                        </button>
                      </div>
                    </td>
                    <td v-if="(detail.currentValue as any)?.hasFieldTtl" class="px-3 py-2 font-mono text-xs text-text-secondary overflow-hidden">
                      <!-- Field TTL editing (Redis >= 7.4.0) -->
                      <div v-if="editingFieldTtl === f.field" class="flex items-center gap-1.5">
                        <input v-model="fieldTtlTemp" @keyup.enter="saveEditFieldTtl($event)" @keyup.escape="cancelEditFieldTtl"
                          type="number"
                          class="flex-1 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary w-16"
                          :placeholder="t('detail.setTtlPlaceholder')" />
                        <button @click="saveEditFieldTtl($event)" class="shrink-0 text-success hover:text-success/80"><Save :size="11" /></button>
                        <button @click="cancelEditFieldTtl" class="shrink-0 text-text-muted hover:text-text-primary"><X :size="11" /></button>
                      </div>
                      <div v-else class="flex items-center gap-1 group/ttl min-w-0">
                        <span :class="f.ttl && f.ttl > 0 ? 'text-amber-400' : 'text-text-muted'" :title="f.ttl && f.ttl > 0 ? `${f.ttl}s` : ''">
                          {{ formatFieldTtl(f.ttl) }}
                        </span>
                        <button @click="startEditFieldTtl(f.field, f.ttl)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/ttl:opacity-100 transition-opacity">
                          <Pencil :size="10" />
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>

          <!-- List -->
          <div v-else-if="detail.currentValue?.type === 'list'" class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-3 shrink-0">
              <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ t("detail.itemCount", (detail.currentValue as any).totalCount || (detail.currentValue as any).items.length) }})</label>
              <div class="flex items-center gap-2">
                <!-- View mode toggle: Text / Hex / JSON / ASCII -->
                <div class="flex items-center border border-border rounded-lg overflow-hidden">
                  <button
                    @click="viewMode = 'text'"
                    class="px-2 py-0.5 text-[11px] transition-colors"
                    :class="viewMode === 'text' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Text</button>
                  <button
                    @click="viewMode = 'hex'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'hex' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Hex</button>
                  <button
                    @click="viewMode = 'json'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'json' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >JSON</button>
                  <button
                    @click="viewMode = 'ascii'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'ascii' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >ASCII</button>
                </div>
                <input
                  type="text"
                  :placeholder="t('detail.searchPlaceholder')"
                  :value="detail.filterPattern"
                  @input="debounceFilter(($event.target as HTMLInputElement).value)"
                  class="w-32 px-2 py-1 text-xs border border-border rounded-lg bg-bg-primary focus:outline-none focus:ring-1 focus:ring-redis/30"
                />
                <div v-if="((detail.currentValue as any).totalCount || 0) > detail.pageSize" class="flex items-center gap-1">
                  <button @click="detail.loadPage(detail.currentPage - 1)" :disabled="detail.currentPage === 0" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronLeft :size="14" />
                  </button>
                  <span class="text-xs text-text-muted">{{ detail.currentPage + 1 }} / {{ Math.ceil((detail.currentValue as any).totalCount / detail.pageSize) }}</span>
                  <button @click="detail.loadPage(detail.currentPage + 1)" :disabled="(detail.currentPage + 1) * detail.pageSize >= ((detail.currentValue as any).totalCount || 0)" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronRight :size="14" />
                  </button>
                </div>
              </div>
            </div>
            <!-- JSON view -->
            <textarea v-if="viewMode === 'json'" :value="compositeJsonStr" readonly class="flex-1 w-full px-4 py-3 text-sm font-mono border border-border rounded-lg resize-none bg-bg-primary focus:outline-none min-h-[200px]" />
            <!-- Table view for Text / Hex / ASCII -->
            <div v-else class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-16">{{ t("detail.index") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.value") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(item, i) in (detail.currentValue as any).items" :key="i" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs text-text-muted font-mono">{{ (detail.currentValue as any).originalIndices ? (detail.currentValue as any).originalIndices[i] : detail.currentPage * detail.pageSize + i }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary truncate max-w-0">
                      <div v-if="editingListIndex === i" class="flex items-center gap-1.5">
                        <input v-model="listItemTemp" @keyup.enter="saveEditList($event)" @keyup.escape="cancelEditList"
                          class="flex-1 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary" />
                        <button @click="saveEditList($event)" class="text-success hover:text-success/80"><Save :size="11" /></button>
                        <button @click="cancelEditList" class="text-text-muted hover:text-text-primary"><X :size="11" /></button>
                      </div>
                      <div v-else class="flex items-center gap-1 group/cell min-w-0">
                        <span
                          class="truncate cursor-pointer hover:bg-bg-hover rounded px-1 -mx-1 flex-1 min-w-0"
                          @click="showCellPopup($event, viewMode === 'hex' ? ((detail.currentValue as any).itemsHex?.[i] || item) : viewMode === 'ascii' ? ((detail.currentValue as any).itemsHex?.[i] ? hexToAscii((detail.currentValue as any).itemsHex[i]) : textToAscii(item)) : item, `Index ${(detail.currentValue as any).originalIndices ? (detail.currentValue as any).originalIndices[i] : detail.currentPage * detail.pageSize + i}`, 'list', String((detail.currentValue as any).originalIndices ? (detail.currentValue as any).originalIndices[i] : detail.currentPage * detail.pageSize + i))"
                          @dblclick.stop="viewMode === 'text' && startEditList(i, item)"
                        >{{ viewMode === 'hex' ? ((detail.currentValue as any).itemsHex?.[i] || item) : viewMode === 'ascii' ? ((detail.currentValue as any).itemsHex?.[i] ? hexToAscii((detail.currentValue as any).itemsHex[i]) : textToAscii(item)) : item }}</span>
                        <button v-if="viewMode === 'text'" @click="startEditList(i, item)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/cell:opacity-100 transition-opacity">
                          <Pencil :size="10" />
                        </button>
                        <button v-if="viewMode === 'text'" @click="deleteSubField('list', { value: item })" class="shrink-0 text-text-muted hover:text-danger opacity-0 group-hover/cell:opacity-100 transition-opacity" :title="t('detail.deleteField')">
                          <Trash2 :size="10" />
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>

          <!-- Set -->
          <div v-else-if="detail.currentValue?.type === 'set'" class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-3 shrink-0">
              <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ t("detail.memberCount", (detail.currentValue as any).totalCount || (detail.currentValue as any).members.length) }})</label>
              <div class="flex items-center gap-2">
                <!-- View mode toggle: Text / Hex -->
                <div class="flex items-center border border-border rounded-lg overflow-hidden">
                  <button
                    @click="viewMode = 'text'"
                    class="px-2 py-0.5 text-[11px] transition-colors"
                    :class="viewMode === 'text' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Text</button>
                  <button
                    @click="viewMode = 'hex'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'hex' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Hex</button>
                  <button
                    @click="viewMode = 'json'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'json' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >JSON</button>
                  <button
                    @click="viewMode = 'ascii'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'ascii' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >ASCII</button>
                </div>
                <input
                  type="text"
                  :placeholder="t('detail.searchPlaceholder')"
                  :value="detail.filterPattern"
                  @input="debounceFilter(($event.target as HTMLInputElement).value)"
                  class="w-32 px-2 py-1 text-xs border border-border rounded-lg bg-bg-primary focus:outline-none focus:ring-1 focus:ring-redis/30"
                />
                <div v-if="((detail.currentValue as any).totalCount || 0) > detail.pageSize" class="flex items-center gap-1">
                  <button @click="detail.loadPage(detail.currentPage - 1)" :disabled="detail.currentPage === 0" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronLeft :size="14" />
                  </button>
                  <span class="text-xs text-text-muted">{{ detail.currentPage + 1 }} / {{ Math.ceil((detail.currentValue as any).totalCount / detail.pageSize) }}</span>
                  <button @click="detail.loadPage(detail.currentPage + 1)" :disabled="(detail.currentPage + 1) * detail.pageSize >= ((detail.currentValue as any).totalCount || 0)" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronRight :size="14" />
                  </button>
                </div>
              </div>
            </div>
            <!-- JSON view -->
            <textarea v-if="viewMode === 'json'" :value="compositeJsonStr" readonly class="flex-1 w-full px-4 py-3 text-sm font-mono border border-border rounded-lg resize-none bg-bg-primary focus:outline-none min-h-[200px]" />
            <!-- List view for Text / Hex / ASCII -->
            <div v-else class="flex-1 min-h-0 overflow-y-auto space-y-1">
              <div v-for="(m, i) in (detail.currentValue as any).members" :key="m"
                class="px-3 py-2 text-xs font-mono bg-bg-primary border border-border-light rounded-lg flex items-center gap-2">
                <span class="text-text-muted w-6 text-right shrink-0">{{ detail.currentPage * detail.pageSize + i + 1 }}</span>
                <!-- Editing -->
                <div v-if="editingSetMember === m" class="flex items-center gap-1.5 flex-1 min-w-0">
                  <input v-model="setMemberTemp" @keyup.enter="saveEditSet($event)" @keyup.escape="cancelEditSet"
                    class="flex-1 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary min-w-0" />
                  <button @click="saveEditSet($event)" class="text-success hover:text-success/80 shrink-0"><Save :size="11" /></button>
                  <button @click="cancelEditSet" class="text-text-muted hover:text-text-primary shrink-0"><X :size="11" /></button>
                </div>
                <!-- Display -->
                <div v-else class="flex items-center gap-1 flex-1 min-w-0 group/cell">
                  <span
                    class="text-text-primary truncate cursor-pointer hover:bg-bg-hover rounded px-1 -mx-1 flex-1 min-w-0"
                    @click="showCellPopup($event, viewMode === 'hex' ? ((detail.currentValue as any).membersHex?.[i] || m) : viewMode === 'ascii' ? ((detail.currentValue as any).membersHex?.[i] ? hexToAscii((detail.currentValue as any).membersHex[i]) : textToAscii(m)) : m, `Member ${detail.currentPage * detail.pageSize + i + 1}`, 'set', m)"
                    @dblclick.stop="viewMode === 'text' && startEditSet(m)"
                  >{{ viewMode === 'hex' ? ((detail.currentValue as any).membersHex?.[i] || m) : viewMode === 'ascii' ? ((detail.currentValue as any).membersHex?.[i] ? hexToAscii((detail.currentValue as any).membersHex[i]) : textToAscii(m)) : m }}</span>
                  <button v-if="viewMode === 'text'" @click="startEditSet(m)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/cell:opacity-100 transition-opacity">
                    <Pencil :size="10" />
                  </button>
                  <button v-if="viewMode === 'text'" @click="deleteSubField('set', { value: m })" class="shrink-0 text-text-muted hover:text-danger opacity-0 group-hover/cell:opacity-100 transition-opacity" :title="t('detail.deleteField')">
                    <Trash2 :size="10" />
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- ZSet -->
          <div v-else-if="detail.currentValue?.type === 'zset'" class="flex flex-col min-h-0">
            <div class="flex items-center justify-between mb-3 shrink-0">
              <label class="text-xs font-medium text-text-secondary">{{ t("detail.value") }} ({{ t("detail.memberCount", (detail.currentValue as any).totalCount || (detail.currentValue as any).members.length) }})</label>
              <div class="flex items-center gap-2">
                <!-- View mode toggle: Text / Hex -->
                <div class="flex items-center border border-border rounded-lg overflow-hidden">
                  <button
                    @click="viewMode = 'text'"
                    class="px-2 py-0.5 text-[11px] transition-colors"
                    :class="viewMode === 'text' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Text</button>
                  <button
                    @click="viewMode = 'hex'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'hex' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >Hex</button>
                  <button
                    @click="viewMode = 'json'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'json' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >JSON</button>
                  <button
                    @click="viewMode = 'ascii'"
                    class="px-2 py-0.5 text-[11px] transition-colors border-l border-border"
                    :class="viewMode === 'ascii' ? 'bg-redis/10 text-redis font-medium' : 'text-text-muted hover:bg-bg-hover'"
                  >ASCII</button>
                </div>
                <input
                  type="text"
                  :placeholder="t('detail.searchPlaceholder')"
                  :value="detail.filterPattern"
                  @input="debounceFilter(($event.target as HTMLInputElement).value)"
                  class="w-32 px-2 py-1 text-xs border border-border rounded-lg bg-bg-primary focus:outline-none focus:ring-1 focus:ring-redis/30"
                />
                <div v-if="((detail.currentValue as any).totalCount || 0) > detail.pageSize" class="flex items-center gap-1">
                  <button @click="detail.loadPage(detail.currentPage - 1)" :disabled="detail.currentPage === 0" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronLeft :size="14" />
                  </button>
                  <span class="text-xs text-text-muted">{{ detail.currentPage + 1 }} / {{ Math.ceil((detail.currentValue as any).totalCount / detail.pageSize) }}</span>
                  <button @click="detail.loadPage(detail.currentPage + 1)" :disabled="(detail.currentPage + 1) * detail.pageSize >= ((detail.currentValue as any).totalCount || 0)" class="p-1 text-text-muted rounded hover:bg-bg-hover disabled:opacity-30">
                    <ChevronRight :size="14" />
                  </button>
                </div>
              </div>
            </div>
            <!-- JSON view -->
            <textarea v-if="viewMode === 'json'" :value="compositeJsonStr" readonly class="flex-1 w-full px-4 py-3 text-sm font-mono border border-border rounded-lg resize-none bg-bg-primary focus:outline-none min-h-[200px]" />
            <!-- Table view for Text / Hex / ASCII -->
            <div v-else class="border border-border rounded-lg flex-1 min-h-0">
              <div class="h-full overflow-y-auto">
              <table class="w-full text-sm">
                <thead class="sticky top-0 z-10"><tr class="bg-bg-primary">
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border w-24">{{ t("detail.score") }}</th>
                  <th class="text-left px-3 py-2 text-xs font-semibold text-text-secondary border-b border-border">{{ t("detail.member") }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(m, i) in (detail.currentValue as any).members" :key="m.member" class="border-b border-border-light last:border-0" :class="i % 2 ? 'bg-bg-primary/50' : ''">
                    <td class="px-3 py-2 text-xs font-mono text-redis font-semibold">{{ m.score.toLocaleString() }}</td>
                    <td class="px-3 py-2 font-mono text-xs text-text-primary truncate max-w-0">
                      <div v-if="editingZSetMember === m.member" class="flex items-center gap-1.5">
                        <input v-model.number="zsetScoreTemp" type="number" @keyup.enter="saveEditZSet($event)" @keyup.escape="cancelEditZSet"
                          class="w-16 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary" />
                        <input v-model="zsetMemberTemp" @keyup.enter="saveEditZSet($event)" @keyup.escape="cancelEditZSet"
                          class="flex-1 text-xs font-mono px-2 py-0.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary min-w-0" />
                        <button @click="saveEditZSet($event)" class="text-success hover:text-success/80"><Save :size="11" /></button>
                        <button @click="cancelEditZSet" class="text-text-muted hover:text-text-primary"><X :size="11" /></button>
                      </div>
                      <div v-else class="flex items-center gap-1 group/cell min-w-0">
                        <span
                          class="truncate cursor-pointer hover:bg-bg-hover rounded px-1 -mx-1 flex-1 min-w-0"
                          @click="showCellPopup($event, viewMode === 'hex' ? (m.memberHex || m.member) : viewMode === 'ascii' ? (m.memberHex ? hexToAscii(m.memberHex) : textToAscii(m.member)) : m.member, `Score: ${m.score}`, 'zset', m.member)"
                          @dblclick.stop="viewMode === 'text' && startEditZSet(m.member, m.score)"
                        >{{ viewMode === 'hex' ? (m.memberHex || m.member) : viewMode === 'ascii' ? (m.memberHex ? hexToAscii(m.memberHex) : textToAscii(m.member)) : m.member }}</span>
                        <button v-if="viewMode === 'text'" @click="startEditZSet(m.member, m.score)" class="shrink-0 text-text-muted hover:text-text-primary opacity-0 group-hover/cell:opacity-100 transition-opacity">
                          <Pencil :size="10" />
                        </button>
                        <button v-if="viewMode === 'text'" @click="deleteSubField('zset', { value: m.member })" class="shrink-0 text-text-muted hover:text-danger opacity-0 group-hover/cell:opacity-100 transition-opacity" :title="t('detail.deleteField')">
                          <Trash2 :size="10" />
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div v-else-if="detail.isExpired && cascade.selectedKey" class="flex-1 flex flex-col items-center justify-center text-danger">
        <AlertTriangle :size="36" class="mb-3 opacity-60" />
        <p class="text-sm font-semibold">{{ t("detail.expired") }}</p>
        <p class="text-xs mt-1 opacity-60 font-mono truncate max-w-[300px] text-center" :title="cascade.selectedKey">{{ cascade.selectedKey }}</p>
        <button @click="detail.refresh()" class="mt-4 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs text-text-secondary bg-bg-hover rounded-lg hover:bg-bg-active transition-colors">
          <RefreshCw :size="12" />{{ t("browser.refresh") }}
        </button>
      </div>

      <div v-else class="flex-1 flex flex-col items-center justify-center text-text-muted">
        <Tag :size="36" class="mb-3 opacity-30" />
        <p class="text-sm">{{ t("browser.selectKey") }}</p>
      </div>
    </div>

    <!-- Right Panel: TTL Gauge + Meta -->
    <div class="w-56 border-l border-border bg-bg-secondary p-4 flex flex-col gap-4 shrink-0" v-if="detail.currentDetail">
      <div class="text-center">
        <div class="flex items-center justify-center gap-2 mb-3">
          <p class="text-xs font-medium text-text-secondary">{{ t("detail.ttl") }}</p>
        </div>
        <TtlGauge :ttl-remaining="detail.ttlRemaining" :ttl-total="detail.ttlTotal" />
        <!-- TTL display / edit -->
        <template v-if="editingTtl">
          <div class="flex items-center gap-1.5 mt-2 justify-center">
            <input v-model="ttlTemp" @keyup.enter="saveEditTtl($event)" @keyup.escape="cancelEditTtl"
              type="number"
              class="w-20 text-xs font-mono px-2 py-1 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-secondary text-center"
              :placeholder="t('detail.setTtlPlaceholder')"
            />
            <button @click="saveEditTtl($event)" class="w-7 h-7 flex items-center justify-center rounded-lg bg-success/10 text-success hover:bg-success/20 transition-colors"><Save :size="13" /></button>
            <button @click="cancelEditTtl" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover text-text-muted hover:text-text-primary transition-colors"><X :size="13" /></button>
          </div>
          <p class="text-[10px] text-text-muted mt-1">{{ t("detail.setTtlPlaceholder") }}</p>
        </template>
        <div v-else class="flex items-center justify-center gap-1.5 mt-2">
          <p class="text-xs text-text-muted" :class="detail.isExpired ? 'text-danger font-semibold' : ''">{{ formatTtl(detail.ttlRemaining) }}</p>
          <button @click="startEditTtl" class="text-text-muted hover:text-text-primary transition-colors" :title="t('detail.setTtl')"><Pencil :size="10" /></button>
        </div>
        <div v-if="detail.isExpired" class="mt-2 px-2 py-1.5 bg-danger/5 border border-danger/20 rounded-lg">
          <p class="text-[11px] text-danger font-medium flex items-center gap-1 justify-center">
            <AlertTriangle :size="11" /> {{ t("detail.expired") }}
          </p>
        </div>
      </div>
      <div class="divider" />
      <div class="space-y-3">
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.type") }}</span>
          <span class="badge text-[10px]" :class="typeColors[detail.currentKey!.type]">{{ detail.currentKey!.type.toUpperCase() }}</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.size") }}</span>
          <span class="text-text-primary font-medium">{{ formatSize(detail.currentKey!.size) }}</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.encoding") }}</span>
          <span class="text-text-primary font-mono">{{ (detail.currentValue as any).encoding }}</span>
        </div>
        <div v-if="(detail.currentValue as any).contentEncoding" class="flex justify-between text-xs">
          <span class="text-text-muted">{{ t("detail.contentEncoding") }}</span>
          <span class="text-text-primary font-mono">{{ (detail.currentValue as any).contentEncoding }}</span>
        </div>
      </div>
    </div>
    </div>

    <!-- Floating windows (Teleport to body to avoid overflow clipping) -->
    <Teleport to="body">
      <FloatingWindow
        v-for="win in floatingWindows"
        :key="win.id"
        :id="win.id"
        :title="win.title"
        :content="win.content"
        :redis-key="win.redisKey"
        :cell-type="win.cellType"
        :cell-id="win.cellId"
        :x="win.x"
        :y="win.y"
        :width="win.width"
        :height="win.height"
        :pinned="win.pinned"
        :z-index="win.zIndex"
        @close="closeFloatingWin"
        @toggle-pin="togglePin"
        @update-position="updateWinPosition"
        @update-size="updateWinSize"
        @focus="focusWin"
        :on-save-content="handleSaveContent"
      />
    </Teleport>

    <!-- New Key Dialog -->
    <Teleport to="body">
      <div v-if="showNewKeyDialog" class="fixed inset-0 z-[9998] flex items-center justify-center" @click.self="closeNewKeyDialog">
        <div class="absolute inset-0 bg-black/50 backdrop-blur-[3px]" />
        <div class="relative z-[9999] bg-bg-secondary border border-border rounded-2xl shadow-[0_20px_60px_-10px_rgba(0,0,0,0.3)] w-[480px] max-w-[92vw] max-h-[88vh] flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-200">
          <!-- Header -->
          <div class="flex items-center gap-3 px-6 py-4 border-b border-border shrink-0 bg-gradient-to-r from-redis/5 to-transparent">
            <div class="w-8 h-8 rounded-lg bg-redis/10 flex items-center justify-center shrink-0">
              <Database :size="16" class="text-redis" />
            </div>
            <div class="flex-1 min-w-0">
              <h3 class="text-sm font-semibold text-text-primary">{{ t('browser.addKeyTitle') }}</h3>
              <p class="text-[11px] text-text-muted mt-0.5">{{ t('browser.keyType') }}: <span class="font-mono font-medium text-text-secondary">{{ newKeyType.toUpperCase() }}</span></p>
            </div>
            <button @click="closeNewKeyDialog" class="w-7 h-7 flex items-center justify-center rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"><X :size="15" /></button>
          </div>
          <!-- Body (scrollable) -->
          <div class="px-6 py-5 space-y-5 overflow-y-auto flex-1 min-h-0">
            <!-- Key Type Selector -->
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-2">{{ t('browser.keyType') }}</label>
              <div class="flex gap-1.5">
                <button v-for="tp in (['string','hash','list','set','zset'] as RedisDataType[])" :key="tp" @click="newKeyType = tp"
                  class="group flex-1 flex items-center justify-center gap-1.5 py-2 px-2 rounded-lg border transition-all duration-200"
                  :style="newKeyType === tp ? { borderColor: typeBorderColors[tp], backgroundColor: typeBorderColors[tp] + '14' } : undefined"
                  :class="newKeyType === tp ? '' : 'border-border bg-bg-primary hover:bg-bg-hover'">
                  <component :is="typeIcons[tp]" :size="14" :class="newKeyType === tp ? `text-type-${tp}` : 'text-text-muted group-hover:text-text-secondary'" class="transition-colors shrink-0" />
                  <span class="text-[11px] font-semibold uppercase tracking-wide" :class="newKeyType === tp ? `text-type-${tp}` : 'text-text-muted group-hover:text-text-secondary'">{{ tp }}</span>
                </button>
              </div>
            </div>
            <!-- Key Name -->
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t('browser.keyName') }}</label>
              <div class="relative">
                <Key :size="14" class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
                <input v-model="newKeyName" :placeholder="t('browser.keyNamePlaceholder')" class="w-full pl-9 pr-3 py-2.5 text-sm font-mono bg-bg-primary border border-border rounded-xl focus:outline-none focus:border-redis focus:ring-2 focus:ring-redis/20 transition-all" @keyup.enter="submitNewKey" />
              </div>
            </div>
            <!-- TTL -->
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t('browser.ttl') }}</label>
              <div class="relative">
                <Clock :size="14" class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
                <input v-model="newKeyTtl" type="number" min="1" :placeholder="t('browser.ttlPlaceholder')" class="w-full pl-9 pr-3 py-2.5 text-sm font-mono bg-bg-primary border border-border rounded-xl focus:outline-none focus:border-redis focus:ring-2 focus:ring-redis/20 transition-all" />
              </div>
            </div>
            <!-- Initial Data -->
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t('browser.batchData') }}</label>
              <p class="text-[11px] text-text-muted mb-2 flex items-center gap-1">
                <Code2 :size="11" class="shrink-0 opacity-60" />
                {{ newKeyBatchHint }}
              </p>
              <NumberedTextarea v-model="newKeyBatchData" :placeholder="newKeyBatchPlaceholder || newKeyBatchHint" :rows="5" />
            </div>
          </div>
          <!-- Footer -->
          <div class="flex items-center justify-end gap-2.5 px-6 py-3.5 border-t border-border bg-bg-primary/30 shrink-0">
            <button @click="closeNewKeyDialog" class="px-4 py-2 text-xs font-medium text-text-muted rounded-xl hover:bg-bg-hover hover:text-text-primary transition-colors">{{ t('common.cancel') }}</button>
            <button @click="submitNewKey" :disabled="newKeyLoading" class="px-5 py-2 text-xs font-semibold text-white bg-redis rounded-xl hover:bg-redis/90 hover:shadow-md hover:shadow-redis/20 transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5">
              <RefreshCw v-if="newKeyLoading" :size="12" class="animate-spin" />
              <Plus v-else :size="12" />
              {{ newKeyLoading ? t('common.loading') : t('common.add') }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Add Field Dialog -->
    <Teleport to="body">
      <div v-if="showAddFieldDialog" class="fixed inset-0 z-[9998] flex items-center justify-center" @click.self="closeAddFieldDialog">
        <div class="absolute inset-0 bg-black/50 backdrop-blur-[3px]" />
        <div class="relative z-[9999] bg-bg-secondary border border-border rounded-2xl shadow-[0_20px_60px_-10px_rgba(0,0,0,0.3)] w-[480px] max-w-[92vw] max-h-[78vh] flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-200">
          <!-- Header -->
          <div class="flex items-center gap-3 px-6 py-4 border-b border-border shrink-0 bg-gradient-to-r from-redis/5 to-transparent">
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0" :class="`bg-type-${detail.currentKey?.type}/10`">
              <component :is="typeIcons[detail.currentKey?.type || 'string']" :size="16" :class="`text-type-${detail.currentKey?.type}`" />
            </div>
            <div class="flex-1 min-w-0">
              <h3 class="text-sm font-semibold text-text-primary">{{ t('detail.addFieldTitle') }}</h3>
              <p class="text-[11px] text-text-muted mt-0.5 font-mono">{{ detail.currentKey?.type?.toUpperCase() }} · {{ detail.currentKey?.key }}</p>
            </div>
            <button @click="closeAddFieldDialog" class="w-7 h-7 flex items-center justify-center rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"><X :size="15" /></button>
          </div>
          <!-- Body -->
          <div class="px-6 py-5 space-y-5 overflow-y-auto flex-1 min-h-0">
            <!-- Batch Data -->
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t('browser.batchData') }}</label>
              <p class="text-[11px] text-text-muted mb-2 flex items-center gap-1">
                <Code2 :size="11" class="shrink-0 opacity-60" />
                {{ addFieldBatchHint }}
              </p>
              <NumberedTextarea v-model="addFieldBatchData" :placeholder="addFieldBatchPlaceholder || addFieldBatchHint" :rows="7" />
            </div>
            <!-- Field TTL (hash only, Redis >= 7.4) -->
            <div v-if="addFieldSupportsTtl" class="pt-1">
              <div class="flex items-center gap-2 mb-1.5">
                <label class="text-xs font-medium text-text-secondary">{{ t('detail.addFieldTtl') }}</label>
                <span class="text-[10px] px-1.5 py-0.5 rounded-md bg-amber-500/10 text-amber-500 font-medium">Redis ≥ 7.4</span>
              </div>
              <div class="relative">
                <Clock :size="14" class="absolute left-3 top-1/2 -translate-y-1/2 text-amber-500/60 pointer-events-none" />
                <input v-model="addFieldTtl" type="number" min="1" :placeholder="t('detail.addFieldTtlPlaceholder')" class="w-full pl-9 pr-3 py-2.5 text-sm font-mono bg-bg-primary border border-border rounded-xl focus:outline-none focus:border-amber-500 focus:ring-2 focus:ring-amber-500/20 transition-all" />
              </div>
            </div>
          </div>
          <!-- Footer -->
          <div class="flex items-center justify-end gap-2.5 px-6 py-3.5 border-t border-border bg-bg-primary/30 shrink-0">
            <button @click="closeAddFieldDialog" class="px-4 py-2 text-xs font-medium text-text-muted rounded-xl hover:bg-bg-hover hover:text-text-primary transition-colors">{{ t('common.cancel') }}</button>
            <button @click="submitAddFieldDialog" :disabled="addFieldLoading" class="px-5 py-2 text-xs font-semibold text-white bg-redis rounded-xl hover:bg-redis/90 hover:shadow-md hover:shadow-redis/20 transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5">
              <RefreshCw v-if="addFieldLoading" :size="12" class="animate-spin" />
              <Plus v-else :size="12" />
              {{ addFieldLoading ? t('common.loading') : t('common.add') }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <ConfirmDialog ref="confirmDialog" />
  </div>
</template>
