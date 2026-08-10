import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  SavedPipeline,
  ServerCapability,
  StreamEntry,
  StreamInfo,
  ConsumerGroup,
  ConsumerInfo,
  PendingEntry,
  FtIndexInfo,
  FtSearchResult,
  FtCreateSpec,
} from "@/types";

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
  cluster: boolean;
  nodes: string[];
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
  cluster: boolean;
  nodes: string[];
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

export interface PubSubMessage {
  channel: string;
  message: string;
  timestamp: number;
}

export interface RustSlowlogEntry {
  id: number;
  timestamp: number;
  durationUs: number;
  command: string;
  argsCount: number;
  clientAddr: string | null;
  clientName: string | null;
}

export interface RustSlowlogInfo {
  entries: RustSlowlogEntry[];
  totalLen: number;
  slowlogLogSlowerThan: number;
}

/** Real-time message pushed from the backend `pubsub-message` event. */
export interface PubSubEvent {
  connectionId: string;
  channel: string;
  message: string;
  timestamp: number;
  /** The glob pattern matched, when delivered via a pattern subscription. */
  pattern?: string | null;
}

/** Full subscription state (exact channels + glob patterns) for a connection. */
export interface SubscriptionState {
  channels: string[];
  patterns: string[];
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
      streamId?: string;
    }) => withConn(params.connectionId, () => invoke<boolean>("create_key", {
      connectionId: params.connectionId,
      key: params.key,
      keyType: params.keyType,
      ttl: params.ttl ?? null,
      initialData: params.initialData ?? null,
      fieldTtl: params.fieldTtl ?? null,
      streamId: params.streamId ?? null,
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

  slowlog: {
    get: (connectionId: string, count?: number) =>
      withConn(connectionId, () => invoke<RustSlowlogInfo>("get_slowlog", { connectionId, count: count ?? null })),

    saveExport: (content: string, filename: string) =>
      invoke<string>("save_slowlog_export", { content, filename }),

    openFileLocation: (path: string) =>
      invoke<void>("open_file_location", { path }),
  },

  pubsub: {
    publish: (connectionId: string, channel: string, message: string) =>
      withConn(connectionId, () => invoke<number>("pubsub_publish", { connectionId, channel, message })),

    subscribe: (connectionId: string, channel: string, isPattern = false) =>
      withConn(connectionId, () => invoke<SubscriptionState>("pubsub_subscribe", { connectionId, channel, isPattern })),

    unsubscribe: (connectionId: string, channel?: string, isPattern = false) =>
      withConn(connectionId, () => invoke<SubscriptionState>("pubsub_unsubscribe", { connectionId, channel: channel ?? null, isPattern })),

    listChannels: (connectionId: string, pattern?: string) =>
      withConn(connectionId, () => invoke<string[]>("pubsub_list_channels", { connectionId, pattern: pattern ?? null })),

    numSubs: (connectionId: string, channel: string) =>
      withConn(connectionId, () => invoke<number>("pubsub_num_subs", { connectionId, channel })),

    /** Listen for real-time messages. Returns an unlisten function. */
    onMessage: (handler: (msg: PubSubEvent) => void): Promise<UnlistenFn> =>
      listen<PubSubEvent>("pubsub-message", (event) => handler(event.payload)),
  },

  capability: {
    get: (connectionId: string, force = false) =>
      withConn(connectionId, () =>
        invoke<ServerCapability>("get_server_capability", { connectionId, force })),
  },

  streams: {
    list: (connectionId: string, pattern?: string, limit?: number) =>
      withConn(connectionId, () =>
        invoke<string[]>("list_streams", { connectionId, pattern: pattern ?? null, limit: limit ?? null })),

    getInfo: (connectionId: string, key: string) =>
      withConn(connectionId, () => invoke<StreamInfo>("get_stream_info", { connectionId, key })),

    getEntries: (connectionId: string, key: string, start?: string, end?: string, count?: number) =>
      withConn(connectionId, () =>
        invoke<StreamEntry[]>("get_stream_entries", {
          connectionId, key,
          start: start ?? null, end: end ?? null, count: count ?? null,
        })),

    getGroups: (connectionId: string, key: string) =>
      withConn(connectionId, () => invoke<ConsumerGroup[]>("get_stream_groups", { connectionId, key })),

    getConsumers: (connectionId: string, key: string, group: string) =>
      withConn(connectionId, () => invoke<ConsumerInfo[]>("get_stream_consumers", { connectionId, key, group })),

    getPending: (connectionId: string, key: string, group: string, count?: number) =>
      withConn(connectionId, () =>
        invoke<PendingEntry[]>("get_pending_entries", { connectionId, key, group, count: count ?? null })),

    addMessage: (connectionId: string, key: string, id: string | null, fields: Array<[string, string]>) =>
      withConn(connectionId, () => invoke<string>("stream_add_message", { connectionId, key, id, fields })),

    trim: (connectionId: string, key: string, maxLen: number, approximate = true) =>
      withConn(connectionId, () => invoke<number>("stream_trim", { connectionId, key, maxLen, approximate })),

    deleteEntries: (connectionId: string, key: string, ids: string[]) =>
      withConn(connectionId, () => invoke<number>("stream_delete_entries", { connectionId, key, ids })),

    ack: (connectionId: string, key: string, group: string, ids: string[]) =>
      withConn(connectionId, () => invoke<number>("stream_ack", { connectionId, key, group, ids })),

    deleteConsumer: (connectionId: string, key: string, group: string, consumer: string) =>
      withConn(connectionId, () => invoke<number>("stream_delete_consumer", { connectionId, key, group, consumer })),

    deleteGroup: (connectionId: string, key: string, group: string) =>
      withConn(connectionId, () => invoke<boolean>("stream_delete_group", { connectionId, key, group })),

    claim: (connectionId: string, key: string, group: string, consumer: string, minIdleMs: number, ids: string[]) =>
      withConn(connectionId, () =>
        invoke<StreamEntry[]>("stream_claim", { connectionId, key, group, consumer, minIdleMs, ids })),
  },

  jsonsearch: {
    jsonGet: (connectionId: string, key: string, path?: string) =>
      withConn(connectionId, () => invoke<string>("json_get", { connectionId, key, path: path ?? null })),

    jsonSet: (connectionId: string, key: string, path: string, value: string) =>
      withConn(connectionId, () => invoke<boolean>("json_set", { connectionId, key, path, value })),

    jsonDel: (connectionId: string, key: string, path?: string) =>
      withConn(connectionId, () => invoke<number>("json_del", { connectionId, key, path: path ?? null })),

    jsonType: (connectionId: string, key: string, path?: string) =>
      withConn(connectionId, () => invoke<string>("json_type", { connectionId, key, path: path ?? null })),

    ftList: (connectionId: string) =>
      withConn(connectionId, () => invoke<string[]>("ft_list", { connectionId })),

    ftInfo: (connectionId: string, index: string) =>
      withConn(connectionId, () => invoke<FtIndexInfo>("ft_info", { connectionId, index })),

    ftSearch: (params: {
      connectionId: string;
      index: string;
      query: string;
      offset?: number;
      limit?: number;
      // Param values are raw byte arrays (number[]) so binary payloads
      // (e.g. FLOAT32 KNN vectors) survive the JSON IPC bridge losslessly.
      params?: Array<[string, number[]]>;
      withScores?: boolean;
    }) => withConn(params.connectionId, () =>
      invoke<FtSearchResult>("ft_search", {
        connectionId: params.connectionId,
        index: params.index,
        query: params.query,
        offset: params.offset ?? null,
        limit: params.limit ?? null,
        params: params.params ?? null,
        withScores: params.withScores ?? null,
      })),

    ftCreate: (connectionId: string, spec: FtCreateSpec) =>
      withConn(connectionId, () => invoke<boolean>("ft_create", { connectionId, spec })),

    ftDropIndex: (connectionId: string, index: string, deleteDocs = false) =>
      withConn(connectionId, () => invoke<boolean>("ft_drop_index", { connectionId, index, deleteDocs })),
  },
};
