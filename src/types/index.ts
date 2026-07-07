// Redis data types
export type RedisDataType = "string" | "hash" | "list" | "set" | "zset";

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface RedisConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  password: string;
  db: number;
  ssl: boolean;
  status: ConnectionStatus;
  lastUsed?: number;
}

export interface RedisKey {
  key: string;
  type: RedisDataType;
  ttl: number; // -1 = no expiry, -2 = missing, >0 = seconds
  size: number; // bytes
}

export interface StringValue {
  type: "string";
  value: string;
  encoding: string;
}

export interface HashValue {
  type: "hash";
  fields: Array<{ field: string; value: string }>;
  encoding: string;
}

export interface ListValue {
  type: "list";
  items: string[];
  encoding: string;
}

export interface SetValue {
  type: "set";
  members: string[];
  encoding: string;
}

export interface ZSetValue {
  type: "zset";
  members: Array<{ member: string; score: number }>;
  encoding: string;
}

export type KeyValue = StringValue | HashValue | ListValue | SetValue | ZSetValue;

export interface KeyDetail {
  key: RedisKey;
  value: KeyValue;
}

// Pipeline
export interface PipelineCommand {
  id: string;
  command: string;
  args: string[];
  result?: PipelineResult;
}

export interface PipelineResult {
  success: boolean;
  value: string;
  error?: string;
  latencyMs: number;
}

// Sandbox
export type DiffChangeType = "added" | "modified" | "deleted";

export interface DiffEntry {
  path: string;
  before: string | null;
  after: string | null;
  changeType: DiffChangeType;
}

export interface SandboxSnapshot {
  id: string;
  timestamp: number;
  command: string;
  diff: DiffEntry[];
  status: "pending" | "applied" | "rolled-back";
}

export interface SandboxHistoryItem {
  id: string;
  snapshotId: string;
  command: string;
  timestamp: number;
  status: "preview" | "applied" | "rolled-back";
  diffCount: number;
  beforeState: Record<string, string>;
  addedKeys: string[];
}

// Metrics
export interface QpsDataPoint {
  timestamp: number;
  value: number;
}

export interface MetricsData {
  qps: number;
  qpsHistory: QpsDataPoint[];
  memoryUsed: number; // bytes
  memoryTotal: number;
  version: string;
  connectedClients: number;
  uptimeSeconds: number;
  usedCpuSys: number;
  usedCpuUser: number;
  keyspaceHits: number;
  keyspaceMisses: number;
}

// Key tree for cascade browsing
export interface KeyTreeNode {
  label: string;
  fullPath: string;
  children: KeyTreeNode[];
  key?: RedisKey; // leaf node has actual key
  expanded: boolean;
}

// UI types
export type ViewMode = "tree" | "list";
export type SortField = "name" | "type" | "ttl" | "size";
export type SortOrder = "asc" | "desc";
