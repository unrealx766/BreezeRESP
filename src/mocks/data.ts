import type {
  RedisConnection,
  RedisKey,
  KeyDetail,
  PipelineCommand,
  SandboxHistoryItem,
  MetricsData,
  QpsDataPoint,
} from "@/types";

// --- Connections ---
export const mockConnections: RedisConnection[] = [
  {
    id: "conn-1",
    name: "Production Redis",
    host: "192.168.1.100",
    port: 6379,
    password: "••••••••",
    db: 0,
    ssl: false,
    status: "connected",
    lastUsed: Date.now() - 300_000,
  },
  {
    id: "conn-2",
    name: "Staging Cache",
    host: "10.0.0.55",
    port: 6380,
    password: "",
    db: 0,
    ssl: true,
    status: "disconnected",
    lastUsed: Date.now() - 86400_000,
  },
  {
    id: "conn-3",
    name: "Local Dev",
    host: "127.0.0.1",
    port: 6379,
    password: "",
    db: 0,
    ssl: false,
    status: "connected",
    lastUsed: Date.now(),
  },
];

// --- Keys ---
const keyTypes = ["string", "hash", "list", "set", "zset"] as const;

export const mockKeys: RedisKey[] = [
  { key: "user:1001:name", type: "string", ttl: 3600, size: 48 },
  { key: "user:1001:email", type: "string", ttl: 3600, size: 64 },
  { key: "user:1001:profile", type: "hash", ttl: -1, size: 512 },
  { key: "user:1001:sessions", type: "set", ttl: 7200, size: 256 },
  { key: "user:1002:name", type: "string", ttl: -1, size: 52 },
  { key: "user:1002:profile", type: "hash", ttl: 86400, size: 480 },
  { key: "session:abc123", type: "hash", ttl: 1800, size: 320 },
  { key: "session:def456", type: "hash", ttl: 900, size: 288 },
  { key: "cache:product:list", type: "list", ttl: 300, size: 2048 },
  { key: "cache:product:detail:1001", type: "string", ttl: 600, size: 1024 },
  { key: "cache:product:detail:1002", type: "string", ttl: 580, size: 980 },
  { key: "queue:email:pending", type: "list", ttl: -1, size: 4096 },
  { key: "queue:email:processing", type: "list", ttl: -1, size: 1024 },
  { key: "leaderboard:global", type: "zset", ttl: -1, size: 8192 },
  { key: "leaderboard:weekly", type: "zset", ttl: 604800, size: 4096 },
  { key: "rate:limit:api:user1001", type: "string", ttl: 60, size: 16 },
  { key: "rate:limit:api:user1002", type: "string", ttl: 45, size: 16 },
  { key: "config:app:settings", type: "hash", ttl: -1, size: 256 },
  { key: "config:feature:flags", type: "hash", ttl: -1, size: 128 },
  { key: "analytics:daily:2025-01-15", type: "hash", ttl: 2592000, size: 512 },
  { key: "analytics:realtime", type: "hash", ttl: 300, size: 64 },
  { key: "lock:order:processing", type: "string", ttl: 30, size: 36 },
  { key: "counter:page:views", type: "string", ttl: -1, size: 24 },
  { key: "geo:stores:nearby", type: "zset", ttl: -1, size: 3072 },
  { key: "tags:popular", type: "set", ttl: -1, size: 512 },
  { key: "notification:user:1001", type: "list", ttl: 86400, size: 768 },
  { key: "token:refresh:user1001", type: "string", ttl: 2592000, size: 128 },
  { key: "inventory:sku:A001", type: "hash", ttl: -1, size: 192 },
  { key: "inventory:sku:B002", type: "hash", ttl: -1, size: 192 },
  { key: "cart:user:1001", type: "hash", ttl: 604800, size: 384 },
];

// --- Key Detail examples ---
export const mockKeyDetails: Record<string, KeyDetail> = {
  "user:1001:name": {
    key: mockKeys[0],
    value: {
      type: "string",
      value: "John Doe",
      encoding: "embstr",
    },
  },
  "user:1001:profile": {
    key: mockKeys[2],
    value: {
      type: "hash",
      fields: [
        { field: "name", value: "John Doe" },
        { field: "email", value: "john@example.com" },
        { field: "age", value: "32" },
        { field: "city", value: "San Francisco" },
        { field: "role", value: "admin" },
        { field: "created_at", value: "2024-03-15T10:30:00Z" },
      ],
      encoding: "ziplist",
    },
  },
  "user:1001:sessions": {
    key: mockKeys[3],
    value: {
      type: "set",
      members: [
        "sess_abc123",
        "sess_def456",
        "sess_ghi789",
        "sess_jkl012",
      ],
      encoding: "hashtable",
    },
  },
  "cache:product:list": {
    key: mockKeys[8],
    value: {
      type: "list",
      items: [
        '{"id":1001,"name":"Laptop","price":999}',
        '{"id":1002,"name":"Phone","price":699}',
        '{"id":1003,"name":"Tablet","price":499}',
        '{"id":1004,"name":"Watch","price":299}',
        '{"id":1005,"name":"Earbuds","price":149}',
      ],
      encoding: "quicklist",
    },
  },
  "leaderboard:global": {
    key: mockKeys[13],
    value: {
      type: "zset",
      members: [
        { member: "player:dragon", score: 98750 },
        { member: "player:phoenix", score: 87230 },
        { member: "player:tiger", score: 76100 },
        { member: "player:wolf", score: 65480 },
        { member: "player:eagle", score: 54320 },
        { member: "player:shark", score: 43210 },
        { member: "player:lion", score: 32100 },
        { member: "player:bear", score: 21980 },
      ],
      encoding: "skiplist",
    },
  },
};

