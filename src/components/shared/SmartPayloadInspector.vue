<script lang="ts">
// ─── Recursive JSON Node (render function component) ───
import { defineComponent, h, type PropType } from "vue";
import { ChevronDown, ChevronRight } from "lucide-vue-next";

function isObj(v: any): boolean {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}
function isArr(v: any): boolean {
  return Array.isArray(v);
}
function valDisplay(v: any): string {
  if (v === null) return "null";
  if (typeof v === "string") return `"${v}"`;
  return String(v);
}
function valClass(v: any): string {
  if (v === null) return "text-text-muted";
  if (typeof v === "string") return "text-green-600";
  if (typeof v === "number") return "text-blue-600";
  if (typeof v === "boolean") return "text-amber-600";
  return "text-text-primary";
}

const JsonNode = defineComponent({
  name: "JsonNode",
  props: {
    value: { type: null as unknown as PropType<any>, required: true },
    path: { type: String, required: true },
    collapsedPaths: { type: Object as PropType<Set<string>>, required: true },
  },
  emits: ["toggle"],
  setup(props, { emit }) {
    function renderChildren(val: any, parentPath: string, keyOrIdx: string | number, isKey: boolean): any {
      const childPath = `${parentPath}${isKey ? "." + String(keyOrIdx) : "[" + keyOrIdx + "]"}`;
      const isContainer = isObj(val) || isArr(val);

      if (isContainer) {
        const collapsed = props.collapsedPaths.has(childPath);
        const summary = isArr(val) ? `[${val.length}]` : `{${Object.keys(val).length}}`;

        return h("div", [
          h("div", {
            class: "flex items-center gap-0.5 cursor-pointer hover:bg-bg-hover rounded px-0.5",
            onClick: () => emit("toggle", childPath),
          }, [
            h(collapsed ? ChevronRight : ChevronDown, { size: 12, class: "text-text-muted shrink-0" }),
            isKey
              ? h("span", { class: "text-purple-600" }, `"${String(keyOrIdx)}"`)
              : h("span", { class: "text-text-muted" }, `${keyOrIdx}`),
            h("span", { class: "text-text-muted" }, ":"),
            collapsed ? h("span", { class: "text-text-muted" }, summary) : null,
          ]),
          !collapsed
            ? h("div", { class: "ml-3 pl-2 border-l border-border-light" }, renderValue(val, childPath))
            : null,
        ]);
      } else {
        return h("div", { class: "flex items-start gap-0.5 pl-4" }, [
          isKey
            ? h("span", { class: "text-purple-600" }, `"${String(keyOrIdx)}"`)
            : h("span", { class: "text-text-muted" }, `${keyOrIdx}:`),
          h("span", { class: "text-text-muted" }, ":"),
          h("span", { class: valClass(val) }, valDisplay(val)),
        ]);
      }
    }

    function renderValue(val: any, currentPath: string): any {
      if (isObj(val)) {
        return Object.entries(val).map(([k, v]) => renderChildren(v, currentPath, k, true));
      }
      if (isArr(val)) {
        return (val as any[]).map((item: any, idx: number) => renderChildren(item, currentPath, idx, false));
      }
      return h("span", { class: valClass(val) }, valDisplay(val));
    }

    return () => renderValue(props.value, props.path);
  },
});

export { JsonNode };
</script>

<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useCopyTip } from "@/utils/copyTip";
import {
  Copy,
  Braces,
  FileCode,
  Hash,
  FileText,
  UnfoldVertical,
  FoldVertical,
  X,
  Maximize2,
} from "lucide-vue-next";

const { t } = useI18n();
const { copyWithTip } = useCopyTip();

const props = defineProps<{
  payload: string;
  channel?: string;
  timestamp?: number;
}>();

// ─── Modal state ─────────────────────────────────────────────────────
const showModal = ref(false);

function openModal() {
  showModal.value = true;
}

