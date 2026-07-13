<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from "vue";
import { X, Pin, PinOff, Copy, Check, KeyRound, Pencil } from "lucide-vue-next";
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const props = withDefaults(
  defineProps<{
    id: string;
    title: string;
    content: string;
    redisKey?: string;
    cellType?: string;
    cellId?: string;
    x: number;
    y: number;
    width?: number;
    height?: number;
    pinned?: boolean;
    zIndex?: number;
    onSaveContent?: (id: string, newContent: string) => Promise<boolean>;
  }>(),
  { width: 380, height: 240, pinned: false, zIndex: 9999, redisKey: '', cellType: '', cellId: '' }
);

const emit = defineEmits<{
  (e: "close", id: string): void;
  (e: "togglePin", id: string): void;
  (e: "updatePosition", id: string, x: number, y: number): void;
  (e: "updateSize", id: string, w: number, h: number): void;
  (e: "focus", id: string): void;
}>();

// Position & size
const posX = ref(props.x);
const posY = ref(props.y);
const winW = ref(props.width);
const winH = ref(props.height);

// Drag state
const isDragging = ref(false);
const dragStartX = ref(0);
const dragStartY = ref(0);
const dragStartPosX = ref(0);
const dragStartPosY = ref(0);

// Resize state
const isResizing = ref(false);
const resizeDir = ref("");
const resizeStartX = ref(0);
const resizeStartY = ref(0);
const resizeStartW = ref(0);
const resizeStartH = ref(0);
const resizeStartPosX = ref(0);
const resizeStartPosY = ref(0);

// Copy feedback
const copied = ref(false);
const copiedKey = ref(false);
let copyTimer: ReturnType<typeof setTimeout> | null = null;
let copyKeyTimer: ReturnType<typeof setTimeout> | null = null;

// Edit mode
const isEditing = ref(false);
const editTemp = ref('');
const isSaving = ref(false);

// Sync editTemp with content prop when it changes externally (e.g. after save)
watch(() => props.content, (val) => {
  editTemp.value = val;
});

function startEdit() {
  editTemp.value = props.content;
  isEditing.value = true;
}

function cancelEdit() {
  isEditing.value = false;
}

async function saveEdit() {
  if (isSaving.value) return;
  isSaving.value = true;
  try {
    const ok = props.onSaveContent ? await props.onSaveContent(props.id, editTemp.value) : false;
    if (ok) isEditing.value = false;
  } finally {
    isSaving.value = false;
  }
}

const MIN_W = 220;
const MIN_H = 140;

function onDragStart(e: MouseEvent) {
  // Ignore if clicking on buttons
  if ((e.target as HTMLElement).closest("button")) return;
  e.preventDefault();
  isDragging.value = true;
  dragStartX.value = e.clientX;
  dragStartY.value = e.clientY;
  dragStartPosX.value = posX.value;
  dragStartPosY.value = posY.value;
  emit("focus", props.id);
  document.addEventListener("mousemove", onDragMove);
  document.addEventListener("mouseup", onDragEnd);
}

function onDragMove(e: MouseEvent) {
  if (!isDragging.value) return;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const newX = dragStartPosX.value + (e.clientX - dragStartX.value);
  const newY = dragStartPosY.value + (e.clientY - dragStartY.value);
  posX.value = Math.max(0, Math.min(newX, vw - winW.value));
  posY.value = Math.max(0, Math.min(newY, vh - 40));
}

function onDragEnd() {
  isDragging.value = false;
  emit("updatePosition", props.id, posX.value, posY.value);
  document.removeEventListener("mousemove", onDragMove);
  document.removeEventListener("mouseup", onDragEnd);
}

function onResizeStart(e: MouseEvent, dir: string) {
  e.preventDefault();
  e.stopPropagation();
  isResizing.value = true;
  resizeDir.value = dir;
  resizeStartX.value = e.clientX;
  resizeStartY.value = e.clientY;
  resizeStartW.value = winW.value;
  resizeStartH.value = winH.value;
  resizeStartPosX.value = posX.value;
  resizeStartPosY.value = posY.value;
  document.addEventListener("mousemove", onResizeMove);
  document.addEventListener("mouseup", onResizeEnd);
}

