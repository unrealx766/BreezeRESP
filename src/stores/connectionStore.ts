import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { RedisConnection, ConnectionStatus } from "@/types";
import { tauriApi, type RustConnectionConfig } from "@/services/tauriApi";
import { toast } from "@/utils/toast";
import { i18n } from "@/i18n";
import { useCascadeStore } from "@/stores/cascadeStore";
import { useDetailStore } from "@/stores/detailStore";

export const useConnectionStore = defineStore("connection", () => {
  const connections = ref<RedisConnection[]>([]);
  const activeConnectionId = ref<string | null>(null);
  const lastError = ref<string | null>(null);
  /** IDs of connections that have been connected during this session (persists until app exit) */
  const sessionConnectedIds = ref<Set<string>>(new Set());

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
  const statusBarConnections = computed(() =>
    connections.value.filter(
      (c) => c.pinned || c.status === "connected" || sessionConnectedIds.value.has(c.id)
    )
  );

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
    if (activeConnectionId.value === id) activeConnectionId.value = null;
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
      conn.lastUsed = Date.now();
      sessionConnectedIds.value = new Set([...sessionConnectedIds.value, id]);
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

  /** Switch the active database for a connected session */
  async function switchDb(db: number) {
    const id = activeConnectionId.value;
    if (!id) return;

    try {
      await tauriApi.connection.switchDb(id, db);
      const conn = connections.value.find((c) => c.id === id);
      if (conn) conn.db = db;
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
    togglePin,
    dismissSession,
    loadSavedConnections,
  };
});