function closeModal() {
  showModal.value = false;
}

// ─── Format Detection ────────────────────────────────────────────────

type PayloadFormat = "json" | "xml" | "hex" | "text";

function detectFormat(raw: string): PayloadFormat {
  const s = raw.trim();
  if (!s) return "text";

  if ((s.startsWith("{") && s.endsWith("}")) || (s.startsWith("[") && s.endsWith("]"))) {
    try {
      JSON.parse(s);
      return "json";
    } catch { /* not json */ }
  }

  if (/^<\?xml\s/i.test(s) || /^<[a-zA-Z][\s\S]*>[\s\S]*<\/[a-zA-Z][\s\S]*>\s*$/.test(s)) {
    return "xml";
  }

  if (s.length > 4) {
    let nonPrintable = 0;
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      if (c < 0x20 && c !== 0x0a && c !== 0x0d && c !== 0x09) nonPrintable++;
    }
    if (nonPrintable / s.length > 0.3) return "hex";
  }

  return "text";
}

const format = computed<PayloadFormat>(() => detectFormat(props.payload));

const formatBadgeClass = computed(() => {
  const map: Record<PayloadFormat, string> = {
    json: "bg-amber-500/15 text-amber-600 border-amber-500/20",
    xml: "bg-blue-500/15 text-blue-600 border-blue-500/20",
    hex: "bg-purple-500/15 text-purple-600 border-purple-500/20",
    text: "bg-bg-secondary text-text-muted border-border-light",
  };
  return map[format.value];
});

const formatIcon = computed(() => {
  const map: Record<PayloadFormat, any> = { json: Braces, xml: FileCode, hex: Hash, text: FileText };
  return map[format.value];
});

const formatLabel = computed(() => {
  const map: Record<PayloadFormat, string> = {
    json: t("pubsub.payloadInspector.formatJson"),
    xml: t("pubsub.payloadInspector.formatXml"),
    hex: t("pubsub.payloadInspector.formatHex"),
    text: t("pubsub.payloadInspector.formatText"),
  };
  return map[format.value];
});

// ─── One-line preview (for compact list view) ────────────────────────

const previewText = computed(() => {
  const s = props.payload;
  if (format.value === "json") {
    try {
      return JSON.stringify(JSON.parse(s));
    } catch { /* fallback */ }
  }
  return s.replace(/\n/g, " ").replace(/\s+/g, " ");
});

// ─── JSON Parsing ────────────────────────────────────────────────────

const parsedJson = computed(() => {
  if (format.value !== "json") return null;
  try { return JSON.parse(props.payload.trim()); } catch { return null; }
});

// ─── XML Formatting ──────────────────────────────────────────────────

const formattedXml = computed(() => {
  if (format.value !== "xml") return null;
  return formatXmlString(props.payload.trim());
});

