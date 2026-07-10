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
  { label: "GET", cmd: "GET", args: ["key"] },
  { label: "SET", cmd: "SET", args: ["key", "value"] },
  { label: "HGETALL", cmd: "HGETALL", args: ["key"] },
  { label: "HSET", cmd: "HSET", args: ["key", "field", "value"] },
  { label: "HDEL", cmd: "HDEL", args: ["key", "field"] },
  { label: "LPUSH", cmd: "LPUSH", args: ["key", "value"] },
  { label: "LRANGE", cmd: "LRANGE", args: ["key", "0", "-1"] },
  { label: "SADD", cmd: "SADD", args: ["key", "member"] },
  { label: "SREM", cmd: "SREM", args: ["key", "member"] },
  { label: "SMEMBERS", cmd: "SMEMBERS", args: ["key"] },
  { label: "ZADD", cmd: "ZADD", args: ["key", "1", "member"] },
  { label: "ZREM", cmd: "ZREM", args: ["key", "member"] },
  { label: "ZRANGE", cmd: "ZRANGE", args: ["key", "0", "-1", "WITHSCORES"] },
  { label: "DEL", cmd: "DEL", args: ["key"] },
  { label: "INCR", cmd: "INCR", args: ["key"] },
  { label: "EXPIRE", cmd: "EXPIRE", args: ["key", "60"] },
  { label: "RENAME", cmd: "RENAME", args: ["key", "newkey"] },
  { label: "TTL", cmd: "TTL", args: ["key"] },
  { label: "EXISTS", cmd: "EXISTS", args: ["key"] },
  { label: "INFO", cmd: "INFO", args: [] },
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
