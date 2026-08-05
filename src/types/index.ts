// Redis data types
export type RedisDataType = "string" | "hash" | "list" | "set" | "zset" | "stream" | "rejson";

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface RedisConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  password: string;
  db: number;
  ssl: boolean;
  cluster?: boolean;
  nodes?: string[];
  status: ConnectionStatus;
  lastUsed?: number;
  pinned?: boolean;
  hasPassword?: boolean;
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
  /** Hex-encoded raw bytes for binary-safe multi-view display */
  valueHex: string;
  encoding: string;
  contentEncoding?: string;
}

export interface HashValue {
  type: "hash";
  fields: Array<{ field: string; fieldHex?: string; value: string; valueHex?: string; ttl?: number }>;
  encoding: string;
  contentEncoding?: string;
  totalCount?: number;
  truncated?: boolean;
  /** True when Redis >= 7.4.0 returned per-field TTLs (HTTL supported) */
  hasFieldTtl?: boolean;
}

export interface ListValue {
  type: "list";
  items: string[];
  /** Hex-encoded raw bytes for each item (parallel to items[]) */
  itemsHex?: string[];
  encoding: string;
  contentEncoding?: string;
  totalCount?: number;
  truncated?: boolean;
  /** Original Redis indices for filtered results (needed for correct LSET) */
  originalIndices?: number[];
}

export interface SetValue {
  type: "set";
  members: string[];
  /** Hex-encoded raw bytes for each member (parallel to members[]) */
  membersHex?: string[];
  encoding: string;
  contentEncoding?: string;
  totalCount?: number;
  truncated?: boolean;
}

export interface ZSetValue {
  type: "zset";
  members: Array<{ member: string; memberHex?: string; score: number }>;
  encoding: string;
  contentEncoding?: string;
  totalCount?: number;
  truncated?: boolean;
}

export interface StreamValue {
  type: "stream";
  length: number;
  lastGeneratedId: string;
  groups: number;
  /** Preview of the first entries */
  entries: StreamEntry[];
  totalCount?: number;
  truncated?: boolean;
}

export interface ReJSONValue {
  type: "rejson";
  /** Raw JSON document (JSON.GET $) */
  json: string;
}

export type KeyValue = StringValue | HashValue | ListValue | SetValue | ZSetValue | StreamValue | ReJSONValue;

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

export interface SavedPipeline {
  id: string;
  name: string;
  commands: { command: string; args: string[] }[];
  createdAt: number;
}

// Sandbox
export type DiffChangeType = "added" | "modified" | "deleted" | "unchanged";

export interface DiffEntry {
  path: string;
  keyType: string | null;
  before: string | null;
  after: string | null;
  /** Raw JSON-serialized value for rollback (not display-formatted) */
  beforeRaw: string | null;
  /** Raw JSON-serialized value for rollback (not display-formatted) */
  afterRaw: string | null;
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
  /** Precise inverse Redis commands for rollback */
  rollbackCommands: string[];
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

// Command History
export type CommandSource = "browser" | "pipeline" | "sandbox";

export interface CommandHistoryItem {
  id: string;
  connectionId: string;
  connectionName: string;
  db: number;
  command: string;
  source: CommandSource;
  success: boolean;
  error?: string;
  timestamp: number;
  /** Execution duration in milliseconds */
  durationMs?: number;
}

// UI types
export type ViewMode = "tree" | "list";
export type SortField = "name" | "type" | "ttl" | "size";
export type SortOrder = "asc" | "desc";

// Slowlog
export interface SlowlogEntry {
  id: number;
  timestamp: number;
  durationUs: number;
  command: string;
  argsCount: number;
  clientAddr: string | null;
  clientName: string | null;
}

export interface SlowlogInfo {
  entries: SlowlogEntry[];
  totalLen: number;
  slowlogLogSlowerThan: number;
}

// Server capability (Redis version + module support)
export interface ServerCapability {
  redisVersion: string;
  redisMode: string;
  streamsSupported: boolean;
  streamFullSupported: boolean;
  streamExtendedSupported: boolean;
  jsonSupported: boolean;
  jsonVersion: string | null;
  searchSupported: boolean;
  searchVersion: string | null;
  vectorSearchSupported: boolean;
}

// Streams
export interface StreamEntry {
  id: string;
  fields: Array<[string, string]>;
}

export interface StreamInfo {
  key: string;
  length: number;
  lastGeneratedId: string;
  groups: number;
  firstEntry: StreamEntry | null;
  lastEntry: StreamEntry | null;
  maxDeletedEntryId: string | null;
  entriesAdded: number | null;
  radixTreeKeys: number | null;
  radixTreeNodes: number | null;
}

export interface ConsumerGroup {
  name: string;
  consumers: number;
  pending: number;
  lastDeliveredId: string;
  lag: number | null;
  entriesRead: number | null;
}

export interface ConsumerInfo {
  name: string;
  pending: number;
  idleMs: number;
  inactiveMs: number | null;
}

export interface PendingEntry {
  id: string;
  consumer: string;
  idleMs: number;
  deliveredCount: number;
}

// RedisJSON & RediSearch
export interface FtField {
  identifier: string;
  attribute: string;
  fieldType: string;
  vectorAlgorithm: string | null;
  vectorDim: number | null;
  vectorDistanceMetric: string | null;
  vectorDataType: string | null;
}

export interface FtIndexInfo {
  name: string;
  numDocs: number;
  fields: FtField[];
  prefixes: string[];
}

export interface FtDocument {
  id: string;
  score: number | null;
  fields: Array<[string, string]>;
}

export interface FtSearchResult {
  total: number;
  docs: FtDocument[];
  elapsedMs: number | null;
}

export interface FtFieldSpec {
  identifier: string;
  attribute?: string | null;
  fieldType: string;
  separator?: string | null;
  vectorAlgorithm?: string | null;
  vectorDim?: number | null;
  vectorDistanceMetric?: string | null;
  vectorDataType?: string | null;
}

export interface FtCreateSpec {
  name: string;
  onType?: string | null;
  prefixes: string[];
  fields: FtFieldSpec[];
}
