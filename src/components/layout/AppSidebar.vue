<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCapabilityStore } from "@/stores/capabilityStore";
import { Database, Server, Layers, FlaskConical, History, Plus, Unplug, X, Pin, PinOff, PanelLeftClose, PanelLeftOpen, Radio, Gauge, ListTree, Radar, TriangleAlert } from "lucide-vue-next";
import type { RedisConnection } from "@/types";
import { toast } from "@/utils/toast";
import { sidebarCollapsed, toggleSidebar, getDotColor } from "@/utils/uiSettings";

const router = useRouter();
const route = useRoute();
const { t } = useI18n();
const connStore = useConnectionStore();
const capStore = useCapabilityStore();

const navItems = computed(() => [
  { name: "connections", icon: Server, label: t("nav.connections"), path: "/" },
  { name: "browser", icon: Database, label: t("nav.browser"), path: "/browser" },
  { name: "pipeline", icon: Layers, label: t("nav.pipeline"), path: "/pipeline" },
  { name: "sandbox", icon: FlaskConical, label: t("nav.sandbox"), path: "/sandbox" },
  { name: "pubsub", icon: Radio, label: t("nav.pubsub"), path: "/pubsub" },
  { name: "streams", icon: ListTree, label: t("nav.streams"), path: "/streams" },
  { name: "search", icon: Radar, label: t("nav.search"), path: "/search" },
  { name: "monitor", icon: Gauge, label: t("nav.monitor"), path: "/monitor" },
  { name: "history", icon: History, label: t("nav.history"), path: "/history" },
]);

function isActive(path: string) {
  return route.path === path;
}

/** Warning hint for nav features the current server does not support. */
function unsupportedHint(name: string): string {
  const cap = capStore.activeCapability;
  if (connStore.activeConnection?.status !== "connected" || !cap) return "";
  if (name === "streams" && cap.streamsSupported === false) {
    return t("nav.streamsUnsupported", { version: cap.redisVersion });
  }
  if (name === "search" && cap.searchSupported === false) {
    return t("nav.searchUnsupported");
  }
  return "";
}

function navTitle(item: { name: string; label: string }): string {
  const hint = unsupportedHint(item.name);
  return hint ? `${item.label} · ${hint}` : item.label;
}

function navigate(path: string) {
  router.push(path);
}

async function handleSidebarConnect(id: string) {
  const conn = connStore.connections.find((c) => c.id === id);
  if (!conn || conn.status === "connecting" || conn.status === "connected") return;
  const ok = await connStore.connect(id);
  if (!ok) {
    toast.error(connStore.lastError || t("connection.connectFailed"), 5000, conn.name);
  }
}

function handleConnectionClick(conn: RedisConnection) {
  // Ignore click that immediately follows a drag
  if (_dragJustFinished) {
    _dragJustFinished = false;
    return;
  }
  _dragJustFinished = false;
  if (conn.status === "connected") {
    if (connStore.activeConnectionId !== conn.id) {
      connStore.activeConnectionId = conn.id;
    }
  } else {
    handleSidebarConnect(conn.id);
  }
}

const disconnectingIds = reactive<Record<string, boolean>>({});

// ── Drag-and-drop state for session reordering (pointer-based for Tauri compat) ──
const dragFromId = ref<string | null>(null);
const dragOverId = ref<string | null>(null);
const isDragging = ref(false);
/** Whether the drop target should show the insertion line above (true) or below (false) */
const dropInsertBefore = ref(true);
let _pointerStartX = 0;
let _pointerStartY = 0;
const DRAG_THRESHOLD = 5; // px before drag activates
let _dragArmed = false;
let _dragJustFinished = false;

function onSessionPointerDown(e: PointerEvent, id: string) {
  if (e.button !== 0) return;
  _pointerStartX = e.clientX;
  _pointerStartY = e.clientY;
  dragFromId.value = id;
  _dragArmed = false;
  isDragging.value = false;
  document.addEventListener("pointermove", onPointerMove);
  document.addEventListener("pointerup", onPointerUp);
}

