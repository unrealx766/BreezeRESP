import { invoke } from "@tauri-apps/api/core";
import type { SavedPipeline } from "@/types";

// ---- Rust-side response types (snake_case → camelCase via serde) ----

export interface RustConnectionConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  password: string;
  db: number;
  ssl: boolean;
}

export interface RustConnectionInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  db: number;
  ssl: boolean;
  status: string;
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
  before: string | null;
  after: string | null;
  changeType: string;
}

export interface RustSandboxPreview {
  command: string;
  diff: RustDiffEntry[];
  snapshotId: string;
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
      invoke<[number, RustRedisKeyInfo[]]>("scan_keys", {
        connectionId,
        pattern,
        cursor,
        count,
      }),

    getKeyDetail: (connectionId: string, key: string) =>
      invoke<RustKeyDetail>("get_key_detail", { connectionId, key }),

    deleteKey: (connectionId: string, key: string) =>
      invoke<boolean>("delete_key", { connectionId, key }),

    setKeyTtl: (connectionId: string, key: string, ttl: number) =>
      invoke<boolean>("set_key_ttl", { connectionId, key, ttl }),

    renameKey: (connectionId: string, oldKey: string, newKey: string) =>
      invoke<boolean>("rename_key", { connectionId, oldKey, newKey }),

    dbSize: (connectionId: string) =>
      invoke<number>("db_size", { connectionId }),

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
    }) => invoke<boolean>("set_value", params),
  },

  pipeline: {
    execute: (connectionId: string, commands: RustPipelineCommand[]) =>
      invoke<RustPipelineResponse>("execute_pipeline", { connectionId, commands }),

    save: (id: string, name: string, commands: RustPipelineCommand[], createdAt: number) =>
      invoke<void>("save_pipeline", { id, name, commands, createdAt }),

    list: () =>
      invoke<SavedPipeline[]>("list_pipelines"),

    delete: (id: string) =>
      invoke<void>("delete_pipeline", { id }),
  },

  sandbox: {
    preview: (connectionId: string, command: string) =>
      invoke<RustSandboxPreview>("sandbox_preview", { connectionId, command }),

    apply: (connectionId: string, command: string) =>
      invoke<boolean>("sandbox_apply", { connectionId, command }),

    rollback: (connectionId: string, beforeState: Record<string, string>, addedKeys: string[]) =>
      invoke<boolean>("sandbox_rollback", { connectionId, beforeState, addedKeys }),
  },

  metrics: {
    get: (connectionId: string) =>
      invoke<RustServerMetrics>("get_metrics", { connectionId }),
  },
};
