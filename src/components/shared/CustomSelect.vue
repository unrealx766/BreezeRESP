<script setup lang="ts">
// Reusable dropdown select matching the type-filter style from BrowserPage.
import { ref, computed, onBeforeUnmount } from "vue";
import { ChevronDown, Check } from "lucide-vue-next";

export interface SelectOption {
  value: string | number;
  label: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string | number;
    options: SelectOption[];
    label?: string;
    mono?: boolean;
    zIndex?: number;
  }>(),
  { label: "", mono: false, zIndex: 5000 }
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string | number): void;
}>();

const open = ref(false);
const triggerRef = ref<HTMLElement | null>(null);
const dropdownRef = ref<HTMLElement | null>(null);

const selectedLabel = computed(() => {
  const opt = props.options.find((o) => o.value === props.modelValue);
  return opt?.label ?? "";
});

const dropdownStyle = computed(() => {
  if (!triggerRef.value) return {};
  const rect = triggerRef.value.getBoundingClientRect();
  return {
    top: rect.bottom + 4 + "px",
    left: rect.left + "px",
    minWidth: rect.width + "px",
    zIndex: props.zIndex,
  };
});

function toggle() {
  open.value = !open.value;
}

function select(val: string | number) {
  emit("update:modelValue", val);
  open.value = false;
}

function close() {
  open.value = false;
}

// Close on outside click (fallback for backdrop)
function onDocClick(e: MouseEvent) {
  if (!open.value) return;
  const target = e.target as Node;
  if (triggerRef.value?.contains(target) || dropdownRef.value?.contains(target)) return;
  close();
}

if (typeof document !== "undefined") {
  document.addEventListener("click", onDocClick, true);
  onBeforeUnmount(() => document.removeEventListener("click", onDocClick, true));
}
</script>

<template>
  <div class="relative inline-block">
    <button
      ref="triggerRef"
      type="button"
      @click="toggle"
      class="px-3 py-1 text-[11px] font-semibold transition-colors inline-flex items-center gap-1 border border-border rounded-lg hover:border-border-light focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20"
      :class="[
        mono ? 'font-mono' : '',
        open ? 'border-redis/50' : 'text-text-secondary bg-bg-primary',
      ]"
    >
      <span class="truncate max-w-[180px]">{{ selectedLabel }}</span>
      <ChevronDown :size="11" class="shrink-0 text-redis/50 transition-transform" :class="open ? 'rotate-180' : ''" />
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        ref="dropdownRef"
        class="fixed bg-bg-secondary border border-border rounded-lg shadow-lg py-1 min-w-[160px]"
        :style="dropdownStyle"
      >
        <div v-if="label" class="px-2.5 py-1 border-b border-border-light mb-0.5">
          <span class="text-[9px] font-semibold text-text-muted uppercase tracking-wider">{{ label }}</span>
        </div>
        <button
          v-for="opt in options"
          :key="opt.value"
          @click="select(opt.value)"
          class="w-full flex items-center justify-between px-2.5 py-1.5 text-xs transition-colors"
          :class="[
            mono ? 'font-mono' : '',
            modelValue === opt.value
              ? 'text-redis font-semibold bg-redis/5'
              : 'text-text-secondary font-medium hover:bg-bg-hover hover:text-text-primary',
          ]"
        >
          <span>{{ opt.label }}</span>
          <Check v-if="modelValue === opt.value" :size="11" class="text-redis" />
        </button>
      </div>
    </Teleport>
  </div>
</template>