function onPointerMove(e: PointerEvent) {
  const dx = e.clientX - _pointerStartX;
  const dy = e.clientY - _pointerStartY;
  if (!_dragArmed && Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return;
  _dragArmed = true;
  isDragging.value = true;
  const el = document.elementFromPoint(e.clientX, e.clientY);
  if (!el) return;
  const item = el.closest("[data-session-id]") as HTMLElement | null;
  if (item) {
    const id = item.getAttribute("data-session-id");
    dragOverId.value = id;
    // Determine insertion position based on pointer Y relative to element midpoint
    const rect = item.getBoundingClientRect();
    dropInsertBefore.value = (e.clientY - rect.top) < rect.height / 2;
  } else {
    dragOverId.value = null;
  }
}

function onPointerUp() {
  document.removeEventListener("pointermove", onPointerMove);
  document.removeEventListener("pointerup", onPointerUp);
  if (isDragging.value && dragFromId.value && dragOverId.value && dragFromId.value !== dragOverId.value) {
    connStore.reorderSessions(dragFromId.value, dragOverId.value);
  }
  if (isDragging.value) {
    _dragJustFinished = true;
  }
  dragFromId.value = null;
  dragOverId.value = null;
  isDragging.value = false;
  _dragArmed = false;
}

async function handleSidebarDisconnect(id: string) {
  if (disconnectingIds[id]) return;
  disconnectingIds[id] = true;
  try {
    await connStore.disconnect(id);
  } finally {
    delete disconnectingIds[id];
  }
}
</script>

<template>
  <aside
    class="h-full bg-bg-sidebar border-r border-border flex flex-col shrink-0 overflow-hidden"
    :class="sidebarCollapsed ? 'w-14' : 'w-56'"
    style="transition: width 0.2s ease"
  >
    <!-- Brand -->
    <div class="h-16 flex items-center px-4 shrink-0">
      <img src="/breezeresp.svg" alt="BreezeRESP" class="w-8 h-8 rounded-lg shrink-0" />
      <div
        class="overflow-hidden whitespace-nowrap"
        :class="sidebarCollapsed ? 'w-0 opacity-0 ml-0' : 'w-auto opacity-100 ml-2.5'"
        style="transition: all 0.2s ease"
      >
        <h1 class="text-sm font-semibold text-text-primary leading-none">{{ t("app.title") }}</h1>
        <p class="text-[10px] text-text-muted mt-0.5">{{ t("app.subtitle") }}</p>
      </div>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 px-2 py-2 space-y-0.5 shrink-0">
      <button
        v-for="item in navItems"
        :key="item.name"
        @click="navigate(item.path)"
        class="w-full flex items-center rounded-lg text-sm font-medium transition-colors duration-150 border overflow-hidden"
        :class="[
          'px-3 py-2',
          isActive(item.path)
            ? 'bg-bg-secondary text-redis shadow-sm border-border-light'
            : 'text-text-secondary border-transparent hover:bg-bg-hover hover:text-text-primary'
        ]"
        :title="sidebarCollapsed ? navTitle(item) : undefined"
      >
        <component :is="item.icon" :size="16" :stroke-width="2" class="shrink-0" />
        <span
          class="overflow-hidden whitespace-nowrap"
          :class="sidebarCollapsed ? 'w-0 opacity-0 ml-0' : 'w-auto opacity-100 ml-2.5'"
          style="transition: all 0.2s ease"
        >{{ item.label }}</span>
        <TriangleAlert
          v-if="unsupportedHint(item.name)"
          :size="12"
          class="text-warning shrink-0 ml-auto"
          :class="sidebarCollapsed ? 'w-0 opacity-0' : 'opacity-100'"
          style="transition: opacity 0.2s ease"
          :title="unsupportedHint(item.name)"
        />
      </button>
    </nav>

    <!-- Connection List -->
    <div class="px-2 pb-2 shrink-0">
      <!-- Section header -->
      <div class="flex items-center justify-between h-7 overflow-hidden">
        <span
          class="text-[11px] font-semibold text-text-muted uppercase tracking-wider whitespace-nowrap overflow-hidden"
          :class="sidebarCollapsed ? 'w-0 opacity-0' : 'w-auto opacity-100'"
          style="transition: all 0.2s ease"
        >
          {{ t("connection.sessions") }}
        </span>
        <button
          @click="navigate('/')"
          class="w-5 h-5 rounded flex items-center justify-center hover:bg-bg-hover transition-colors shrink-0"
          :class="sidebarCollapsed ? 'opacity-0 w-0' : 'opacity-100'"
          style="transition: all 0.2s ease"
        >
          <Plus :size="12" class="text-text-muted" />
        </button>
      </div>

      <!-- Connection items -->
      <div v-if="connStore.statusBarConnections.length === 0" class="py-3 text-center text-text-muted text-xs overflow-hidden"
        :class="sidebarCollapsed ? 'h-0 opacity-0 py-0' : 'h-auto opacity-100'"
        style="transition: all 0.2s ease"
      >
        {{ t("connection.noSessions") }}
      </div>
      <div v-else class="space-y-0.5 max-h-48 overflow-y-auto">
        <div
          v-for="conn in connStore.statusBarConnections"
          :key="conn.id"
          :data-session-id="conn.id"
          @pointerdown="onSessionPointerDown($event, conn.id)"
          class="session-item relative flex items-center gap-2 px-3 py-1.5 rounded-md text-xs cursor-pointer hover:bg-bg-hover overflow-hidden select-none"
          :class="[
            connStore.activeConnectionId === conn.id ? 'session-item--active' : '',
            dragFromId === conn.id && isDragging ? 'session-item--dragging' : '',
            dragOverId === conn.id && dragFromId !== conn.id ? 'session-item--target' : ''
          ]"
          :style="{
            transition: isDragging ? 'none' : 'background-color 0.15s ease, box-shadow 0.15s ease',
            ...(connStore.activeConnectionId === conn.id ? { boxShadow: `inset 2.5px 0 0 ${getDotColor(conn.id)}` } : {})
          }"
          @click="handleConnectionClick(conn)"
        >
          <!-- Drop indicator line -->
          <span
            v-if="dragOverId === conn.id && dragFromId !== conn.id"
            class="absolute left-1 right-1 h-0.5 rounded-full bg-redis z-10 pointer-events-none"
            :class="dropInsertBefore ? '-top-px' : '-bottom-px'"
          />
          <span
            class="w-2 h-2 rounded-full shrink-0 transition-all duration-300"
            :class="{
              'bg-text-muted': conn.status === 'disconnected',
              'bg-warning animate-dot-pulse': conn.status === 'connecting',
              'bg-danger': conn.status === 'error',
            }"
            :style="conn.status === 'connected' ? { backgroundColor: getDotColor(conn.id) } : undefined"
          />
          <span
            class="truncate text-text-secondary whitespace-nowrap overflow-hidden"
            :class="sidebarCollapsed ? 'w-0 opacity-0 ml-0' : 'flex-1 opacity-100 ml-0'"
            style="transition: all 0.2s ease"
            :title="conn.name"
          >{{ conn.name }}</span>
          <!-- DB/Cluster badge for connected connections -->
          <span
            :class="[
              conn.status === 'connected' ? 'opacity-100' : 'opacity-0 pointer-events-none',
              sidebarCollapsed ? 'w-0 opacity-0' : 'opacity-100'
            ]"
            class="text-[10px] font-mono font-semibold text-redis/70 bg-redis/8 px-1.5 py-0.5 rounded shrink-0 transition-opacity overflow-hidden whitespace-nowrap"
            style="transition: all 0.2s ease"
          >{{ conn.cluster ? 'Cluster' : `DB${connStore.getActiveDb(conn.id)}` }}</span>
          <button
            v-if="conn.status === 'connected' || conn.status === 'connecting'"
            @click.stop="handleSidebarDisconnect(conn.id)"
            class="w-5 h-5 rounded flex items-center justify-center hover:bg-danger/10 transition-opacity shrink-0 group/disconnect overflow-hidden"
            :class="sidebarCollapsed ? 'w-0 opacity-0' : 'w-5 opacity-100'"
            style="transition: all 0.2s ease"
            :title="t('connection.disconnect')"
          >
            <Unplug :size="12" class="text-text-muted group-hover/disconnect:text-danger" />
          </button>
          <template v-else>
            <button
              @click.stop="connStore.togglePin(conn.id)"
              class="w-5 h-5 rounded flex items-center justify-center hover:bg-danger/10 transition-opacity shrink-0 group/pin overflow-hidden"
              :class="sidebarCollapsed ? 'w-0 opacity-0' : 'w-5 opacity-100'"
              style="transition: all 0.2s ease"
              :title="conn.pinned ? t('connection.unpin') : t('connection.pin')"
            >
              <PinOff v-if="conn.pinned" :size="12" class="text-text-muted group-hover/pin:text-danger" />
              <Pin v-else :size="12" class="text-text-muted group-hover/pin:text-danger" />
            </button>
            <button
              @click.stop="connStore.dismissSession(conn.id)"
              class="w-5 h-5 rounded flex items-center justify-center hover:bg-danger/10 transition-opacity shrink-0 group/dismiss overflow-hidden"
              :class="sidebarCollapsed ? 'w-0 opacity-0' : 'w-5 opacity-100'"
              style="transition: all 0.2s ease"
              :title="t('connection.dismissSession')"
            >
              <X :size="12" class="text-text-muted group-hover/dismiss:text-danger" />
            </button>
          </template>
        </div>
      </div>
    </div>

    <!-- Collapse toggle -->
    <div class="px-2 pb-3 shrink-0">
      <button
        @click="toggleSidebar"
        class="w-full flex items-center justify-center rounded-lg text-text-muted hover:bg-bg-hover hover:text-text-secondary transition-colors overflow-hidden"
        :class="sidebarCollapsed ? 'px-2 py-2' : 'px-3 py-1.5'"
        :title="sidebarCollapsed ? t('nav.expand') : t('nav.collapse')"
      >
        <PanelLeftOpen v-if="sidebarCollapsed" :size="15" class="shrink-0" />
        <template v-else>
          <PanelLeftClose :size="15" class="shrink-0" />
          <span
            class="text-[11px] overflow-hidden whitespace-nowrap ml-2"
            :class="sidebarCollapsed ? 'w-0 opacity-0 ml-0' : 'w-auto opacity-100'"
            style="transition: all 0.2s ease"
          >{{ t("nav.collapse") }}</span>
        </template>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.session-item--active {
  background-color: var(--color-bg-active);
}
.session-item--active .truncate {
  color: var(--color-text-primary);
  font-weight: 600;
}

.session-item--dragging {
  opacity: 0.35;
  outline: 1.5px dashed var(--color-redis);
  outline-offset: -1.5px;
}

.session-item--target {
  background-color: color-mix(in srgb, var(--color-redis) 8%, transparent);
}
</style>
