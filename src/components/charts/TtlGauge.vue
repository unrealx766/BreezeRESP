<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  ttlRemaining: number;
  ttlTotal: number;
}>();

const radius = 48;
const stroke = 8;
const circumference = 2 * Math.PI * radius;

const percent = computed(() => {
  if (props.ttlTotal <= 0) return -1; // no expiry
  return Math.max(0, (props.ttlRemaining / props.ttlTotal) * 100);
});

const dashOffset = computed(() => {
  if (percent.value < 0) return 0;
  return circumference - (percent.value / 100) * circumference;
});

const color = computed(() => {
  if (percent.value < 0) return "#8b92ad"; // muted
  if (percent.value > 60) return "#10b981"; // green
  if (percent.value > 30) return "#f59e0b"; // yellow
  return "#ef4444"; // red
});

const bgColor = computed(() => {
  if (percent.value < 0) return "#e2e6ef";
  if (percent.value > 60) return "#d1fae5";
  if (percent.value > 30) return "#fef3c7";
  return "#fecaca";
});

const centerText = computed(() => {
  if (percent.value < 0) return "∞";
  const s = props.ttlRemaining;
  if (s <= 0) return "0s";
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
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
    <div class="absolute inset-0 flex items-center justify-center">
      <span class="text-lg font-semibold" :style="{ color }">{{ centerText }}</span>
    </div>
  </div>
</template>
