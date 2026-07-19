import { ref, watch } from "vue";

// ── Sidebar collapse ──
const SIDEBAR_KEY = "breezeresp-sidebar-collapsed";
export const sidebarCollapsed = ref(localStorage.getItem(SIDEBAR_KEY) === "true");

watch(sidebarCollapsed, (v) => {
  localStorage.setItem(SIDEBAR_KEY, String(v));
});

export function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
}

// ── Per-connection dot color ──
const DOT_COLORS_KEY = "breezeresp-dot-colors";
export const DEFAULT_DOT_COLOR = "#22c55e"; // Tailwind green-500 (same as bg-success)

/** Preset color palette for quick selection */
export const DOT_COLOR_PRESETS = [
  { color: "#22c55e", name: "Green" },   // default / success
  { color: "#3b82f6", name: "Blue" },    // info
  { color: "#06b6d4", name: "Cyan" },    // cool
  { color: "#a855f7", name: "Purple" },  // creative
  { color: "#ec4899", name: "Pink" },    // accent
  { color: "#f97316", name: "Orange" },  // warm
  { color: "#eab308", name: "Yellow" },  // bright
  { color: "#6366f1", name: "Indigo" },  // distinct
];

/** Reactive map: connectionId → custom hex color */
export const dotColorMap = ref<Record<string, string>>(loadColorMap());

function loadColorMap(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(DOT_COLORS_KEY) || "{}");
  } catch {
    return {};
  }
}

watch(dotColorMap, (v) => {
  localStorage.setItem(DOT_COLORS_KEY, JSON.stringify(v));
}, { deep: true });

/** Get the dot color for a specific connection (falls back to default) */
export function getDotColor(connectionId: string): string {
  return dotColorMap.value[connectionId] || DEFAULT_DOT_COLOR;
}

/** Set a custom dot color for a connection */
export function setDotColor(connectionId: string, color: string) {
  dotColorMap.value = { ...dotColorMap.value, [connectionId]: color };
}

/** Reset a connection's dot color to default */
export function resetDotColor(connectionId: string) {
  const next = { ...dotColorMap.value };
  delete next[connectionId];
  dotColorMap.value = next;
}

/** Remove color entry when a connection is deleted */
export function removeDotColor(connectionId: string) {
  resetDotColor(connectionId);
}