function formatXmlString(xml: string): string {
  let formatted = "";
  let indent = 0;
  const parts = xml.replace(/>\s*</g, ">\n<").split("\n");
  for (const part of parts) {
    const trimmed = part.trim();
    if (!trimmed) continue;
    if (/^<\//.test(trimmed)) indent = Math.max(indent - 1, 0);
    formatted += "  ".repeat(indent) + trimmed + "\n";
    if (/^<[^\/!?][^>]*[^\/]>$/.test(trimmed) || /^<[^\/!?]>[^<]*$/.test(trimmed)) {
      if (!/\/>$/.test(trimmed)) indent++;
    }
  }
  return formatted.trim();
}

function highlightXml(xml: string): string {
  let escaped = xml.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  escaped = escaped.replace(
    /(&lt;\/?)([\w:.-]+)((?:\s+[\w:.-]+\s*=\s*"[^"]*")*\s*?)(\/?&gt;)/g,
    (_match, open, tagName, attrs, close) => {
      let result = `<span class="xml-tag">${open}${tagName}</span>`;
      if (attrs) {
        result += attrs.replace(
          /([\w:.-]+)(\s*=\s*)(".*?")/g,
          '<span class="xml-attr-name">$1</span>$2<span class="xml-attr-value">$3</span>',
        );
      }
      result += `<span class="xml-tag">${close}</span>`;
      return result;
    },
  );
  return escaped;
}

// ─── Hex Dump ────────────────────────────────────────────────────────

const hexDump = computed(() => {
  if (format.value !== "hex") return null;
  const bytes: number[] = [];
  for (let i = 0; i < props.payload.length; i++) bytes.push(props.payload.charCodeAt(i));
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const chunk = bytes.slice(offset, offset + 16);
    const hexPart = chunk.map((b) => b.toString(16).padStart(2, "0")).join(" ");
    const asciiPart = chunk.map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    lines.push(`${offset.toString(16).padStart(8, "0")}  ${hexPart.padEnd(47, " ")}  |${asciiPart}|`);
  }
  return lines.join("\n");
});

// ─── Modal: JSON Tree Collapse State ─────────────────────────────────

const collapsedPaths = ref<Set<string>>(new Set());
const showRaw = ref(false);

function togglePath(path: string) {
  const s = collapsedPaths.value;
  if (s.has(path)) s.delete(path); else s.add(path);
  collapsedPaths.value = new Set(s);
}

function expandAll() { collapsedPaths.value = new Set(); }

function collapseAll() {
  const paths = new Set<string>();
  if (parsedJson.value !== null && typeof parsedJson.value === "object") {
    collectPaths(parsedJson.value, "$", paths);
  }
  collapsedPaths.value = paths;
}

function collectPaths(obj: any, path: string, paths: Set<string>) {
  if (obj && typeof obj === "object") {
    paths.add(path);
    if (Array.isArray(obj)) obj.forEach((item, i) => collectPaths(item, `${path}[${i}]`, paths));
    else for (const key of Object.keys(obj)) collectPaths(obj[key], `${path}.${key}`, paths);
  }
}

// Reset state when modal opens
watch(showModal, (open) => {
  if (open) {
    collapsedPaths.value = new Set();
    showRaw.value = false;
  }
});

// ─── Copy ────────────────────────────────────────────────────────────

function copyPayload(event: Event) {
  copyWithTip(props.payload, event);
}
</script>

<template>
  <!-- ─── Compact inline preview (one line) ─── -->
  <div
    class="payload-preview flex items-center gap-2 cursor-pointer group"
    @click="openModal"
  >
    <span
      class="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-semibold rounded border shrink-0"
      :class="formatBadgeClass"
    >
      <component :is="formatIcon" :size="10" />
      {{ formatLabel }}
    </span>
    <span class="text-xs font-mono text-text-secondary truncate flex-1 select-none" :title="props.payload">
      {{ previewText }}
    </span>
    <button
      class="shrink-0 p-0.5 rounded text-text-muted opacity-0 group-hover:opacity-100 hover:text-redis hover:bg-redis/10 transition-all"
      :title="t('pubsub.payloadInspector.copyPayload')"
      @click.stop="copyPayload"
    >
      <Copy :size="12" />
    </button>
    <Maximize2 :size="12" class="shrink-0 text-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
  </div>

  <!-- ─── Modal Dialog ─── -->
  <Teleport to="body">
    <div
      v-if="showModal"
      class="fixed inset-0 z-[9990] flex items-center justify-center"
      @click.self="closeModal"
    >
      <div class="absolute inset-0 bg-black/50" />
      <div
        class="relative z-[9999] bg-bg-secondary border border-border rounded-2xl shadow-[0_20px_60px_-10px_rgba(0,0,0,0.3)] w-[640px] max-w-[92vw] max-h-[80vh] flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-200"
      >
        <!-- Header -->
        <div class="flex items-center gap-3 px-5 py-3 border-b border-border shrink-0 bg-gradient-to-r from-redis/5 to-transparent">
          <div class="w-7 h-7 rounded-lg bg-redis/10 flex items-center justify-center shrink-0">
            <component :is="formatIcon" :size="14" class="text-redis" />
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-semibold text-text-primary truncate">{{ channel || 'Payload' }}</h3>
              <span
                class="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-semibold rounded border shrink-0"
                :class="formatBadgeClass"
              >
                {{ formatLabel }}
              </span>
            </div>
            <p v-if="timestamp" class="text-[11px] text-text-muted mt-0.5 font-mono">
              {{ new Date(timestamp).toLocaleString() }}
            </p>
          </div>
          <!-- Actions -->
          <div class="flex items-center gap-1 shrink-0">
            <!-- Expand/Collapse all (JSON) -->
            <template v-if="format === 'json' && parsedJson !== null && !showRaw">
              <button @click="expandAll" class="p-1.5 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors" :title="t('pubsub.payloadInspector.expandAll')">
                <UnfoldVertical :size="13" />
              </button>
              <button @click="collapseAll" class="p-1.5 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors" :title="t('pubsub.payloadInspector.collapseAll')">
                <FoldVertical :size="13" />
              </button>
            </template>
            <!-- Toggle raw/formatted -->
            <button
              v-if="format === 'json' || format === 'xml'"
              @click="showRaw = !showRaw"
              class="p-1.5 rounded text-text-muted hover:text-text-primary hover:bg-bg-hover transition-colors"
              :title="showRaw ? t('pubsub.payloadInspector.formattedView') : t('pubsub.payloadInspector.rawView')"
            >
              <component :is="showRaw ? Braces : FileText" :size="13" />
            </button>
            <!-- Copy -->
            <button @click="copyPayload" class="p-1.5 rounded text-text-muted hover:text-redis hover:bg-redis/10 transition-colors" :title="t('pubsub.payloadInspector.copyPayload')">
              <Copy :size="13" />
            </button>
            <!-- Close -->
            <button @click="closeModal" class="p-1.5 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors" :title="t('common.close')">
              <X :size="13" />
            </button>
          </div>
        </div>

        <!-- Body -->
        <div class="flex-1 min-h-0 overflow-auto p-5">
          <!-- JSON Tree -->
          <template v-if="format === 'json' && parsedJson !== null && !showRaw">
            <div class="text-xs font-mono">
              <JsonNode :value="parsedJson" path="$" :collapsed-paths="collapsedPaths" @toggle="togglePath" />
            </div>
          </template>
          <!-- JSON Raw -->
          <pre v-else-if="format === 'json' && showRaw" class="text-xs font-mono whitespace-pre-wrap text-text-primary">{{ props.payload }}</pre>
          <!-- XML Formatted -->
          <pre v-else-if="format === 'xml' && formattedXml && !showRaw" class="text-xs font-mono whitespace-pre-wrap xml-view" v-html="highlightXml(formattedXml)"></pre>
          <!-- XML Raw -->
          <pre v-else-if="format === 'xml' && showRaw" class="text-xs font-mono whitespace-pre-wrap text-text-primary">{{ props.payload }}</pre>
          <!-- Hex -->
          <pre v-else-if="format === 'hex' && hexDump" class="text-xs font-mono whitespace-pre text-text-primary leading-relaxed">{{ hexDump }}</pre>
          <!-- Plain Text -->
          <pre v-else class="text-xs font-mono whitespace-pre-wrap text-text-primary">{{ props.payload }}</pre>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.payload-preview {
  min-width: 0;
}

/* XML syntax highlighting */
:deep(.xml-tag) { color: #2563eb; }
:deep(.xml-attr-name) { color: #9333ea; }
:deep(.xml-attr-value) { color: #16a34a; }
:deep(.xml-text) { color: var(--color-text-primary, #1f2937); }
:deep(.xml-comment) { color: #9ca3af; font-style: italic; }
</style>
