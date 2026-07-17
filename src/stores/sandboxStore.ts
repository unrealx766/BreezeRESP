import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { DiffEntry, SandboxHistoryItem } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";
import { useCascadeStore } from "./cascadeStore";
import { useHistoryStore } from "./historyStore";
import { toast } from "@/utils/toast";
import { computeInverseCommands } from "@/utils/rollbackInverse";

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
  const currentKeyTypes = ref<Record<string, string>>({});

  const hasDiff = computed(() => currentDiff.value.length > 0);
  const addedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "added").length);
  const modifiedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "modified").length);
  const deletedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "deleted").length);
  const unchangedCount = computed(() => currentDiff.value.filter((d) => d.changeType === "unchanged").length);
  const isReadOnly = computed(() => commandResult.value != null);

  // --- Frontend command validation ---
  const BLOCKED_COMMANDS = new Set([
    // Server administration
    "CONFIG", "DEBUG", "MODULE", "SHUTDOWN", "ACL",
    "FLUSHDB", "FLUSHALL", "SWAPDB",
    "BGSAVE", "BGREWRITEAOF", "SAVE",
    "CLUSTER", "MIGRATE", "RESTORE", "SORT", "WAIT",
    "OBJECT", "LATENCY", "SLOWLOG",
    // Scripting / replication
    "SCRIPT", "EVAL", "EVALSHA", "SLAVEOF", "REPLICAOF",
    // Connection-breaking commands
    "SUBSCRIBE", "PSUBSCRIBE", "UNSUBSCRIBE", "PUNSUBSCRIBE",
    "MONITOR", "SELECT", "QUIT", "RESET",
    // Blocking commands
    "BLPOP", "BRPOP", "BLMOVE", "BRPOPLPUSH", "BZPOPMIN", "BZPOPMAX",
    // Data movement
    "COPY", "MOVE",
  ]);

  /** Minimum arguments (excluding the command name) for common commands */
  const MIN_ARGS: Record<string, number> = {
    // String
    GET: 1, SET: 2, APPEND: 2, INCR: 1, DECR: 1,
    SETNX: 2, GETSET: 2, SETEX: 3, PSETEX: 3,
    INCRBY: 2, DECRBY: 2, STRLEN: 1, MSET: 2, MGET: 1,
    // Hash
    HGET: 2, HSET: 3, HDEL: 2, HGETALL: 1,
    HMSET: 3, HINCRBY: 3, HLEN: 1, HKEYS: 1, HVALS: 1,
    HEXISTS: 2, HMGET: 2,
    // List
    LPUSH: 2, RPUSH: 2, LPOP: 1, RPOP: 1,
    LRANGE: 3, LLEN: 1, LINDEX: 2,
    // Set
    SADD: 2, SREM: 2, SMEMBERS: 1,
    SCARD: 1, SISMEMBER: 2, SRANDMEMBER: 1,
    // Sorted set
    ZADD: 3, ZREM: 2, ZRANGE: 3, ZCARD: 1,
    ZSCORE: 2, ZRANK: 2, ZRANGEBYSCORE: 3,
    // Key-level
    DEL: 1, EXPIRE: 2, PERSIST: 1, RENAME: 2, RENAMENX: 2,
    TTL: 1, PTTL: 1, TYPE: 1, EXISTS: 1,
    // Read-only
    KEYS: 1, SCAN: 1, DBSIZE: 0, INFO: 0, PING: 0, ECHO: 1,
  };

  /**
   * Parse a command string into parts, respecting quoted strings.
   * Mirrors the Rust parse_command_parts function.
   */
  function parseCommandParts(input: string): string[] {
    const parts: string[] = [];
    let current = "";
    let i = 0;

    while (i < input.length) {
      const c = input[i];

      if ((c === ' ' || c === '\t') && current === "") {
        i++;
        continue;
      }

      if (c === '"') {
        i++;
        while (i < input.length) {
          const inner = input[i];
          if (inner === '"') { i++; break; }
          if (inner === '\\' && i + 1 < input.length) {
            current += input[i + 1];
            i += 2;
          } else {
            current += inner;
            i++;
          }
        }
        continue;
      }

      if (c === "'") {
        i++;
        while (i < input.length) {
          if (input[i] === "'") { i++; break; }
          current += input[i];
          i++;
        }
        continue;
      }

      if (c === ' ' || c === '\t') {
        parts.push(current);
        current = "";
        i++;
        continue;
      }

      current += c;
      i++;
    }

    if (current !== "") {
      parts.push(current);
    }

    return parts;
  }

  /** Commands that require paired arguments (field-value or score-member) */
  const PAIRED_COMMANDS: Record<string, { pairArgs: 'after-key' | 'all'; keyCount: number }> = {
    HSET: { pairArgs: 'after-key', keyCount: 1 },
    HMSET: { pairArgs: 'after-key', keyCount: 1 },
    ZADD: { pairArgs: 'after-key', keyCount: 1 },
    MSET: { pairArgs: 'all', keyCount: 0 },
  };

  function validateCommand(input: string): string | null {
    const cmd = input.trim();
    if (!cmd) return null;

    const parts = parseCommandParts(cmd);
    if (parts.length === 0) return null;

    const cmdName = parts[0].toUpperCase();

    if (BLOCKED_COMMANDS.has(cmdName)) {
      return `命令 "${cmdName}" 已被安全策略禁止`;
    }

    const minArgs = MIN_ARGS[cmdName];
    if (minArgs !== undefined && parts.length - 1 < minArgs) {
      return `命令 "${cmdName}" 至少需要 ${minArgs} 个参数，当前只有 ${parts.length - 1} 个`;
    }

    // Check paired arguments
    const pairRule = PAIRED_COMMANDS[cmdName];
    if (pairRule) {
      const argCount = pairRule.pairArgs === 'after-key'
        ? parts.length - 1 - pairRule.keyCount  // args after the key
        : parts.length - 1;                      // all args
      if (argCount % 2 !== 0) {
        const pairDesc = pairRule.pairArgs === 'after-key' ? 'field-value' : 'key-value';
        return `命令 "${cmdName}" 需要成对的 ${pairDesc} 参数，当前参数数量为奇数`;
      }
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
      // Merge (not replace): cumulative previews return keyTypes for only
      // the current command's affected keys; we need ALL original types for rollback.
      currentKeyTypes.value = { ...currentKeyTypes.value, ...(result.keyTypes ?? {}) };
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

      // Compute precise inverse rollback commands using frontend logic
      const rollbackCommands = computeInverseCommands(
        cmd,
        currentDiff.value,
        currentKeyTypes.value,
      );

      history.value.unshift({
        id: `sb-${Date.now()}`,
        snapshotId: "",
        command: cmd,
        timestamp: Date.now(),
        status: "applied",
        diffCount: currentDiff.value.length,
        rollbackCommands,
      });
      // Cap history to prevent unbounded growth
      if (history.value.length > MAX_HISTORY) {
        history.value = history.value.slice(0, MAX_HISTORY);
      }

      // Record to command history
      const connStore = useConnectionStore();
      const conn = connStore.connections.find((c) => c.id === connId);
      useHistoryStore().addRecord({
        connectionId: connId,
        connectionName: conn?.name ?? "",
        db: connStore.getActiveDb(connId),
        command: cmd,
        source: "sandbox",
        success: true,
      });

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
      const ok = await tauriApi.sandbox.rollback(connId, item.rollbackCommands);
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
