<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { Database, Server, Layers, FlaskConical, Plus, Unplug, ChevronDown, Check } from "lucide-vue-next";

const router = useRouter();
const route = useRoute();
const { t } = useI18n();
const connStore = useConnectionStore();
const cascade = useCascadeStore();
const detail = useDetailStore();

const switchingDb = ref(false);
const showDbDropdown = ref(false);
let triggerEl: HTMLElement | null = null;
const dbDropdownPos = ref({ top: 0, left: 0 });

function setTriggerRef(el: any) {
  triggerEl = el as HTMLElement | null;
}

function toggleDbDropdown() {
  if (showDbDropdown.value) {
    showDbDropdown.value = false;
    return;
  }
  if (triggerEl) {
    const rect = triggerEl.getBoundingClientRect();
    dbDropdownPos.value = {
      top: rect.top,
      left: rect.left + rect.width / 2,
    };
  }
  showDbDropdown.value = true;
}

function closeDbDropdown() {
  showDbDropdown.value = false;
}

async function handleDbSwitch(connId: string, db: number) {
  if (switchingDb.value) return;
  switchingDb.value = true;
  try {
    await connStore.switchDb(db);
    if (connStore.activeConnectionId === connId) {
      cascade.selectedKey = null;
      cascade.searchQuery = "";
      cascade.typeFilter = "all";
      detail.clearDetail();
      await cascade.refreshKeys(true);
    }
  } catch (e) {
    console.error("Sidebar DB switch failed:", e);
  } finally {
    switchingDb.value = false;
  }
}

const navItems = computed(() => [
  { name: "connections", icon: Server, label: t("nav.connections"), path: "/" },
  { name: "browser", icon: Database, label: t("nav.browser"), path: "/browser" },
  { name: "pipeline", icon: Layers, label: t("nav.pipeline"), path: "/pipeline" },
  { name: "sandbox", icon: FlaskConical, label: t("nav.sandbox"), path: "/sandbox" },
]);

const activeConnDb = computed(() => connStore.activeConnection?.db ?? 0);

const dbDropdownStyle = computed(() => ({
  bottom: `${window.innerHeight - dbDropdownPos.value.top + 6}px`,
  left: `${dbDropdownPos.value.left}px`,
  transform: 'translateX(-50%)',
}));

function isActive(path: string) {
  return route.path === path;
}

function navigate(path: string) {
  router.push(path);
}
</script>

<template>
  <aside class="w-56 h-full bg-bg-sidebar border-r border-border flex flex-col shrink-0">
    <!-- Brand -->
    <div class="px-4 py-4 flex items-center gap-2.5">
      <img src="/breezeresp.svg" alt="BreezeRESP" class="w-8 h-8 rounded-lg" />
      <div>
        <h1 class="text-sm font-semibold text-text-primary leading-none">{{ t("app.title") }}</h1>
        <p class="text-[10px] text-text-muted mt-0.5">{{ t("app.subtitle") }}</p>
      </div>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 px-2 py-2 space-y-0.5">
      <button
        v-for="item in navItems"
        :key="item.name"
        @click="navigate(item.path)"
        class="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm font-medium transition-colors duration-150 border"
        :class="isActive(item.path)
          ? 'bg-white text-redis shadow-sm border-border-light'
          : 'text-text-secondary border-transparent hover:bg-bg-hover hover:text-text-primary'"
      >
        <component :is="item.icon" :size="16" :stroke-width="2" />
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <!-- Connection List -->
    <div class="px-2 pb-2">
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-[11px] font-semibold text-text-muted uppercase tracking-wider">
          {{ t("connection.status") }}
        </span>
        <button
          @click="navigate('/')"
          class="w-5 h-5 rounded flex items-center justify-center hover:bg-bg-hover transition-colors"
        >
          <Plus :size="12" class="text-text-muted" />
        </button>
      </div>
      <div class="space-y-0.5 max-h-48 overflow-y-auto">
        <div
          v-for="conn in connStore.connections"
          :key="conn.id"
          class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs cursor-pointer hover:bg-bg-hover transition-colors"
          :class="connStore.activeConnectionId === conn.id ? 'bg-bg-active' : ''"
          @click="conn.status === 'connected' ? (connStore.activeConnectionId = conn.id) : connStore.connect(conn.id)"
        >
          <span
            class="w-2 h-2 rounded-full shrink-0"
            :class="{
              'bg-success': conn.status === 'connected',
              'bg-text-muted': conn.status === 'disconnected',
              'bg-warning animate-pulse': conn.status === 'connecting',
              'bg-danger': conn.status === 'error',
            }"
          />
          <span class="truncate text-text-secondary flex-1">{{ conn.name }}</span>
          <!-- DB selector for active connected connection -->
          <div
            v-if="conn.status === 'connected' && connStore.activeConnectionId === conn.id"
            class="shrink-0"
          >
            <button
              :ref="setTriggerRef"
              @click.stop="toggleDbDropdown"
              :disabled="switchingDb"
              class="inline-flex items-center gap-0.5 text-[10px] font-mono font-semibold text-redis bg-white border border-border rounded-md px-1.5 py-0.5 hover:border-redis/40 focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 transition-colors disabled:opacity-50"
            >
              <span>DB{{ conn.db }}</span>
              <ChevronDown :size="10" class="text-redis/50 transition-transform" :class="showDbDropdown ? 'rotate-180' : ''" />
            </button>
          </div>
          <!-- DB badge for other connected connections -->
          <span
            v-else-if="conn.status === 'connected'"
            class="text-[10px] font-mono font-semibold text-redis/70 bg-redis/8 px-1.5 py-0.5 rounded shrink-0"
          >DB{{ conn.db }}</span>
          <button
            v-if="conn.status === 'connected' || conn.status === 'connecting'"
            @click.stop="connStore.disconnect(conn.id)"
            class="w-5 h-5 rounded flex items-center justify-center hover:bg-danger/10 transition-colors shrink-0 group/disconnect"
            :title="t('connection.disconnect')"
          >
            <Unplug :size="12" class="text-text-muted group-hover/disconnect:text-danger" />
          </button>
        </div>
      </div>
    </div>
  </aside>

  <!-- Teleported DB dropdown + backdrop -->
  <Teleport to="body">
    <!-- Invisible backdrop to close dropdown on outside click -->
    <div
      v-if="showDbDropdown"
      class="fixed inset-0 z-[9998]"
      @click="closeDbDropdown"
    />
    <div
      v-if="showDbDropdown"
      class="fixed w-28 bg-white border border-border rounded-lg shadow-lg py-1 z-[9999] max-h-48 overflow-y-auto"
      :style="dbDropdownStyle"
    >
      <div class="px-2 py-1 border-b border-border-light mb-0.5">
        <span class="text-[9px] font-semibold text-text-muted uppercase tracking-wider">Database</span>
      </div>
      <button
        v-for="n in 16"
        :key="n - 1"
        @click="handleDbSwitch(connStore.activeConnectionId!, n - 1); closeDbDropdown()"
        class="w-full flex items-center justify-between px-2 py-1 text-[11px] font-mono transition-colors"
        :class="activeConnDb === n - 1
          ? 'text-redis font-semibold bg-redis/5'
          : 'text-text-secondary font-medium hover:bg-bg-hover hover:text-text-primary'"
      >
        <span>DB{{ n - 1 }}</span>
        <Check v-if="activeConnDb === n - 1" :size="10" class="text-redis" />
      </button>
    </div>
  </Teleport>
</template>