// Provide a default detail for any key not in the map
export function getMockKeyDetail(key: string): KeyDetail {
  if (mockKeyDetails[key]) return mockKeyDetails[key];
  const redisKey = mockKeys.find((k) => k.key === key) ?? {
    key,
    type: "string" as const,
    ttl: -1,
    size: 64,
  };
  const type = redisKey.type;
  if (type === "string") {
    return { key: redisKey, value: { type: "string", value: `value_of_${key}`, encoding: "embstr" } };
  } else if (type === "hash") {
    return {
      key: redisKey,
      value: { type: "hash", fields: [{ field: "field1", value: "val1" }, { field: "field2", value: "val2" }], encoding: "ziplist" },
    };
  } else if (type === "list") {
    return { key: redisKey, value: { type: "list", items: ["item1", "item2", "item3"], encoding: "quicklist" } };
  } else if (type === "set") {
    return { key: redisKey, value: { type: "set", members: ["member1", "member2"], encoding: "hashtable" } };
  } else {
    return { key: redisKey, value: { type: "zset", members: [{ member: "m1", score: 1.5 }, { member: "m2", score: 2.0 }], encoding: "skiplist" } };
  }
}

// --- Pipeline ---
export const mockPipelineCommands: PipelineCommand[] = [
  { id: "cmd-1", command: "GET", args: ["user:1001:name"], result: { success: true, value: "John Doe", latencyMs: 0.3 } },
  { id: "cmd-2", command: "HGETALL", args: ["user:1001:profile"], result: { success: true, value: '{"name":"John Doe","email":"john@example.com"}', latencyMs: 0.5 } },
  { id: "cmd-3", command: "TTL", args: ["user:1001:name"], result: { success: true, value: "3600", latencyMs: 0.1 } },
  { id: "cmd-4", command: "SET", args: ["counter:test", "42"], result: undefined },
  { id: "cmd-5", command: "LRANGE", args: ["queue:email:pending", "0", "-1"], result: undefined },
];

// --- Sandbox ---
export const mockSandboxHistory: SandboxHistoryItem[] = [
  { id: "sb-1", snapshotId: "snap-1", command: "SET user:1001:name \"Jane Doe\"", timestamp: Date.now() - 120_000, status: "applied", diffCount: 1, beforeState: { "user:1001:name": "John Doe" }, addedKeys: [] },
  { id: "sb-2", snapshotId: "snap-2", command: "HSET user:1001:profile city \"New York\"", timestamp: Date.now() - 60_000, status: "preview", diffCount: 1, beforeState: {}, addedKeys: [] },
  { id: "sb-3", snapshotId: "snap-3", command: "DEL cache:product:detail:1001", timestamp: Date.now() - 30_000, status: "rolled-back", diffCount: 1, beforeState: { "cache:product:detail:1001": "cached_data" }, addedKeys: [] },
  { id: "sb-4", snapshotId: "snap-4", command: "LPUSH queue:email:pending \"new_job\"", timestamp: Date.now() - 10_000, status: "preview", diffCount: 2, beforeState: {}, addedKeys: ["queue:email:pending"] },
];

// --- Metrics ---
function generateQpsHistory(): QpsDataPoint[] {
  const now = Date.now();
  const points: QpsDataPoint[] = [];
  for (let i = 59; i >= 0; i--) {
    points.push({
      timestamp: now - i * 1000,
      value: Math.floor(800 + Math.random() * 400 + Math.sin(i / 10) * 200),
    });
  }
  return points;
}

export const mockMetrics: MetricsData = {
  qps: 1024,
  qpsHistory: generateQpsHistory(),
  memoryUsed: 2_097_152, // 2MB
  memoryTotal: 1_073_741_824, // 1GB
  version: "7.2.4",
  connectedClients: 48,
  uptimeSeconds: 86400 * 7 + 3600 * 5, // 7 days 5 hours
  usedCpuSys: 12.5,
  usedCpuUser: 8.3,
  keyspaceHits: 1_234_567,
  keyspaceMisses: 12_345,
};

// Suppress unused import warnings
void keyTypes;
