<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { availableLocales } from "@/i18n";
import { Globe } from "lucide-vue-next";

const { t, locale } = useI18n();
const connStore = useConnectionStore();

function toggleLocale() {
  const codes = availableLocales.map((l) => l.code);
  const idx = codes.indexOf(locale.value);
  locale.value = codes[(idx + 1) % codes.length];
}
</script>

<template>
  <header class="h-12 bg-white border-b border-border flex items-center justify-between px-4 shrink-0 overflow-hidden">
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
        <span class="text-sm font-medium text-text-primary truncate max-w-[180px]" :title="connStore.activeConnection.name">{{ connStore.activeConnection.name }}</span>
        <span class="text-xs text-text-muted whitespace-nowrap hidden sm:block">{{ connStore.activeConnection.host }}:{{ connStore.activeConnection.port }}</span>
        <span class="badge bg-redis-light text-redis text-[10px] shrink-0">DB{{ connStore.activeConnection.db }}</span>
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
