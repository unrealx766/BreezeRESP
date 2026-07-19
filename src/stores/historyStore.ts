import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { CommandHistoryItem, CommandSource } from "@/types";
import { useConnectionStore } from "./connectionStore";

export const useHistoryStore = defineStore("history", () => {
  const MAX_HISTORY = 500;
  const history = ref<CommandHistoryItem[]>([]);

  /** Add a command record. Newest first. */
  function addRecord(params: {
    connectionId: string;
    connectionName: string;
    db: number;
    command: string;
    source: CommandSource;
    success: boolean;
    error?: string;
    durationMs?: number;
  }) {
    const item: CommandHistoryItem = {
      id: `hist-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      connectionId: params.connectionId,
      connectionName: params.connectionName,
      db: params.db,
      command: params.command,
      source: params.source,
      success: params.success,
      error: params.error,
      timestamp: Date.now(),
      durationMs: params.durationMs,
    };
    history.value.unshift(item);
    if (history.value.length > MAX_HISTORY) {
      history.value = history.value.slice(0, MAX_HISTORY);
    }
  }

  /**
   * Execute an async operation and record the command to history.
   * Automatically captures connectionId, connectionName, db, and success/failure.
   */
  async function execAndRecord<T>(
    command: string,
    source: CommandSource,
    fn: () => Promise<T>,
  ): Promise<T> {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const conn = connId ? connStore.connections.find((c) => c.id === connId) : undefined;
    const activeDb = connId ? connStore.getActiveDb(connId) : 0;
    const start = performance.now();
    try {
      const result = await fn();
      const durationMs = Math.round(performance.now() - start);
      if (connId) {
        addRecord({
          connectionId: connId,
          connectionName: conn?.name ?? "",
          db: activeDb,
          command,
          source,
          success: true,
          durationMs,
        });
      }
      return result;
    } catch (e) {
      const durationMs = Math.round(performance.now() - start);
      if (connId) {
        const errMsg = e instanceof Error ? e.message : typeof e === "string" ? e : String(e);
        addRecord({
          connectionId: connId,
          connectionName: conn?.name ?? "",
          db: activeDb,
          command,
          source,
          success: false,
          error: errMsg,
          durationMs,
        });
      }
      throw e;
    }
  }

  /** Clear all history or for a specific connection */
  function clearHistory(connectionId?: string) {
    if (connectionId) {
      history.value = history.value.filter((h) => h.connectionId !== connectionId);
    } else {
      history.value = [];
    }
  }

  /** Get history for a specific connection */
  function getHistoryForConnection(connectionId: string): CommandHistoryItem[] {
    return history.value.filter((h) => h.connectionId === connectionId);
  }

  /** Get distinct DB numbers used in a connection's history */
  function getDbsForConnection(connectionId: string): number[] {
    const dbs = new Set<number>();
    for (const h of history.value) {
      if (h.connectionId === connectionId) dbs.add(h.db);
    }
    return [...dbs].sort((a, b) => a - b);
  }

  /** Filter history by connection and optional DB */
  function getFiltered(connectionId: string, db?: number): CommandHistoryItem[] {
    let items = history.value.filter((h) => h.connectionId === connectionId);
    if (db !== undefined) {
      items = items.filter((h) => h.db === db);
    }
    return items;
  }

  const totalCount = computed(() => history.value.length);

  return {
    history,
    totalCount,
    addRecord,
    execAndRecord,
    clearHistory,
    getHistoryForConnection,
    getDbsForConnection,
    getFiltered,
  };
});
