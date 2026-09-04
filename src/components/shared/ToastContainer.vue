<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { CheckCircle, XCircle, AlertTriangle, Info, X } from "lucide-vue-next";
import { toast, type ToastAction } from "@/utils/toast";

interface ToastItem {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
  action?: ToastAction;
}

const toasts = ref<ToastItem[]>([]);

const iconMap = {
  success: CheckCircle,
  error: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const colorMap = {
  success: { border: "toast-border-success", icon: "text-success", glow: "toast-glow-success" },
  error:   { border: "toast-border-danger", icon: "text-danger", glow: "toast-glow-danger" },
  warning: { border: "toast-border-warning", icon: "text-warning", glow: "toast-glow-warning" },
  info:    { border: "toast-border-info", icon: "text-info", glow: "toast-glow-info" },
};

function dismiss(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

function runAction(item: ToastItem) {
  item.action?.onClick();
  dismiss(item.id);
}

let unsubscribe: (() => void) | undefined;

onMounted(() => {
  unsubscribe = toast.on((event) => {
    toasts.value.push({ id: event.id, type: event.type, message: event.message, action: event.action });
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
    <div class="fixed top-4 right-4 z-[99999] flex flex-col gap-2 pointer-events-none">
      <TransitionGroup name="toast">
        <div
          v-for="item in toasts"
          :key="item.id"
          class="toast-item pointer-events-auto flex items-start gap-3 px-4 py-3 rounded-xl shadow-lg max-w-sm bg-bg-secondary border border-border"
          :class="[colorMap[item.type].border, colorMap[item.type].glow]"
        >
          <component :is="iconMap[item.type]" :size="18" class="shrink-0 mt-0.5" :class="colorMap[item.type].icon" />
          <div class="flex-1 min-w-0">
            <p class="text-sm leading-relaxed break-all text-text-primary">{{ item.message }}</p>
            <button
              v-if="item.action"
              @click="runAction(item)"
              class="mt-1.5 text-xs font-semibold underline underline-offset-2 transition-opacity hover:opacity-100"
              :class="colorMap[item.type].icon"
            >
              {{ item.action.label }}
            </button>
          </div>
          <button
            @click="dismiss(item.id)"
            class="shrink-0 text-text-muted hover:text-text-primary transition-colors mt-0.5"
          >
            <X :size="14" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-item {
  border-left-width: 3px;
  border-left-style: solid;
}
.toast-border-success { border-left-color: var(--color-success); }
.toast-border-danger  { border-left-color: var(--color-danger); }
.toast-border-warning { border-left-color: var(--color-warning); }
.toast-border-info    { border-left-color: var(--color-info); }

.toast-glow-success { box-shadow: 0 4px 20px -4px rgba(16, 185, 129, 0.18), 0 1px 3px rgba(0,0,0,0.08); }
.toast-glow-danger  { box-shadow: 0 4px 20px -4px rgba(239, 68, 68, 0.18), 0 1px 3px rgba(0,0,0,0.08); }
.toast-glow-warning { box-shadow: 0 4px 20px -4px rgba(245, 158, 11, 0.18), 0 1px 3px rgba(0,0,0,0.08); }
.toast-glow-info    { box-shadow: 0 4px 20px -4px rgba(59, 130, 246, 0.18), 0 1px 3px rgba(0,0,0,0.08); }

:global(.dark) .toast-glow-success { box-shadow: 0 4px 20px -4px rgba(52, 211, 153, 0.15), 0 1px 3px rgba(0,0,0,0.3); }
:global(.dark) .toast-glow-danger  { box-shadow: 0 4px 20px -4px rgba(248, 113, 113, 0.15), 0 1px 3px rgba(0,0,0,0.3); }
:global(.dark) .toast-glow-warning { box-shadow: 0 4px 20px -4px rgba(251, 191, 36, 0.15), 0 1px 3px rgba(0,0,0,0.3); }
:global(.dark) .toast-glow-info    { box-shadow: 0 4px 20px -4px rgba(96, 165, 250, 0.15), 0 1px 3px rgba(0,0,0,0.3); }

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
