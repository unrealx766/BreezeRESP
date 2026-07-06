<script setup lang="ts">
import { computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { Database, Server, Layers, FlaskConical, Plus } from "lucide-vue-next";

const router = useRouter();
const route = useRoute();
const { t } = useI18n();
const connStore = useConnectionStore();

const navItems = computed(() => [
  { name: "connections", icon: Server, label: t("nav.connections"), path: "/" },
  { name: "browser", icon: Database, label: t("nav.browser"), path: "/browser" },
  { name: "pipeline", icon: Layers, label: t("nav.pipeline"), path: "/pipeline" },
  { name: "sandbox", icon: FlaskConical, label: t("nav.sandbox"), path: "/sandbox" },
]);

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
      <div class="w-8 h-8 rounded-lg bg-redis flex items-center justify-center">
        <span class="text-white font-bold text-sm">R</span>
      </div>
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
        class="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150"
        :class="isActive(item.path)
          ? 'bg-white text-redis shadow-sm border border-border-light'
          : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'"
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
          @click="connStore.connect(conn.id).then(() => navigate('/browser'))"
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
          <span class="truncate text-text-secondary">{{ conn.name }}</span>
        </div>
      </div>
    </div>
  </aside>
</template>
