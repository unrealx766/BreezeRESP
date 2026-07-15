<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, nextTick } from "vue";

const props = withDefaults(defineProps<{
  modelValue: string;
  placeholder?: string;
  rows?: number;
}>(), {
  rows: 5,
});

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const LINE_HEIGHT = 26; // 1.625rem = 26px

const textareaRef = ref<HTMLTextAreaElement>();
const gutterRef = ref<HTMLDivElement>();
const containerRef = ref<HTMLDivElement>();
const measureRef = ref<HTMLSpanElement>();
const textareaWidth = ref(0);
const charWidth = ref(8.4);

let resizeObserver: ResizeObserver | null = null;

function measure() {
  // Measure textarea content width
  if (textareaRef.value) {
    const style = getComputedStyle(textareaRef.value);
    const paddingLeft = parseFloat(style.paddingLeft);
    const paddingRight = parseFloat(style.paddingRight);
    textareaWidth.value = textareaRef.value.clientWidth - paddingLeft - paddingRight;
    // Measure character width
    if (measureRef.value) {
      charWidth.value = measureRef.value.getBoundingClientRect().width || 8.4;
    }
  }
}

onMounted(() => {
  nextTick(measure);
  if (textareaRef.value) {
    resizeObserver = new ResizeObserver(() => measure());
    resizeObserver.observe(textareaRef.value);
  }
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
});

const contentLines = computed(() => {
  const val = props.modelValue;
  if (!val) return [""];
  return val.split("\n");
});

/** How many visual lines does a content line occupy? */
function visualLineCount(text: string): number {
  if (!text) return 1;
  const cw = charWidth.value;
  if (cw <= 0 || textareaWidth.value <= 0) return 1;
  const textWidth = text.length * cw;
  return Math.max(1, Math.ceil(textWidth / textareaWidth.value));
}

/** Array of { showNumber: boolean, number: number } for each visual row */
const gutterRows = computed(() => {
  const result: { showNumber: boolean; number: number }[] = [];
  const lines = contentLines.value;
  for (let i = 0; i < lines.length; i++) {
    const vLines = visualLineCount(lines[i]);
    for (let j = 0; j < vLines; j++) {
      result.push({ showNumber: j === 0, number: i + 1 });
    }
  }
  // Ensure minimum rows
  while (result.length < props.rows) {
    result.push({ showNumber: false, number: 0 });
  }
  return result;
});

const gutterWidth = computed(() => {
  const maxNum = contentLines.value.length;
  if (maxNum > 999) return "3.2rem";
  if (maxNum > 99) return "2.8rem";
  if (maxNum > 9) return "2.2rem";
  return "1.8rem";
});

function onInput(e: Event) {
  const target = e.target as HTMLTextAreaElement;
  emit("update:modelValue", target.value);
}

function syncScroll() {
  if (textareaRef.value && gutterRef.value) {
    gutterRef.value.scrollTop = textareaRef.value.scrollTop;
  }
}
</script>

<template>
  <div ref="containerRef" class="relative flex rounded-xl border border-border bg-bg-primary overflow-hidden focus-within:border-redis focus-within:ring-2 focus-within:ring-redis/20 transition-all">
    <!-- Hidden char width measurer -->
    <span
      ref="measureRef"
      class="absolute invisible whitespace-pre font-mono text-sm"
      style="padding: 0; border: 0;"
    >M</span>
    <!-- Line number gutter -->
    <div
      ref="gutterRef"
      class="shrink-0 overflow-hidden select-none border-r border-border bg-bg-secondary/50"
      :style="{ width: gutterWidth }"
    >
      <div class="py-2.5">
        <div
          v-for="(row, idx) in gutterRows"
          :key="idx"
          class="font-mono text-right pr-2"
          :style="{ height: `${LINE_HEIGHT}px`, lineHeight: `${LINE_HEIGHT}px`, fontSize: '11px' }"
        >
          <span v-if="row.showNumber" class="text-text-muted/50">{{ row.number }}</span>
        </div>
      </div>
    </div>
    <!-- Textarea -->
    <textarea
      ref="textareaRef"
      :value="modelValue"
      @input="onInput"
      @scroll="syncScroll"
      :placeholder="placeholder"
      :rows="rows"
      class="flex-1 min-w-0 px-3 py-2.5 text-sm font-mono bg-transparent border-0 resize-none focus:outline-none placeholder:text-text-muted/40"
      :style="{ lineHeight: `${LINE_HEIGHT}px` }"
    />
  </div>
</template>
