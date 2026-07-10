<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { ref, computed } from "vue";
import { useConnectionStore } from "@/stores/connectionStore";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { availableLocales } from "@/i18n";
import { Globe, Database, ChevronDown, Check, RefreshCw } from "lucide-vue-next";

const { t, locale } = useI18n();
const connStore = useConnectionStore();
const cascade = useCascadeStore();
const detail = useDetailStore();

function toggleLocale() {
  const codes = availableLocales.map((l) => l.code);
  const idx = codes.indexOf(locale.value);
  locale.value = codes[(idx + 1) % codes.length];
}

// DB switching
const switchingDb = ref(false);
const showDbDropdown = ref(false);

const activeConnDb = computed(() => connStore.activeConnection?.db ?? 0);

async function handleDbSwitch(db: number) {
  if (switchingDb.value) return;
  switchingDb.value = true;
  showDbDropdown.value = false;
  try {
    await connStore.switchDb(db);
    cascade.selectedKey = null;
    cascade.searchQuery = "";
    cascade.debouncedSearchQuery = "";
    cascade.typeFilter = "all";
    detail.clearDetail();
    await cascade.refreshKeys(true);
  } catch (e) {
    console.error("Header DB switch failed:", e);
  } finally {
    switchingDb.value = false;
  }
}
</script>

<template>
  <header class="h-12 bg-white border-b border-border flex items-center justify-between px-4 shrink-0 overflow-visible">
    <!-- Left: Connection info -->
    <div class="flex items-center gap-3 min-w-0">
      <template v-if="connStore.activeConnection">
        <span
          class="w-2 h-2 rounded-full shrink-0"
          :class="{
            'bg-success': connStore.activeConnection.status === 'connected',
            'bg-text-muted': connStore.activeConnection.status === 'disconnected',
            'bg-warning animate-pulse': connStore.activeConnection.status === 'connecting',
            'bg-danger': connStore.activeConnection.status === 'error',
          }"
        />
        <span class="text-sm font-medium text-text-primary truncate max-w-[40%]" :title="connStore.activeConnection.name">{{ connStore.activeConnection.name }}</span>
        <span class="text-xs text-text-muted whitespace-nowrap shrink-0">{{ connStore.activeConnection.host }}:{{ connStore.activeConnection.port }}</span>
        <!-- DB switcher -->
        <div v-if="connStore.activeConnection.status === 'connected'" class="relative shrink-0">
          <button
            @click="showDbDropdown = !showDbDropdown"
            :disabled="switchingDb"
            class="inline-flex items-center gap-1 px-2 py-1 text-[11px] font-mono font-semibold text-redis bg-redis/5 border border-redis/20 rounded-lg hover:border-redis/40 focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 transition-colors disabled:opacity-50"
          >
            <Database :size="11" class="shrink-0" />
            <span>DB{{ activeConnDb }}</span>
            <RefreshCw v-if="switchingDb" :size="10" class="animate-spin text-redis/60" />
            <ChevronDown v-else :size="11" class="text-redis/50 transition-transform" :class="showDbDropdown ? 'rotate-180' : ''" />
          </button>
          <!-- Backdrop -->
          <div v-if="showDbDropdown" class="fixed inset-0 z-40" @click="showDbDropdown = false" />
          <!-- Dropdown -->
          <div
            v-if="showDbDropdown"
            class="absolute top-full left-0 mt-1 w-28 bg-white border border-border rounded-lg shadow-lg py-1 z-50 max-h-52 overflow-y-auto"
          >
            <div class="px-2.5 py-1 border-b border-border-light mb-0.5">
              <span class="text-[9px] font-semibold text-text-muted uppercase tracking-wider">Database</span>
            </div>
            <button
              v-for="n in 16"
              :key="n - 1"
              @click="handleDbSwitch(n - 1)"
              class="w-full flex items-center justify-between px-2.5 py-1.5 text-xs font-mono transition-colors"
              :class="activeConnDb === n - 1
                ? 'text-redis font-semibold bg-redis/5'
                : 'text-text-secondary font-medium hover:bg-bg-hover hover:text-text-primary'"
            >
              <span>DB{{ n - 1 }}</span>
              <Check v-if="activeConnDb === n - 1" :size="11" class="text-redis" />
            </button>
          </div>
        </div>
      </template>
      <template v-else>
        <span class="text-sm text-text-muted whitespace-nowrap">{{ t("status.noConnection") }}</span>
      </template>
    </div>

    <!-- Right: Language -->
    <div class="flex items-center gap-2">
      <button
        @click="toggleLocale"
        class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs text-text-secondary hover:bg-bg-hover transition-colors"
      >
        <Globe :size="14" />
        <span>{{ availableLocales.find((l) => l.code === locale)?.label }}</span>
      </button>
    </div>
  </header>
</template>
