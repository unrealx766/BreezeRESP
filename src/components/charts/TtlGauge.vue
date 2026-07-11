<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from "vue";

const props = defineProps<{
  ttlRemaining: number;
  ttlTotal: number;
}>();

const radius = 48;
const stroke = 8;
const circumference = 2 * Math.PI * radius;

const isDark = ref(document.documentElement.classList.contains("dark"));
let observer: MutationObserver | null = null;

onMounted(() => {
  observer = new MutationObserver(() => {
    isDark.value = document.documentElement.classList.contains("dark");
  });
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
});

onBeforeUnmount(() => {
  observer?.disconnect();
});

const percent = computed(() => {
  if (props.ttlTotal <= 0) return -1; // no expiry
  return Math.max(0, (props.ttlRemaining / props.ttlTotal) * 100);
});

const dashOffset = computed(() => {
  if (percent.value < 0) return 0;
  return circumference - (percent.value / 100) * circumference;
});

const color = computed(() => {
  if (percent.value < 0) return isDark.value ? "#6b7194" : "#8b92ad";
  if (percent.value > 60) return isDark.value ? "#34d399" : "#10b981";
  if (percent.value > 30) return isDark.value ? "#fbbf24" : "#f59e0b";
  return isDark.value ? "#f87171" : "#ef4444";
});

const bgColor = computed(() => {
  if (percent.value < 0) return isDark.value ? "#1f2236" : "#e2e6ef";
  if (percent.value > 60) return isDark.value ? "#0d3326" : "#d1fae5";
  if (percent.value > 30) return isDark.value ? "#332b0e" : "#fef3c7";
  return isDark.value ? "#331515" : "#fecaca";
});

const centerText = computed(() => {
  if (percent.value < 0) return "∞";
  const s = props.ttlRemaining;
  if (s <= 0) return "0s";
  if (s < 60) return `${s}s`;
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const parts: string[] = [];
  if (d > 0) parts.push(`${d}d`);
  if (h > 0) parts.push(`${h}h`);
  if (m > 0) parts.push(`${m}m`);
  if (sec > 0) parts.push(`${sec}s`);
  // Keep at most 2 units so text fits inside the circle
  return parts.slice(0, 2).join(" ");
});

const fullText = computed(() => {
  if (percent.value < 0) return "";
  const s = props.ttlRemaining;
  if (s <= 0) return "0s";
  if (s < 60) return `${s}s`;
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const parts: string[] = [];
  if (d > 0) parts.push(`${d}d`);
  if (h > 0) parts.push(`${h}h`);
  if (m > 0) parts.push(`${m}m`);
  if (sec > 0) parts.push(`${sec}s`);
  return parts.join(" ");
});
</script>

<template>
  <div class="relative inline-flex items-center justify-center">
    <svg :width="(radius + stroke) * 2" :height="(radius + stroke) * 2" class="transform -rotate-90">
      <!-- Background circle -->
      <circle
        :cx="radius + stroke"
        :cy="radius + stroke"
        :r="radius"
        fill="none"
        :stroke="bgColor"
        :stroke-width="stroke"
      />
      <!-- Progress circle -->
      <circle
        v-if="percent >= 0"
        :cx="radius + stroke"
        :cy="radius + stroke"
        :r="radius"
        fill="none"
        :stroke="color"
        :stroke-width="stroke"
        stroke-linecap="round"
        :stroke-dasharray="circumference"
        :stroke-dashoffset="dashOffset"
        class="transition-all duration-1000 ease-linear"
      />
    </svg>
    <!-- Center text -->
    <div class="absolute inset-0 flex items-center justify-center overflow-hidden px-3">
      <span class="text-base font-semibold leading-tight text-center truncate block w-full" :title="fullText" :style="{ color }">{{ centerText }}</span>
    </div>
  </div>
</template>
