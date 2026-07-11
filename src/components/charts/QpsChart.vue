<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed } from "vue";
import type { QpsDataPoint } from "@/types";

const props = defineProps<{
  data: QpsDataPoint[];
  height?: number;
}>();

const containerRef = ref<HTMLElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const tooltip = ref<{ x: number; y: number; value: number } | null>(null);
const containerWidth = ref(400);

const h = computed(() => props.height ?? 120);
const padding = { top: 10, right: 10, bottom: 20, left: 40 };

let ro: ResizeObserver | null = null;
let themeObserver: MutationObserver | null = null;

onMounted(() => {
  if (containerRef.value) {
    containerWidth.value = containerRef.value.clientWidth;
    ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        containerWidth.value = entry.contentRect.width;
      }
    });
    ro.observe(containerRef.value);
  }
  // Redraw on theme change
  themeObserver = new MutationObserver(() => draw());
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
  draw();
});

onBeforeUnmount(() => {
  ro?.disconnect();
  themeObserver?.disconnect();
});

const w = computed(() => containerWidth.value);

function getCssColor(varName: string, fallback: string): string {
  const val = getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
  return val || fallback;
}

function draw() {
  const canvas = canvasRef.value;
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const data = props.data;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = w.value * dpr;
  canvas.height = h.value * dpr;
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w.value, h.value);

  if (data.length < 2) return;

  const chartW = w.value - padding.left - padding.right;
  const chartH = h.value - padding.top - padding.bottom;

  const maxVal = Math.max(...data.map((d) => d.value)) * 1.1;
  const minVal = Math.min(...data.map((d) => d.value)) * 0.9;
  const range = maxVal - minVal || 1;

  const toX = (i: number) => padding.left + (i / (data.length - 1)) * chartW;
  const toY = (v: number) => padding.top + chartH - ((v - minVal) / range) * chartH;

  const gridColor = getCssColor("--color-border", "#e2e6ef");
  const labelColor = getCssColor("--color-text-muted", "#8b92ad");
  const redisColor = getCssColor("--color-redis", "#DC382D");

  // Grid lines
  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 0.5;
  for (let i = 0; i <= 4; i++) {
    const y = padding.top + (chartH / 4) * i;
    ctx.beginPath();
    ctx.moveTo(padding.left, y);
    ctx.lineTo(w.value - padding.right, y);
    ctx.stroke();

    // Y labels
    ctx.fillStyle = labelColor;
    ctx.font = "10px Inter, sans-serif";
    ctx.textAlign = "right";
    ctx.fillText(Math.round(maxVal - ((maxVal - minVal) / 4) * i).toString(), padding.left - 6, y + 3);
  }

  // Gradient fill
  const gradient = ctx.createLinearGradient(0, padding.top, 0, h.value - padding.bottom);
  gradient.addColorStop(0, redisColor + "26");
  gradient.addColorStop(1, redisColor + "03");

  ctx.beginPath();
  ctx.moveTo(toX(0), h.value - padding.bottom);
  for (let i = 0; i < data.length; i++) {
    if (i === 0) ctx.lineTo(toX(i), toY(data[i].value));
    else {
      const prevX = toX(i - 1);
      const currX = toX(i);
      const cpX = (prevX + currX) / 2;
      ctx.bezierCurveTo(cpX, toY(data[i - 1].value), cpX, toY(data[i].value), currX, toY(data[i].value));
    }
  }
  ctx.lineTo(toX(data.length - 1), h.value - padding.bottom);
  ctx.closePath();
  ctx.fillStyle = gradient;
  ctx.fill();

  // Line
  ctx.beginPath();
  for (let i = 0; i < data.length; i++) {
    if (i === 0) ctx.moveTo(toX(i), toY(data[i].value));
    else {
      const prevX = toX(i - 1);
      const currX = toX(i);
      const cpX = (prevX + currX) / 2;
      ctx.bezierCurveTo(cpX, toY(data[i - 1].value), cpX, toY(data[i].value), currX, toY(data[i].value));
    }
  }
  ctx.strokeStyle = redisColor;
  ctx.lineWidth = 2;
  ctx.stroke();

  // End dot
  const lastX = toX(data.length - 1);
  const lastY = toY(data[data.length - 1].value);
  ctx.beginPath();
  ctx.arc(lastX, lastY, 3, 0, Math.PI * 2);
  ctx.fillStyle = redisColor;
  ctx.fill();
  ctx.beginPath();
  ctx.arc(lastX, lastY, 6, 0, Math.PI * 2);
  ctx.fillStyle = redisColor + "33";
  ctx.fill();
}

function handleMouseMove(e: MouseEvent) {
  const canvas = canvasRef.value;
  if (!canvas || props.data.length < 2) return;
  const rect = canvas.getBoundingClientRect();
  const mouseX = e.clientX - rect.left;
  const chartW = w.value - padding.left - padding.right;
  const idx = Math.round(((mouseX - padding.left) / chartW) * (props.data.length - 1));
  if (idx >= 0 && idx < props.data.length) {
    const x = padding.left + (idx / (props.data.length - 1)) * chartW;
    const maxVal = Math.max(...props.data.map((d) => d.value)) * 1.1;
    const minVal = Math.min(...props.data.map((d) => d.value)) * 0.9;
    const range = maxVal - minVal || 1;
    const chartH = h.value - padding.top - padding.bottom;
    const y = padding.top + chartH - ((props.data[idx].value - minVal) / range) * chartH;
    tooltip.value = { x, y, value: props.data[idx].value };
  }
}

function handleMouseLeave() {
  tooltip.value = null;
}

watch(() => props.data, draw, { deep: true });
watch([w, h], draw);
</script>

<template>
  <div ref="containerRef" class="relative w-full" :style="{ height: `${h}px` }">
    <canvas
      ref="canvasRef"
      :style="{ width: `${w}px`, height: `${h}px` }"
      @mousemove="handleMouseMove"
      @mouseleave="handleMouseLeave"
    />
    <!-- Tooltip -->
    <div
      v-if="tooltip"
      class="absolute pointer-events-none bg-text-primary text-white text-[10px] px-2 py-1 rounded shadow-lg"
      :style="{ left: `${tooltip.x - 20}px`, top: `${tooltip.y - 24}px` }"
    >
      {{ tooltip.value }} req/s
    </div>
  </div>
</template>
