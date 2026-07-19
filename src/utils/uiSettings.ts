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

// ── Connection dot color ──
const DOT_COLOR_KEY = "breezeresp-dot-color";
const DEFAULT_DOT_COLOR = "#22c55e"; // Tailwind green-500 (same as bg-success)

export const dotColor = ref(localStorage.getItem(DOT_COLOR_KEY) || DEFAULT_DOT_COLOR);

watch(dotColor, (v) => {
  localStorage.setItem(DOT_COLOR_KEY, v);
  applyDotColor(v);
});

/** Apply the dot color as a CSS custom property on :root */
export function applyDotColor(color: string) {
  document.documentElement.style.setProperty("--dot-connected", color);
}

// Initialize on import
applyDotColor(dotColor.value);

export function resetDotColor() {
  dotColor.value = DEFAULT_DOT_COLOR;
}

export { DEFAULT_DOT_COLOR };