function onResizeMove(e: MouseEvent) {
  if (!isResizing.value) return;
  const dx = e.clientX - resizeStartX.value;
  const dy = e.clientY - resizeStartY.value;
  const dir = resizeDir.value;
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  if (dir.includes("e")) {
    winW.value = Math.max(MIN_W, Math.min(resizeStartW.value + dx, vw - posX.value));
  }
  if (dir.includes("s")) {
    winH.value = Math.max(MIN_H, Math.min(resizeStartH.value + dy, vh - posY.value));
  }
  if (dir.includes("w")) {
    const newW = Math.max(MIN_W, resizeStartW.value - dx);
    const newPosX = Math.max(0, resizeStartPosX.value + (resizeStartW.value - newW));
    winW.value = newW;
    posX.value = newPosX;
  }
  if (dir.includes("n")) {
    const newH = Math.max(MIN_H, resizeStartH.value - dy);
    const newPosY = Math.max(0, resizeStartPosY.value + (resizeStartH.value - newH));
    winH.value = newH;
    posY.value = newPosY;
  }
}

function onResizeEnd() {
  isResizing.value = false;
  emit("updateSize", props.id, winW.value, winH.value);
  emit("updatePosition", props.id, posX.value, posY.value);
  document.removeEventListener("mousemove", onResizeMove);
  document.removeEventListener("mouseup", onResizeEnd);
}

async function copyText(text: string, which: 'content' | 'key') {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
  }
  if (which === 'content') {
    copied.value = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied.value = false), 1500);
  } else {
    copiedKey.value = true;
    if (copyKeyTimer) clearTimeout(copyKeyTimer);
    copyKeyTimer = setTimeout(() => (copiedKey.value = false), 1500);
  }
}

const style = computed(() => ({
  left: posX.value + "px",
  top: posY.value + "px",
  width: winW.value + "px",
  height: winH.value + "px",
  zIndex: props.zIndex,
}));

// Resize handles: n, s, e, w, ne, nw, se, sw
const handles = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

function handleStyle(dir: string): Record<string, string> {
  const base: Record<string, string> = { position: "absolute", zIndex: "10" };
  const t = "3px";
  if (dir === "n") return { ...base, top: `-${t}`, left: t, right: t, height: "6px", cursor: "ns-resize" };
  if (dir === "s") return { ...base, bottom: `-${t}`, left: t, right: t, height: "6px", cursor: "ns-resize" };
  if (dir === "e") return { ...base, right: `-${t}`, top: t, bottom: t, width: "6px", cursor: "ew-resize" };
  if (dir === "w") return { ...base, left: `-${t}`, top: t, bottom: t, width: "6px", cursor: "ew-resize" };
  if (dir === "nw") return { ...base, top: `-${t}`, left: `-${t}`, width: "10px", height: "10px", cursor: "nwse-resize" };
  if (dir === "ne") return { ...base, top: `-${t}`, right: `-${t}`, width: "10px", height: "10px", cursor: "nesw-resize" };
  if (dir === "sw") return { ...base, bottom: `-${t}`, left: `-${t}`, width: "10px", height: "10px", cursor: "nesw-resize" };
  if (dir === "se") return { ...base, bottom: `-${t}`, right: `-${t}`, width: "10px", height: "10px", cursor: "nwse-resize" };
  return base;
}

onBeforeUnmount(() => {
  document.removeEventListener("mousemove", onDragMove);
  document.removeEventListener("mouseup", onDragEnd);
  document.removeEventListener("mousemove", onResizeMove);
  document.removeEventListener("mouseup", onResizeEnd);
  if (copyTimer) clearTimeout(copyTimer);
  if (copyKeyTimer) clearTimeout(copyKeyTimer);
});
</script>

