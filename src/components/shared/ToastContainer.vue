<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { CheckCircle, XCircle, AlertTriangle, Info, X } from "lucide-vue-next";
import { toast } from "@/utils/toast";

interface ToastItem {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
}

const toasts = ref<ToastItem[]>([]);

const iconMap = {
  success: CheckCircle,
  error: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const colorMap = {
  success: "text-success bg-success/10 border-success/20",
  error: "text-danger bg-danger/10 border-danger/20",
  warning: "text-warning bg-warning/10 border-warning/20",
  info: "text-info bg-info/10 border-info/20",
};

function dismiss(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

let unsubscribe: (() => void) | undefined;

onMounted(() => {
  unsubscribe = toast.on((event) => {
    toasts.value.push({ id: event.id, type: event.type, message: event.message });
    if (event.duration > 0) {
      setTimeout(() => dismiss(event.id), event.duration);
    }
  });
});

onUnmounted(() => {
  unsubscribe?.();
});
</script>

<template>
  <Teleport to="body">
    <div class="fixed top-4 right-4 z-[10001] flex flex-col gap-2 pointer-events-none">
      <TransitionGroup name="toast">
        <div
          v-for="item in toasts"
          :key="item.id"
          class="pointer-events-auto flex items-start gap-2.5 px-4 py-3 rounded-xl border shadow-lg backdrop-blur-sm max-w-sm"
          :class="colorMap[item.type]"
        >
          <component :is="iconMap[item.type]" :size="16" class="shrink-0 mt-0.5" />
          <p class="flex-1 text-sm leading-relaxed break-all">{{ item.message }}</p>
          <button
            @click="dismiss(item.id)"
            class="shrink-0 opacity-60 hover:opacity-100 transition-opacity mt-0.5"
          >
            <X :size="14" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active {
  animation: toast-in 0.25s ease-out;
}
.toast-leave-active {
  animation: toast-out 0.2s ease-in forwards;
}
.toast-move {
  transition: transform 0.2s ease;
}
@keyframes toast-in {
  from {
    opacity: 0;
    transform: translateX(40px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
}
@keyframes toast-out {
  from {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
  to {
    opacity: 0;
    transform: translateX(40px) scale(0.95);
  }
}
</style>
