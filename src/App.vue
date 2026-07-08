<script setup lang="ts">
import { onMounted } from "vue";
import AppSidebar from "@/components/layout/AppSidebar.vue";
import AppHeader from "@/components/layout/AppHeader.vue";
import StatusBar from "@/components/layout/StatusBar.vue";
import ToastContainer from "@/components/shared/ToastContainer.vue";
import { useMetricsStore } from "@/stores/metricsStore";

const metricsStore = useMetricsStore();

onMounted(() => {
  metricsStore.startMonitoring();
});
</script>

<template>
  <div class="h-screen w-screen flex overflow-hidden bg-bg-primary">
    <!-- Sidebar -->
    <AppSidebar />

    <!-- Main Area -->
    <div class="flex-1 flex flex-col min-w-0">
      <!-- Header -->
      <AppHeader />

      <!-- Content -->
      <main class="flex-1 overflow-auto">
        <router-view v-slot="{ Component }">
          <transition name="fade" mode="out-in">
            <component :is="Component" v-if="Component" :key="$route.fullPath" />
          </transition>
        </router-view>
      </main>

      <!-- Status Bar -->
      <StatusBar />
    </div>

    <!-- Global Toast -->
    <ToastContainer ref="toast" />
  </div>
</template>

<style>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
