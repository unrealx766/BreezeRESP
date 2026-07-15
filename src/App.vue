<script setup lang="ts">
import { onMounted } from "vue";
import AppSidebar from "@/components/layout/AppSidebar.vue";
import AppHeader from "@/components/layout/AppHeader.vue";
import StatusBar from "@/components/layout/StatusBar.vue";
import ToastContainer from "@/components/shared/ToastContainer.vue";
import { useMetricsStore } from "@/stores/metricsStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { setConnectionNameGetter, setConnectionDbGetter } from "@/utils/toast";
import { emit } from "@tauri-apps/api/event";

const metricsStore = useMetricsStore();
const connStore = useConnectionStore();

// Register active connection name for toast message history
setConnectionNameGetter(() => connStore.activeConnection?.name);
setConnectionDbGetter(() => connStore.activeConnection?.db);

onMounted(async () => {
  metricsStore.startMonitoring();

  // Show the window — splash is already rendered and visible at this point
  await emit("app-ready").catch(() => {});

  // Brief pause so the user sees the splash before it fades out
  await new Promise((r) => setTimeout(r, 300));

  // Dismiss splash screen with fade-out
  const splash = document.getElementById("splash-screen");
  if (splash) {
    splash.classList.add("splash-hidden");
    splash.addEventListener("transitionend", () => splash.remove());
  }
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
      <main class="flex-1 overflow-auto relative">
        <router-view v-slot="{ Component }">
          <transition name="fade">
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
.fade-leave-active {
  position: absolute;
  inset: 0;
  overflow: auto;
}
</style>
