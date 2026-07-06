<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { availableLocales } from "@/i18n";
import { Search, Globe } from "lucide-vue-next";

const { t, locale } = useI18n();
const connStore = useConnectionStore();

function toggleLocale() {
  const codes = availableLocales.map((l) => l.code);
  const idx = codes.indexOf(locale.value);
  locale.value = codes[(idx + 1) % codes.length];
}
</script>

<template>
  <header class="h-12 bg-white border-b border-border flex items-center justify-between px-4 shrink-0">
    <!-- Left: Connection info -->
    <div class="flex items-center gap-3">
      <template v-if="connStore.activeConnection">
        <span class="w-2 h-2 rounded-full bg-success" />
        <span class="text-sm font-medium text-text-primary">{{ connStore.activeConnection.name }}</span>
        <span class="text-xs text-text-muted">{{ connStore.activeConnection.host }}:{{ connStore.activeConnection.port }}</span>
        <span class="badge bg-redis-light text-redis text-[10px]">DB{{ connStore.activeConnection.db }}</span>
      </template>
      <template v-else>
        <span class="text-sm text-text-muted">{{ t("status.noConnection") }}</span>
      </template>
    </div>

    <!-- Right: Search + Language -->
    <div class="flex items-center gap-2">
      <div class="relative">
        <Search :size="14" class="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted" />
        <input
          type="text"
          :placeholder="t('browser.search')"
          class="pl-8 pr-3 py-1.5 text-xs bg-bg-primary border border-border rounded-lg w-52 focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 transition-all"
        />
      </div>
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
