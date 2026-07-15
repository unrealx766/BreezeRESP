<script setup lang="ts">
import { ref } from "vue";
import { AlertTriangle } from "lucide-vue-next";

const visible = ref(false);
const title = ref("");
const message = ref("");
const confirmLabel = ref("");
const cancelLabel = ref("");
const danger = ref(false);

let resolvePromise: ((value: boolean) => void) | null = null;

function open(options: {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}): Promise<boolean> {
  title.value = options.title;
  message.value = options.message;
  confirmLabel.value = options.confirmLabel || "Confirm";
  cancelLabel.value = options.cancelLabel || "Cancel";
  danger.value = options.danger ?? false;
  visible.value = true;

  return new Promise<boolean>((resolve) => {
    resolvePromise = resolve;
  });
}

function onConfirm() {
  visible.value = false;
  resolvePromise?.(true);
  resolvePromise = null;
}

function onCancel() {
  visible.value = false;
  resolvePromise?.(false);
  resolvePromise = null;
}

defineExpose({ open });
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="visible" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/40" @click="onCancel" />

        <!-- Dialog -->
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[400px] max-w-[90vw] p-5 animate-in">
          <!-- Icon + Title -->
          <div class="flex items-center gap-3 mb-3">
            <div
              class="w-9 h-9 rounded-full flex items-center justify-center shrink-0"
              :class="danger ? 'bg-danger/10' : 'bg-warning/10'"
            >
              <AlertTriangle :size="18" :class="danger ? 'text-danger' : 'text-warning'" />
            </div>
            <h3 class="text-base font-semibold text-text-primary">{{ title }}</h3>
          </div>

          <!-- Message -->
          <p class="text-sm text-text-muted leading-relaxed break-all">{{ message }}</p>

          <!-- Actions -->
          <div class="flex justify-end gap-2.5 mt-6">
            <button
              @click="onCancel"
              class="px-4 py-2 text-sm font-medium text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors"
            >
              {{ cancelLabel }}
            </button>
            <button
              @click="onConfirm"
              class="px-4 py-2 text-sm font-medium text-white rounded-lg transition-colors shadow-sm"
              :class="danger ? 'bg-danger hover:bg-danger/90' : 'bg-redis hover:bg-redis-dark'"
            >
              {{ confirmLabel }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
.animate-in {
  animation: dialog-in 0.2s ease-out;
}
@keyframes dialog-in {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(8px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
