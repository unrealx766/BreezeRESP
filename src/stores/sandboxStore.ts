import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { DiffEntry, SandboxHistoryItem } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

export const useSandboxStore = defineStore("sandbox", () => {
  const commandInput = ref("");
  const executing = ref(false);
  const currentDiff = ref<DiffEntry[]>([]);
  const history = ref<SandboxHistoryItem[]>([]);
  const showPreview = ref(false);

  const hasDiff = computed(() => currentDiff.value.length > 0);
  const addedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "added").length);
  const modifiedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "modified").length);
  const deletedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "deleted").length);

  async function executePreview() {
    if (!commandInput.value.trim()) return;

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    executing.value = true;
    try {
      const result = await tauriApi.sandbox.preview(connId, commandInput.value.trim());

      // Map Rust diff to frontend DiffEntry
      currentDiff.value = result.diff.map((d) => ({
        path: d.path,
        before: d.before,
        after: d.after,
        changeType: d.changeType as "added" | "modified" | "deleted",
      }));

      showPreview.value = true;
    } catch (e) {
      console.error("Sandbox preview failed:", e);
      currentDiff.value = [];
    } finally {
      executing.value = false;
    }
  }

  async function applyChange() {
    if (!commandInput.value.trim()) return;

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    try {
      // Pass the command as snapshot_id since Rust apply re-executes it
      await tauriApi.sandbox.apply(connId, commandInput.value.trim());

      history.value.unshift({
        id: `sb-${Date.now()}`,
        command: commandInput.value,
        timestamp: Date.now(),
        status: "applied",
        diffCount: currentDiff.value.length,
      });
    } catch (e) {
      console.error("Sandbox apply failed:", e);
    }

    resetPreview();
  }

  function rollbackChange() {
    // Preview already rolled back on the backend, just reset UI
    resetPreview();
  }

  function resetPreview() {
    currentDiff.value = [];
    showPreview.value = false;
    commandInput.value = "";
  }

  function rollbackHistoryItem(id: string) {
    const item = history.value.find((h) => h.id === id);
    if (item) item.status = "rolled-back";
  }

  return {
    commandInput,
    executing,
    currentDiff,
    history,
    showPreview,
    hasDiff,
    addedCount,
    modifiedCount,
    deletedCount,
    executePreview,
    applyChange,
    rollbackChange,
    resetPreview,
    rollbackHistoryItem,
  };
});