<template>
  <div
    class="fixed bg-bg-secondary border rounded-xl shadow-2xl flex flex-col overflow-hidden select-none"
    :class="[isDragging ? 'border-info/50' : 'border-border']"
    :style="style"
    @mousedown="emit('focus', props.id)"
  >
    <!-- Resize handles -->
    <div
      v-for="dir in handles"
      :key="dir"
      :style="handleStyle(dir)"
      @mousedown="onResizeStart($event, dir)"
    />

    <!-- Title bar (draggable) -->
    <div
      class="flex items-center justify-between px-3 py-2 border-b border-border-light bg-bg-primary shrink-0 cursor-move"
      @mousedown="onDragStart"
    >
      <div class="flex flex-col min-w-0 flex-1 mr-2">
        <div v-if="redisKey" class="flex items-center gap-1 min-w-0 leading-tight mb-0.5">
          <KeyRound :size="11" class="text-redis shrink-0" />
          <span class="text-xs font-mono text-redis truncate flex-1">{{ redisKey }}</span>
        </div>
        <span class="text-xs font-semibold text-text-secondary truncate leading-tight">{{ title }}</span>
      </div>
      <div class="flex items-center gap-1 shrink-0">
        <!-- Edit button (only when cellType is set) -->
        <button
          v-if="cellType && !isEditing"
          @click.stop="startEdit"
          class="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
          :title="t('detail.edit')"
        >
          <Pencil :size="12" />
        </button>
        <!-- Copy Key button -->
        <button
          v-if="redisKey"
          @click.stop="copyText(redisKey, 'key')"
          class="p-1 rounded text-text-muted hover:text-redis hover:bg-bg-hover transition-colors flex items-center gap-0.5"
          :title="t('browser.copyKey')"
        >
          <Check :size="12" v-if="copiedKey" class="text-success" />
          <template v-else>
            <KeyRound :size="10" />
            <Copy :size="12" />
          </template>
        </button>
        <!-- Copy Content button -->
        <button
          @click.stop="copyText(content, 'content')"
          class="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
          :title="t('common.copy')"
        >
          <Check :size="12" v-if="copied" class="text-success" />
          <Copy :size="12" v-else />
        </button>
        <!-- Pin/Unpin button -->
        <button
          @click.stop="emit('togglePin', props.id)"
          class="p-1 rounded transition-colors"
          :class="pinned
            ? 'text-info hover:text-info/80 hover:bg-bg-hover'
            : 'text-text-muted hover:text-text-primary hover:bg-bg-hover'"
          :title="pinned ? t('floating.unpin') : t('floating.pin')"
        >
          <PinOff :size="12" v-if="pinned" />
          <Pin :size="12" v-else />
        </button>
        <!-- Close button -->
        <button
          @click.stop="emit('close', props.id)"
          class="p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors"
          :title="t('common.close')"
        >
          <X :size="12" />
        </button>
      </div>
    </div>

    <!-- Content -->
    <div v-if="isEditing" class="flex flex-col flex-1 min-h-0 p-2 gap-2">
      <textarea
        v-model="editTemp"
        class="flex-1 text-xs font-mono px-2 py-1.5 border border-redis rounded focus:outline-none focus:ring-1 focus:ring-redis/30 bg-bg-primary text-text-primary resize-none"
        @keydown.escape="cancelEdit"
      />
      <div class="flex items-center justify-end gap-1.5 shrink-0">
        <button @click.stop="cancelEdit" class="px-2 py-1 text-xs rounded hover:bg-bg-hover text-text-muted hover:text-text-primary transition-colors">
          {{ t("detail.cancel") }}
        </button>
        <button @click.stop="saveEdit" :disabled="isSaving" class="px-2 py-1 text-xs rounded bg-success/10 text-success hover:bg-success/20 transition-colors disabled:opacity-50">
          {{ t("detail.save") }}
        </button>
      </div>
    </div>
    <div v-else class="px-3 py-2.5 text-xs font-mono text-text-primary overflow-auto whitespace-pre-wrap break-all flex-1">
      {{ content }}
    </div>
  </div>
</template>
