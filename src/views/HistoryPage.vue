<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { History } from "lucide-vue-next";

const { t } = useI18n();
</script>

<template>
  <div class="h-full flex flex-col items-center justify-center text-text-muted">
    <History :size="48" :stroke-width="1.5" class="mb-4 opacity-30" />
    <h2 class="text-lg font-semibold text-text-secondary mb-1">{{ t("history.title") }}</h2>
    <p class="text-sm">{{ t("history.comingSoon") }}</p>
  </div>
</template>
