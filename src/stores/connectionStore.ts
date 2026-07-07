import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { RedisConnection, ConnectionStatus } from "@/types";
import { tauriApi, type RustConnectionConfig } from "@/services/tauriApi";

export const useConnectionStore = defineStore("connection", () => {
  const connections = ref<RedisConnection[]>([]);
  const activeConnectionId = ref<string | null>(null);
  const lastError = ref<string | null>(null);

  const activeConnection = computed(() =>
    connections.value.find((c) => c.id === activeConnectionId.value) ?? null
  );

  const connectedCount = computed(
    () => connections.value.filter((c) => c.status === "connected").length
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
      }));
    } catch (e) {
      console.error("Failed to load saved connections:", e);
    }
  }

  /** Build a Rust-side config from a local connection */
  function toRustConfig(conn: RedisConnection): RustConnectionConfig {
    return {
      id: conn.id,
      name: conn.name,
      host: conn.host,
      port: conn.port,
      password: conn.password,
      db: conn.db,
      ssl: conn.ssl,
    };
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

  async function updateConnection(id: string, patch: Partial<RedisConnection>) {
    const idx = connections.value.findIndex((c) => c.id === id);
    if (idx !== -1) {
      // If password is empty in patch, keep the existing password (backend doesn't return it)
      if (patch.password === "" && connections.value[idx].password) {
        patch = { ...patch, password: connections.value[idx].password };
      }
      connections.value[idx] = { ...connections.value[idx], ...patch };
      // Persist to disk
      try {
        await tauriApi.connection.saveConnection(toRustConfig(connections.value[idx]));
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
    }
  }

  async function connect(id: string): Promise<boolean> {
    const conn = connections.value.find((c) => c.id === id);
    if (!conn) return false;

    lastError.value = null;
    setStatus(id, "connecting");
    try {
      await tauriApi.connection.connect(toRustConfig(conn));
      setStatus(id, "connected");
      activeConnectionId.value = id;
      conn.lastUsed = Date.now();
      return true;
    } catch (e) {
      console.error("Connect failed:", e);
      setStatus(id, "error");
      lastError.value = typeof e === "string" ? e : (e as Error)?.message || String(e);
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
    if (activeConnectionId.value === id) activeConnectionId.value = null;
  }

  async function testConnection(id: string): Promise<boolean> {
    const conn = connections.value.find((c) => c.id === id);
    if (!conn) return false;

    setStatus(id, "connecting");
    try {
      const result = await tauriApi.connection.testConnection(toRustConfig(conn));
      setStatus(id, result ? "connected" : "error");
      // Revert to disconnected since test doesn't maintain connection
      if (result) setStatus(id, "disconnected");
      return result;
    } catch (e) {
      console.error("Test connection failed:", e);
      setStatus(id, "error");
      return false;
    }
  }

  /** Test a connection from form data without saving it */
  async function testFormConnection(config: Omit<RedisConnection, "id" | "status">): Promise<boolean> {
    const tempConfig: RustConnectionConfig = {
      id: `__form_test_${Date.now()}`,
      name: config.name,
      host: config.host,
      port: config.port,
      password: config.password,
      db: config.db,
      ssl: config.ssl,
    };
    try {
      return await tauriApi.connection.testConnection(tempConfig);
    } catch (e) {
      console.error("Form test connection failed:", e);
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

  // Load saved connections on store init
  loadSavedConnections();

  return {
    connections,
    activeConnectionId,
    activeConnection,
    connectedCount,
    lastError,
    addConnection,
    updateConnection,
    removeConnection,
    connect,
    disconnect,
    markConnectionLost,
    testConnection,
    testFormConnection,
    switchDb,
    loadSavedConnections,
  };
});
