import { defineStore } from "pinia";
import { ref, computed, watch } from "vue";
import type { RedisConnection, ConnectionStatus } from "@/types";
import { tauriApi, type RustConnectionConfig } from "@/services/tauriApi";
import { toast } from "@/utils/toast";
import { i18n } from "@/i18n";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";
import { useHistoryStore } from "@/stores/historyStore";
import { usePubsubStore } from "@/stores/pubsubStore";
import { usePipelineStore } from "@/stores/pipelineStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSandboxStore } from "@/stores/sandboxStore";
import { useCapabilityStore } from "@/stores/capabilityStore";

export const useConnectionStore = defineStore("connection", () => {
  const connections = ref<RedisConnection[]>([]);
  const activeConnectionId = ref<string | null>(null);
  const lastError = ref<string | null>(null);
  /** IDs of connections that have been connected during this session (persists until app exit) */
  const sessionConnectedIds = ref<Set<string>>(new Set());
  /** Tracks the currently active DB per connection (separate from the default DB in conn.db) */
  const activeDbMap = ref<Record<string, number>>({});
  /** Custom display order for session list (persisted in localStorage) */
  const sessionOrder = ref<string[]>((() => {
    try {
      const saved = localStorage.getItem("sessionOrder");
      return saved ? JSON.parse(saved) : [];
    } catch {
      return [];
    }
  })());

  // ── Cancellation support ──
  const _cancellers = new Map<string, () => void>();
  let _formTestCanceller: (() => void) | null = null;

  function _withCancel<T>(key: string, promise: Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      _cancellers.set(key, () => reject(new Error("__cancelled__")));
      promise.then(resolve, reject).finally(() => _cancellers.delete(key));
    });
  }

  function cancelConnect(id: string) {
    _cancellers.get(`connect:${id}`)?.();
  }
  function cancelTest(id: string) {
    _cancellers.get(`test:${id}`)?.();
  }
  function cancelFormTest() {
    _formTestCanceller?.();
  }

  const activeConnection = computed(() =>
    connections.value.find((c) => c.id === activeConnectionId.value) ?? null
  );

  const connectedCount = computed(
    () => connections.value.filter((c) => c.status === "connected").length
  );

  /** Connections visible in status bar: pinned (startup) + connected this session (stays until exit) */
  const statusBarConnections = computed(() => {
    const filtered = connections.value.filter(
      (c) => c.pinned || c.status === "connected" || sessionConnectedIds.value.has(c.id)
    );
    // Apply custom session order if available
    const order = sessionOrder.value;
    if (order.length > 0) {
      return [...filtered].sort((a, b) => {
        const ai = order.indexOf(a.id);
        const bi = order.indexOf(b.id);
        // Items not in order list go to the end
        return (ai === -1 ? Infinity : ai) - (bi === -1 ? Infinity : bi);
      });
    }
    return filtered;
  });

  /** Reorder session list: swap the positions of two sessions */
  function reorderSessions(fromId: string, toId: string) {
    if (fromId === toId) return;
    const currentIds = statusBarConnections.value.map((c) => c.id);
    const fromIdx = currentIds.indexOf(fromId);
    const toIdx = currentIds.indexOf(toId);
    if (fromIdx === -1 || toIdx === -1) return;
    // Build new order from current visible order, then swap
    const newOrder = [...currentIds];
    const [moved] = newOrder.splice(fromIdx, 1);
    newOrder.splice(toIdx, 0, moved);
    sessionOrder.value = newOrder;
    try {
      localStorage.setItem("sessionOrder", JSON.stringify(newOrder));
    } catch { /* ignore quota errors */ }
  }

  /** Load saved connections from encrypted local storage */
  async function loadSavedConnections() {
    try {
      const infos = await tauriApi.connection.getConnections();
      connections.value = infos.map((info) => ({
        id: info.id,
        name: info.name,
        host: info.host,
        port: info.port,
        password: "", // password not returned from backend for security
        db: info.db,
        ssl: info.ssl,
        cluster: info.cluster ?? false,
        nodes: info.nodes ?? [],
        status: "disconnected" as ConnectionStatus,
        pinned: info.pinned ?? false,
        hasPassword: info.hasPassword ?? false,
      }));
    } catch (e) {
      console.error("Failed to load saved connections:", e);
    }
  }

  /** Build a Rust-side config from a local connection
   *  @param forceKeepPassword  true  → always set keepPassword (connect flow)
   *                            false → never set keepPassword (save with password cleared)
   *                            undefined → auto-detect (default, backward-compatible)
   */
  function toRustConfig(conn: RedisConnection, forceKeepPassword?: boolean): RustConnectionConfig {
    const config: RustConnectionConfig = {
      id: conn.id,
      name: conn.name,
      host: conn.host,
      port: conn.port,
      password: conn.password,
      db: conn.db,
      ssl: conn.ssl,
      cluster: conn.cluster ?? false,
      nodes: conn.nodes ?? [],
      pinned: conn.pinned ?? false,
    };
    // If frontend doesn't have the real password (always empty after load),
    // tell backend to preserve the stored password
    const shouldKeep = forceKeepPassword ?? (!conn.password && !conn.id.startsWith("__form_test_"));
    if (shouldKeep) {
      config.keepPassword = true;
    }
    return config;
  }

  async function addConnection(conn: Omit<RedisConnection, "id" | "status">) {
    const newConn: RedisConnection = {
      ...conn,
      id: `conn-${Date.now()}`,
      status: "disconnected",
    };

    // Save to backend
    try {
      await tauriApi.connection.saveConnection(toRustConfig(newConn));
    } catch (e) {
      console.error("Failed to save connection:", e);
    }

    connections.value.push(newConn);
    return newConn;
  }

  async function updateConnection(id: string, patch: Partial<RedisConnection>, forceKeepPassword?: boolean) {
    const idx = connections.value.findIndex((c) => c.id === id);
    if (idx !== -1) {
      // If password is undefined in patch, user didn't change it → set empty so toRustConfig adds keepPassword
      if (patch.password === undefined) {
        patch = { ...patch, password: "" };
      }
      connections.value[idx] = { ...connections.value[idx], ...patch };
      // Sync hasPassword so card badge reflects actual state
      // - New password provided → true
      // - Explicitly cleared (forceKeepPassword === false) → false
      // - Otherwise (keep old password) → preserve existing value
      if (connections.value[idx].password) {
        connections.value[idx].hasPassword = true;
      } else if (forceKeepPassword === false) {
        connections.value[idx].hasPassword = false;
      }
      // Persist to disk (toRustConfig auto-sets keepPassword when password is empty)
      try {
        await tauriApi.connection.saveConnection(toRustConfig(connections.value[idx], forceKeepPassword));
      } catch (e) {
        console.error("Failed to save connection update:", e);
      }
    }
  }

  async function removeConnection(id: string) {
    try {
      await tauriApi.connection.deleteConnection(id);
    } catch (e) {
      console.error("Failed to delete connection:", e);
    }
    connections.value = connections.value.filter((c) => c.id !== id);
    delete activeDbMap.value[id];
    if (activeConnectionId.value === id) activeConnectionId.value = null;
    // Drop cached capability profile (backend cache is cleared on delete)
    useCapabilityStore().invalidate(id);
    // Clean up orphaned history records for the removed connection
    const historyStore = useHistoryStore();
    historyStore.clearHistory(id);
    // Drop any Pub/Sub state (backend tears down its listener on delete)
    usePubsubStore().clearConnection(id);
  }

  function setStatus(id: string, status: ConnectionStatus) {
    const conn = connections.value.find((c) => c.id === id);
    if (conn) conn.status = status;
  }

  /** Mark the active connection as lost (called by metrics polling on failure) */
  function markConnectionLost(id: string) {
    const conn = connections.value.find((c) => c.id === id);
    if (conn && conn.status === "connected") {
      conn.status = "error";
      lastError.value = "Connection lost";
      const msg = i18n.global.t("connection.connectionLost");
      toast.error(msg, 5000, conn.name);

      // Clear data browser content regardless of current page
      // Lazy store calls (inside function body) break the circular dependency safely
      const cascade = useCascadeStore();
      const detail = useDetailStore();
      cascade.keys = [];
      cascade.selectedKey = null;
      cascade.searchQuery = "";
      cascade.debouncedSearchQuery = "";
      cascade.typeFilter = "all";
      cascade.expandedPaths = new Set<string>();
      cascade.totalKeyCount = 0;
      detail.clearDetail();
      // Backend listener dies when the connection drops; clear local state.
      usePubsubStore().clearConnection(id);
    }
  }

  async function connect(id: string): Promise<boolean> {
    const conn = connections.value.find((c) => c.id === id);
    if (!conn) return false;

    lastError.value = null;
    setStatus(id, "connecting");
    try {
      await _withCancel(`connect:${id}`, tauriApi.connection.connect(toRustConfig(conn)));
      setStatus(id, "connected");
      activeConnectionId.value = id;
      // Drop any stale capability profile: the backend cache was cleared on
      // the previous disconnect, and the server may have been upgraded or
      // re-pointed since, so force a fresh probe on next access.
      useCapabilityStore().invalidate(id);
      conn.lastUsed = Date.now();
      sessionConnectedIds.value = new Set([...sessionConnectedIds.value, id]);
      // Restore last active DB if it differs from the default (session persistence across reconnect).
      // Cluster connections have no multi-DB, so skip.
      const savedDb = activeDbMap.value[id];
      if (!conn.cluster && savedDb !== undefined && savedDb !== conn.db) {
        try {
          await tauriApi.connection.switchDb(id, savedDb);
        } catch (e) {
          console.warn("Failed to restore active DB on reconnect:", e);
          delete activeDbMap.value[id];
        }
      }
      return true;
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      if (msg === "__cancelled__") {
        setStatus(id, "disconnected");
        return false;
      }
      console.error("Connect failed:", e);
      setStatus(id, "error");
      lastError.value = msg;
      return false;
    }
  }

  async function disconnect(id: string) {
    try {
      await tauriApi.connection.disconnect(id);
    } catch (e) {
      console.error("Disconnect failed:", e);
    }
    setStatus(id, "disconnected");
    // Backend tears down the pubsub listener and capability cache on
    // disconnect; mirror both locally.
    usePubsubStore().clearConnection(id);
    useCapabilityStore().invalidate(id);
    if (activeConnectionId.value === id) {
      activeConnectionId.value = null;

      // Clear data browser content on active disconnect
      const cascade = useCascadeStore();
      const detail = useDetailStore();
      cascade.keys = [];
      cascade.selectedKey = null;
      cascade.searchQuery = "";
      cascade.debouncedSearchQuery = "";
      cascade.typeFilter = "all";
      cascade.expandedPaths = new Set<string>();
      cascade.totalKeyCount = 0;
      detail.clearDetail();
    }
  }

  async function testConnection(id: string): Promise<boolean> {
    const conn = connections.value.find((c) => c.id === id);
    if (!conn) return false;

    lastError.value = null;
    setStatus(id, "connecting");
    try {
      const result = await _withCancel(`test:${id}`, tauriApi.connection.testConnection(toRustConfig(conn)));
      setStatus(id, result ? "connected" : "error");
      // Revert to disconnected since test doesn't maintain connection
      if (result) setStatus(id, "disconnected");
      return result;
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      if (msg === "__cancelled__") {
        setStatus(id, "disconnected");
        return false;
      }
      console.error("Test connection failed:", e);
      setStatus(id, "error");
      lastError.value = msg;
      return false;
    }
  }

  /** Test a connection from form data without saving it */
  async function testFormConnection(
    config: Omit<RedisConnection, "id" | "status">,
    editingId?: string | null
  ): Promise<boolean> {
    lastError.value = null;
    // If editing and password is empty, signal backend to use saved password
    const useSavedPw = editingId && !config.password;
    const tempConfig: RustConnectionConfig = {
      id: editingId || `__form_test_${Date.now()}`,
      name: config.name,
      host: config.host,
      port: config.port,
      password: config.password,
      db: config.db,
      ssl: config.ssl,
      cluster: config.cluster ?? false,
      nodes: config.nodes ?? [],
      pinned: false,
      useSavedPassword: useSavedPw || undefined,
    };
    try {
      const promise = tauriApi.connection.testConnection(tempConfig);
      return await new Promise<boolean>((resolve, reject) => {
        _formTestCanceller = () => reject(new Error("__cancelled__"));
        promise.then(resolve, reject).finally(() => { _formTestCanceller = null; });
      });
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      if (msg === "__cancelled__") return false;
      console.error("Form test connection failed:", e);
      lastError.value = msg;
      return false;
    }
  }

  /** Get the currently active DB for a connection (falls back to default conn.db) */
  function getActiveDb(id: string): number {
    const conn = connections.value.find((c) => c.id === id);
    return activeDbMap.value[id] ?? conn?.db ?? 0;
  }

  /** Switch the active database for a connected session */
  async function switchDb(db: number) {
    const id = activeConnectionId.value;
    if (!id) return;
    // Cluster mode has a single logical DB; nothing to switch.
    if (activeConnection.value?.cluster) return;

    try {
      await tauriApi.connection.switchDb(id, db);
      activeDbMap.value = { ...activeDbMap.value, [id]: db };
      // Subscriptions were bound to the previous DB and torn down by the
      // backend on switch; clear local Pub/Sub state to match.
      usePubsubStore().clearConnection(id);
    } catch (e) {
      console.error("Switch DB failed:", e);
      throw e;
    }
  }

  /** Toggle pin status for a connection (persist to disk) */
  async function togglePin(id: string) {
    const conn = connections.value.find((c) => c.id === id);
    if (!conn) return;
    conn.pinned = !conn.pinned;
    try {
      await tauriApi.connection.saveConnection(toRustConfig(conn));
    } catch (e) {
      console.error("Failed to save pin state:", e);
    }
  }

  /** Dismiss a disconnected session from the sidebar list (does NOT delete config from disk, does NOT unpin) */
  function dismissSession(id: string) {
    // Remove from session-connected set
    const next = new Set(sessionConnectedIds.value);
    next.delete(id);
    sessionConnectedIds.value = next;

    // Clear active if it was the dismissed connection
    if (activeConnectionId.value === id) activeConnectionId.value = null;
  }

  // Centralized reset of connection-scoped global (single-value) store state.
  // This fires on EVERY active-connection change regardless of which page is
  // currently mounted, so switching connections never leaves the previous
  // connection's data behind — closing the blind spot of per-page watches that
  // don't run while their page is unmounted. Stores keyed by connectionId
  // (pubsub, history) switch automatically and are not touched here; pages
  // still own their own data reload (e.g. BrowserPage re-fetches keys).
  watch(activeConnectionId, (newId, oldId) => {
    if (newId === oldId) return;

    const cascade = useCascadeStore();
    const detail = useDetailStore();
    cascade.keys = [];
    cascade.selectedKey = null;
    cascade.searchQuery = "";
    cascade.debouncedSearchQuery = "";
    cascade.typeFilter = "all";
    cascade.expandedPaths = new Set<string>();
    cascade.totalKeyCount = 0;
    detail.clearDetail();

    // Pipeline: keep the authored command queue (it intentionally survives
    // navigation) but drop the previous connection's execution results.
    usePipelineStore().clearResults();

    // Metrics: scalars self-heal on the next poll, but qpsHistory only appends
    // and would otherwise mix both connections' samples into one chart.
    useMetricsStore().resetMetrics();

    // Sandbox: preview diff / result / input / history are connection-specific.
    useSandboxStore().resetForConnectionSwitch();
  });

  // Load saved connections on store init
  loadSavedConnections();

  return {
    connections,
    activeConnectionId,
    activeConnection,
    connectedCount,
    statusBarConnections,
    lastError,
    addConnection,
    updateConnection,
    removeConnection,
    connect,
    disconnect,
    markConnectionLost,
    testConnection,
    testFormConnection,
    cancelConnect,
    cancelTest,
    cancelFormTest,
    switchDb,
    getActiveDb,
    togglePin,
    dismissSession,
    reorderSessions,
    loadSavedConnections,
  };
});
