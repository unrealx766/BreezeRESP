import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { DiffEntry, SandboxHistoryItem } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";
import { useCascadeStore } from "./cascadeStore";
import { toast } from "@/utils/toast";

export const useSandboxStore = defineStore("sandbox", () => {
  const MAX_HISTORY = 50;
  const commandInput = ref("");
  const executing = ref(false);
  const applying = ref(false);
  const rollingBack = ref(false);
  const commandError = ref<string | null>(null);
  const currentDiff = ref<DiffEntry[]>([]);
  const commandResult = ref<string | null>(null);
  const history = ref<SandboxHistoryItem[]>([]);
  const showPreview = ref(false);
  const currentCommand = ref<string>("");
  /** Original key types BEFORE command execution — needed for type-safe rollback */
  const currentKeyTypes = ref<Record<string, string>>({});

  const hasDiff = computed(() => currentDiff.value.length > 0);
  const addedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "added").length);
  const modifiedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "modified").length);
  const deletedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "deleted").length);
  const unchangedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "unchanged").length);
  const isReadOnly = computed(() => commandResult.value != null);

  // --- Frontend command validation ---
  const BLOCKED_COMMANDS = new Set([
    "CONFIG", "DEBUG", "MODULE", "SCRIPT", "EVAL", "EVALSHA",
    "SLAVEOF", "REPLICAOF", "SHUTDOWN", "ACL",
  ]);

  /** Minimum arguments (excluding the command name) for common commands */
  const MIN_ARGS: Record<string, number> = {
    GET: 1, SET: 2, DEL: 1, HGET: 2, HSET: 3, HDEL: 2, HGETALL: 1,
    HMSET: 3, HINCRBY: 3, LPUSH: 2, RPUSH: 2, LPOP: 1, RPOP: 1,
    LRANGE: 3, LLEN: 1, LINDEX: 2, SADD: 2, SREM: 2, SMEMBERS: 1,
    SCARD: 1, SISMEMBER: 2, ZADD: 3, ZREM: 2, ZRANGE: 3, ZCARD: 1,
    ZSCORE: 2, ZRANK: 2, EXPIRE: 2, PERSIST: 1, RENAME: 2, RENAMENX: 2,
    TTL: 1, PTTL: 1, TYPE: 1, EXISTS: 1, APPEND: 2, INCR: 1, DECR: 1,
    SETNX: 2, GETSET: 2, SETEX: 3, PSETEX: 3, MSET: 2, MGET: 1,
    INCRBY: 2, DECRBY: 2, STRLEN: 1, HLEN: 1, HKEYS: 1, HVALS: 1,
    HEXISTS: 2, HMGET: 2,
  };

  function validateCommand(input: string): string | null {
    const trimmed = input.trim();
    if (!trimmed) return null; // empty handled by button disabled state

    const parts = trimmed.split(/\s+/);
    const cmd = parts[0].toUpperCase();

    if (BLOCKED_COMMANDS.has(cmd)) {
      return `命令 "${cmd}" 已被安全策略禁止`;
    }

    const minArgs = MIN_ARGS[cmd];
    if (minArgs !== undefined && parts.length - 1 < minArgs) {
      return `命令 "${cmd}" 至少需要 ${minArgs} 个参数，当前只有 ${parts.length - 1} 个`;
    }

    return null;
  }

  async function executePreview() {
    if (!commandInput.value.trim()) return;

    commandError.value = null;

    // Frontend validation
    const validationError = validateCommand(commandInput.value);
    if (validationError) {
      commandError.value = validationError;
      return;
    }

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) {
      toast.error("Not connected. Please connect first.");
      return;
    }

    executing.value = true;
    try {
      const result = await tauriApi.sandbox.preview(connId, commandInput.value.trim());

      // Map Rust diff to frontend DiffEntry
      currentDiff.value = result.diff.map((d) => ({
        path: d.path,
        keyType: d.keyType,
        before: d.before,
        after: d.after,
        beforeRaw: d.beforeRaw,
        afterRaw: d.afterRaw,
        changeType: d.changeType as "added" | "modified" | "deleted" | "unchanged",
      }));

      commandResult.value = result.commandResult;
      currentCommand.value = commandInput.value.trim();
      currentKeyTypes.value = result.keyTypes ?? {};
      showPreview.value = true;
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      commandError.value = msg;
      toast.error(msg);
      console.error("Sandbox preview failed:", e);
      currentDiff.value = [];
      commandResult.value = null;
    } finally {
      executing.value = false;
    }
  }

  async function applyChange() {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const cmd = currentCommand.value;
    if (!connId || !cmd) {
      if (!connId) toast.error("Not connected. Please connect first.");
      return;
    }

    applying.value = true;

    try {
      // Re-execute the command in Redis for real and clear pending state.
      const ok = await tauriApi.sandbox.apply(connId, cmd);
      if (!ok) {
        toast.error("Apply returned false.");
        return;
      }

      // Build beforeState from the cumulative diff.
      // beforeRaw holds the ORIGINAL Redis state (before any pending preview),
      // which is exactly what we need for history-item rollback.
      const beforeState: Record<string, string> = {};
      const addedKeys: string[] = [];
      for (const entry of currentDiff.value) {
        if (entry.changeType === "added") {
          addedKeys.push(entry.path);
        } else if (entry.beforeRaw != null) {
          beforeState[entry.path] = entry.beforeRaw;
        }
      }

      history.value.unshift({
        id: `sb-${Date.now()}`,
        snapshotId: "",
        command: cmd,
        timestamp: Date.now(),
        status: "applied",
        diffCount: currentDiff.value.length,
        beforeState,
        addedKeys,
        keyTypes: currentKeyTypes.value,
      });
      // Cap history to prevent unbounded growth
      if (history.value.length > MAX_HISTORY) {
        history.value = history.value.slice(0, MAX_HISTORY);
      }

      // Refresh keys after apply
      try {
        const cascade = useCascadeStore();
        await cascade.refreshKeys(true);
      } catch { /* best-effort */ }

      // Only reset preview on success
      resetPreviewUI();
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      toast.error(msg);
      console.error("Sandbox apply failed:", e);
    } finally {
      applying.value = false;
    }
  }

  /**
   * Reset the preview UI without touching the backend pending state.
   * Called after apply (backend already cleared pending state).
   */
  function resetPreviewUI() {
    currentDiff.value = [];
    commandResult.value = null;
    showPreview.value = false;
    commandInput.value = "";
    currentCommand.value = "";
    currentKeyTypes.value = {};
    commandError.value = null;
  }

  /**
   * Cancel all pending sandbox previews.
   * Since previews always rollback (Redis is never modified), this only
   * clears the in-memory shadow state on the backend and resets the UI.
   */
  async function resetPreview() {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;

    if (connId) {
      try {
        await tauriApi.sandbox.cancel(connId);
      } catch (e) {
        console.error("Sandbox cancel failed:", e);
      }
    }

    resetPreviewUI();
  }

  async function rollbackHistoryItem(id: string) {
    const item = history.value.find((h) => h.id === id);
    if (!item || item.status !== "applied") return;

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) {
      toast.error("No active connection.");
      return;
    }

    rollingBack.value = true;

    try {
      const ok = await tauriApi.sandbox.rollback(connId, item.beforeState, item.addedKeys, item.keyTypes);
      if (!ok) {
        toast.error("Rollback returned false.");
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
      toast.error(msg);
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
    commandError,
    currentDiff,
    commandResult,
    history,
    showPreview,
    hasDiff,
    addedCount,
    modifiedCount,
    deletedCount,
    unchangedCount,
    isReadOnly,
    executePreview,
    applyChange,
    resetPreview,
    rollbackHistoryItem,
  };
});
