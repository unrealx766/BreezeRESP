import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { DiffEntry, SandboxHistoryItem } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";
import { useCascadeStore } from "./cascadeStore";

export const useSandboxStore = defineStore("sandbox", () => {
  const commandInput = ref("");
  const executing = ref(false);
  const applying = ref(false);
  const rollingBack = ref(false);
  const lastError = ref<string | null>(null);
  const currentDiff = ref<DiffEntry[]>([]);
  const history = ref<SandboxHistoryItem[]>([]);
  const showPreview = ref(false);
  const currentSnapshotId = ref<string | null>(null);
  const currentCommand = ref<string>("");

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

      currentSnapshotId.value = result.snapshotId;
      currentCommand.value = commandInput.value.trim();
      showPreview.value = true;
    } catch (e) {
      console.error("Sandbox preview failed:", e);
      currentDiff.value = [];
      currentSnapshotId.value = null;
    } finally {
      executing.value = false;
    }
  }

  async function applyChange() {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const cmd = currentCommand.value;
    if (!connId || !cmd) return;

    applying.value = true;
    lastError.value = null;

    try {
      const ok = await tauriApi.sandbox.apply(connId, cmd);
      if (!ok) {
        lastError.value = "Apply returned false.";
        return;
      }

      // Extract before-state and added keys from diff for reliable rollback
      const beforeState: Record<string, string> = {};
      const addedKeys: string[] = [];
      for (const entry of currentDiff.value) {
        if (entry.changeType === "added") {
          addedKeys.push(entry.path);
        } else if (entry.before != null) {
          beforeState[entry.path] = entry.before;
        }
      }

      history.value.unshift({
        id: `sb-${Date.now()}`,
        snapshotId: currentSnapshotId.value ?? "",
        command: cmd,
        timestamp: Date.now(),
        status: "applied",
        diffCount: currentDiff.value.length,
        beforeState,
        addedKeys,
      });

      // Refresh keys after apply
      try {
        const cascade = useCascadeStore();
        await cascade.refreshKeys(true);
      } catch { /* best-effort */ }

      // Only reset preview on success
      resetPreview();
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      lastError.value = msg;
      console.error("Sandbox apply failed:", e);
    } finally {
      applying.value = false;
    }
  }

  function resetPreview() {
    currentDiff.value = [];
    showPreview.value = false;
    commandInput.value = "";
    currentSnapshotId.value = null;
    currentCommand.value = "";
  }

  async function rollbackHistoryItem(id: string) {
    const item = history.value.find((h) => h.id === id);
    if (!item || item.status !== "applied") return;

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) {
      lastError.value = "No active connection.";
      return;
    }

    rollingBack.value = true;
    lastError.value = null;

    try {
      const ok = await tauriApi.sandbox.rollback(connId, item.beforeState, item.addedKeys);
      if (!ok) {
        lastError.value = "Rollback returned false.";
        return;
      }
      item.status = "rolled-back";

      // Refresh keys after rollback
      try {
        const cascade = useCascadeStore();
        await cascade.refreshKeys(true);
      } catch { /* best-effort */ }
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      lastError.value = msg;
      console.error("Sandbox history rollback failed:", e);
    } finally {
      rollingBack.value = false;
    }
  }

  return {
    commandInput,
    executing,
    applying,
    rollingBack,
    lastError,
    currentDiff,
    history,
    showPreview,
    hasDiff,
    addedCount,
    modifiedCount,
    deletedCount,
    executePreview,
    applyChange,
    resetPreview,
    rollbackHistoryItem,
  };
});
