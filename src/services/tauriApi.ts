import { invoke } from "@tauri-apps/api/core";
import type { SavedPipeline } from "@/types";

// ---- Connection guard (registered at app startup to avoid circular deps) ----
let _requireConnection: (() => void) | null = null;

/** Register a function that throws if no active connection. Call once at app startup. */
export function registerConnectionGuard(fn: () => void) {
  _requireConnection = fn;
}

/** Wrapper: validates connection before invoking IPC. Use for all connection-requiring calls. */
function withConn<T>(connectionId: string, fn: () => Promise<T>): Promise<T> {
  if (!connectionId) throw new Error("No active connection");
  if (_requireConnection) _requireConnection();
  return fn();
}

// ---- Rust-side response types (snake_case → camelCase via serde) ----

export interface RustConnectionConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  password: string;
  db: number;
  ssl: boolean;
  pinned: boolean;
  useSavedPassword?: boolean;
  keepPassword?: boolean;
}

export interface RustConnectionInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  db: number;
  ssl: boolean;
  status: string;
  pinned: boolean;
  hasPassword: boolean;
}

export interface RustRedisKeyInfo {
  key: string;
  keyType: string;
  ttl: number;
  size: number;
}

export interface RustKeyDetail {
  key: RustRedisKeyInfo;
  value: Record<string, unknown>;
  encoding: string;
}

export interface RustPipelineCommand {
  command: string;
  args: string[];
}

export interface RustPipelineResult {
  success: boolean;
  value: string;
  error: string | null;
  latencyMs: number;
}

export interface RustPipelineResponse {
  results: RustPipelineResult[];
  totalLatencyMs: number;
  individualSumMs: number;
}

export interface RustDiffEntry {
  path: string;
  keyType: string | null;
  before: string | null;
  after: string | null;
  beforeRaw: string | null;
  afterRaw: string | null;
  changeType: string;
}

export interface RustSandboxPreview {
  command: string;
  diff: RustDiffEntry[];
  commandResult: string | null;
  snapshotId: string;
  /** Original key types BEFORE command execution — for type-safe rollback */
  keyTypes: Record<string, string>;
  /** Precise inverse commands for rollback (computed by backend) */
  rollbackCommands: string[];
}

export interface RustServerMetrics {
  usedMemory: number;
  totalMemory: number;
  version: string;
  connectedClients: number;
  uptimeSeconds: number;
  usedCpuSys: number;
  usedCpuUser: number;
  keyspaceHits: number;
  keyspaceMisses: number;
  instantaneousOpsPerSec: number;
}

// ---- Tauri invoke wrappers ----

export const tauriApi = {
  connection: {
    connect: (config: RustConnectionConfig) =>
      invoke<RustConnectionInfo>("connect", { config }),

    disconnect: (id: string) =>
      invoke<void>("disconnect", { id }),

    testConnection: (config: RustConnectionConfig) =>
      invoke<boolean>("test_connection", { config }),

    getConnections: () =>
      invoke<RustConnectionInfo[]>("get_connections"),

    saveConnection: (config: RustConnectionConfig) =>
      invoke<void>("save_connection", { config }),

    deleteConnection: (id: string) =>
      invoke<void>("delete_connection", { id }),

    switchDb: (id: string, db: number) =>
      invoke<void>("switch_db", { id, db }),
  },

  cascade: {
    scanKeys: (connectionId: string, pattern: string, cursor: number, count: number) =>
      withConn(connectionId, () => invoke<[number, RustRedisKeyInfo[]]>("scan_keys", {
        connectionId,
        pattern,
        cursor,
        count,
      })),

    getKeyDetail: (connectionId: string, key: string, offset?: number, limit?: number, filter?: string, redisVersion?: string) =>
      withConn(connectionId, () => invoke<RustKeyDetail>("get_key_detail", { connectionId, key, offset, limit, filter, redisVersion })),

    deleteKey: (connectionId: string, key: string) =>
      withConn(connectionId, () => invoke<boolean>("delete_key", { connectionId, key })),

    setKeyTtl: (connectionId: string, key: string, ttl: number) =>
      withConn(connectionId, () => invoke<boolean>("set_key_ttl", { connectionId, key, ttl })),

    renameKey: (connectionId: string, oldKey: string, newKey: string) =>
      withConn(connectionId, () => invoke<boolean>("rename_key", { connectionId, oldKey, newKey })),

    dbSize: (connectionId: string) =>
      withConn(connectionId, () => invoke<number>("db_size", { connectionId })),

    setValue: (params: {
      connectionId: string;
      key: string;
      keyType: string;
      action: string;
      field?: string;
      value?: string;
      index?: number;
      score?: number;
      oldValue?: string;
    }) => withConn(params.connectionId, () => invoke<boolean>("set_value", params)),

    setHashFieldTtl: (connectionId: string, key: string, field: string, ttl: number) =>
      withConn(connectionId, () => invoke<boolean>("set_hash_field_ttl", { connectionId, key, field, ttl })),

    createKey: (params: {
      connectionId: string;
      key: string;
      keyType: string;
      ttl?: number;
      initialData?: any;
      fieldTtl?: number;
    }) => withConn(params.connectionId, () => invoke<boolean>("create_key", {
      connectionId: params.connectionId,
      key: params.key,
      keyType: params.keyType,
      ttl: params.ttl ?? null,
      initialData: params.initialData ?? null,
      fieldTtl: params.fieldTtl ?? null,
    })),

    batchAddFields: (params: {
      connectionId: string;
      key: string;
      keyType: string;
      items: any;
      fieldTtl?: number;
    }) => withConn(params.connectionId, () => invoke<boolean>("batch_add_fields", {
      connectionId: params.connectionId,
      key: params.key,
      keyType: params.keyType,
      items: params.items,
      fieldTtl: params.fieldTtl ?? null,
    })),
  },

  pipeline: {
    execute: (connectionId: string, commands: RustPipelineCommand[]) =>
      withConn(connectionId, () => invoke<RustPipelineResponse>("execute_pipeline", { connectionId, commands })),

    save: (id: string, name: string, commands: RustPipelineCommand[], createdAt: number) =>
      invoke<void>("save_pipeline", { id, name, commands, createdAt }),

    list: () =>
      invoke<SavedPipeline[]>("list_pipelines"),

    delete: (id: string) =>
      invoke<void>("delete_pipeline", { id }),
  },

  sandbox: {
    preview: (connectionId: string, command: string) =>
      withConn(connectionId, () => invoke<RustSandboxPreview>("sandbox_preview", { connectionId, command })),

    apply: (connectionId: string, command: string) =>
      withConn(connectionId, () => invoke<boolean>("sandbox_apply", { connectionId, command })),

    cancel: (connectionId: string) =>
      withConn(connectionId, () => invoke<boolean>("sandbox_cancel", { connectionId })),

    rollback: (connectionId: string, commands: string[]) =>
      withConn(connectionId, () => invoke<boolean>("sandbox_rollback", { connectionId, commands })),
  },

  metrics: {
    get: (connectionId: string) =>
      withConn(connectionId, () => invoke<RustServerMetrics>("get_metrics", { connectionId })),
  },
};
