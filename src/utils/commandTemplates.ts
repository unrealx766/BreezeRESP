/**
 * Shared Redis command templates used by Pipeline & Sandbox pages.
 * `cmd` + `args` keep structure; consumers join them as needed.
 */
export interface CommandTemplate {
  label: string;
  cmd: string;
  args: string[];
}

/** All common Redis command templates (read + write). */
export const allCommandTemplates: CommandTemplate[] = [
  // ── String (read) ──────────────────────────────────────────
  { label: "GET", cmd: "GET", args: ["key"] },
  // ── String (write) ─────────────────────────────────────────
  { label: "SET", cmd: "SET", args: ["key", "value"] },
  { label: "SETNX", cmd: "SETNX", args: ["key", "value"] },
  { label: "GETSET", cmd: "GETSET", args: ["key", "value"] },
  { label: "SETEX", cmd: "SETEX", args: ["key", "60", "value"] },
  { label: "PSETEX", cmd: "PSETEX", args: ["key", "60000", "value"] },
  { label: "APPEND", cmd: "APPEND", args: ["key", "suffix"] },
  { label: "INCR", cmd: "INCR", args: ["key"] },
  { label: "DECR", cmd: "DECR", args: ["key"] },
  { label: "INCRBY", cmd: "INCRBY", args: ["key", "10"] },
  { label: "DECRBY", cmd: "DECRBY", args: ["key", "10"] },
  { label: "MSET", cmd: "MSET", args: ["key1", "value1", "key2", "value2"] },
  // ── Hash (read) ────────────────────────────────────────────
  { label: "HGETALL", cmd: "HGETALL", args: ["key"] },
  // ── Hash (write) ───────────────────────────────────────────
  { label: "HSET", cmd: "HSET", args: ["key", "field", "value"] },
  { label: "HMSET", cmd: "HMSET", args: ["key", "field1", "value1", "field2", "value2"] },
  { label: "HDEL", cmd: "HDEL", args: ["key", "field"] },
  { label: "HINCRBY", cmd: "HINCRBY", args: ["key", "field", "1"] },
  // ── List (read) ────────────────────────────────────────────
  { label: "LRANGE", cmd: "LRANGE", args: ["key", "0", "-1"] },
  // ── List (write) ───────────────────────────────────────────
  { label: "LPUSH", cmd: "LPUSH", args: ["key", "value"] },
  { label: "RPUSH", cmd: "RPUSH", args: ["key", "value"] },
  { label: "LPOP", cmd: "LPOP", args: ["key"] },
  { label: "RPOP", cmd: "RPOP", args: ["key"] },
  // ── Set (read) ─────────────────────────────────────────────
  { label: "SMEMBERS", cmd: "SMEMBERS", args: ["key"] },
  // ── Set (write) ────────────────────────────────────────────
  { label: "SADD", cmd: "SADD", args: ["key", "member"] },
  { label: "SREM", cmd: "SREM", args: ["key", "member"] },
  // ── Sorted Set (read) ──────────────────────────────────────
  { label: "ZRANGE", cmd: "ZRANGE", args: ["key", "0", "-1", "WITHSCORES"] },
  // ── Sorted Set (write) ─────────────────────────────────────
  { label: "ZADD", cmd: "ZADD", args: ["key", "1", "member"] },
  { label: "ZREM", cmd: "ZREM", args: ["key", "member"] },
  // ── Key-level (read) ───────────────────────────────────────
  { label: "TTL", cmd: "TTL", args: ["key"] },
  { label: "EXISTS", cmd: "EXISTS", args: ["key"] },
  { label: "INFO", cmd: "INFO", args: [] },
  // ── Key-level (write) ──────────────────────────────────────
  { label: "DEL", cmd: "DEL", args: ["key"] },
  { label: "EXPIRE", cmd: "EXPIRE", args: ["key", "60"] },
  { label: "PERSIST", cmd: "PERSIST", args: ["key"] },
  { label: "RENAME", cmd: "RENAME", args: ["key", "newkey"] },
  { label: "RENAMENX", cmd: "RENAMENX", args: ["key", "newkey"] },
];

/** Read-only commands that don't modify data. */
const READ_ONLY_LABELS = new Set([
  "GET", "HGETALL", "LRANGE", "SMEMBERS", "ZRANGE",
  "TTL", "EXISTS", "INFO",
]);

/** Write/mutation command templates only (for sandbox diff preview). */
export const writeCommandTemplates: CommandTemplate[] = allCommandTemplates.filter(
  (t) => !READ_ONLY_LABELS.has(t.label)
);
