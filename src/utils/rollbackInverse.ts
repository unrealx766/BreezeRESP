import type { DiffEntry } from "@/types";

/**
 * Compute precise inverse Redis commands for rollback.
 *
 * Instead of deleting and rebuilding entire keys, this generates
 * minimal commands that only reverse the actual changes made.
 *
 * @param cmd        The original command string, e.g. "HSET myhash f3 v3"
 * @param diff       The diff entries from the sandbox preview
 * @param keyTypes   Original key types BEFORE command execution
 * @returns Array of Redis command strings that, when executed, undo the original command
 */
export function computeInverseCommands(
  cmd: string,
  diff: DiffEntry[],
  keyTypes: Record<string, string>,
): string[] {
  const parts = parseCommandParts(cmd);
  if (parts.length === 0) return [];

  const cmdName = parts[0].toUpperCase();
  const args = parts.slice(1);
  const inverseCmds: string[] = [];

  // Helper: find the diff entry for a given key
  const findDiff = (key: string): DiffEntry | undefined =>
    diff.find((d) => d.path === key);

  // Helper: quote a value if it contains spaces
  const q = (val: string): string =>
    val.includes(" ") || val.includes('"') || val === ""
      ? `"${val.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`
      : val;

  switch (cmdName) {
    // ── String commands ──────────────────────────────────────────
    case "SET":
    case "SETNX":
    case "GETSET": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else if (entry.changeType === "modified" || entry.changeType === "unchanged") {
        // Restore original value
        if (entry.beforeRaw != null) {
          inverseCmds.push(`SET ${q(key)} ${q(entry.beforeRaw)}`);
        }
      }
      break;
    }

    case "SETEX": {
      const key = args[0];
      const seconds = args[1];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else if (entry.beforeRaw != null) {
        inverseCmds.push(`SETEX ${q(key)} ${seconds} ${q(entry.beforeRaw)}`);
      }
      break;
    }

    case "PSETEX": {
      const key = args[0];
      const ms = args[1];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else if (entry.beforeRaw != null) {
        inverseCmds.push(`PSETEX ${q(key)} ${ms} ${q(entry.beforeRaw)}`);
      }
      break;
    }

    case "APPEND": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else if (entry.beforeRaw != null) {
        inverseCmds.push(`SET ${q(key)} ${q(entry.beforeRaw)}`);
      }
      break;
    }

    case "INCR": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        inverseCmds.push(`DECRBY ${q(key)} 1`);
      }
      break;
    }

    case "DECR": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        inverseCmds.push(`INCRBY ${q(key)} 1`);
      }
      break;
    }

    case "INCRBY": {
      const key = args[0];
      const delta = args[1];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        inverseCmds.push(`DECRBY ${q(key)} ${delta}`);
      }
      break;
    }

    case "DECRBY": {
      const key = args[0];
      const delta = args[1];
      const entry = findDiff(key);
      if (!entry) break;
      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        inverseCmds.push(`INCRBY ${q(key)} ${delta}`);
      }
      break;
    }

    // ── Hash commands ────────────────────────────────────────────
    case "HSET":
    case "HMSET": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        // Entire key was new → DEL
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        // Key existed: compute field-level diff
        const beforeFields = parseHashFields(entry.beforeRaw);
        const afterFields = parseHashFields(entry.afterRaw);

        const beforeMap = new Map(beforeFields);
        const afterMap = new Map(afterFields);

        // Fields added or modified by the command
        const fieldArgs = args.slice(1);
        for (let i = 0; i + 1 < fieldArgs.length; i += 2) {
          const field = fieldArgs[i];
          const beforeVal = beforeMap.get(field);
          if (beforeVal === undefined) {
            // Field was added → HDEL
            inverseCmds.push(`HDEL ${q(key)} ${q(field)}`);
          } else {
            // Field was modified → HSET old value
            inverseCmds.push(`HSET ${q(key)} ${q(field)} ${q(beforeVal)}`);
          }
        }

        // If the key ends up empty after all HDELs, we may need cleanup,
        // but Redis automatically removes empty hashes.
        void afterMap; // afterMap used only for completeness
      }
      break;
    }

    case "HDEL": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "deleted") {
        // Entire key was deleted (all fields removed) → rebuild
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, keyTypes[key] ?? "hash"));
      } else {
        // Some fields remain; restore deleted fields from beforeRaw
        const beforeFields = parseHashFields(entry.beforeRaw);
        const beforeMap = new Map(beforeFields);
        const deletedFields = args.slice(1);
        for (const field of deletedFields) {
          const oldVal = beforeMap.get(field);
          if (oldVal !== undefined) {
            inverseCmds.push(`HSET ${q(key)} ${q(field)} ${q(oldVal)}`);
          }
        }
      }
      break;
    }

    case "HINCRBY": {
      const key = args[0];
      const field = args[1];
      const increment = args[2];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        const beforeFields = parseHashFields(entry.beforeRaw);
        const beforeMap = new Map(beforeFields);
        const oldVal = beforeMap.get(field);
        if (oldVal === undefined) {
          // Field was created by HINCRBY → HDEL
          inverseCmds.push(`HDEL ${q(key)} ${q(field)}`);
        } else {
          // Field existed → restore old value
          inverseCmds.push(`HSET ${q(key)} ${q(field)} ${q(oldVal)}`);
        }
      }
      void increment;
      break;
    }

    // ── List commands ────────────────────────────────────────────
    case "LPUSH": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        // LPUSH prepended N values → LPOP N values to undo
        const count = args.length - 1;
        if (count === 1) {
          inverseCmds.push(`LPOP ${q(key)}`);
        } else {
          inverseCmds.push(`LPOP ${q(key)} ${count}`);
        }
      }
      break;
    }

    case "RPUSH": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        const count = args.length - 1;
        if (count === 1) {
          inverseCmds.push(`RPOP ${q(key)}`);
        } else {
          inverseCmds.push(`RPOP ${q(key)} ${count}`);
        }
      }
      break;
    }

    case "LPOP": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "deleted") {
        // Key was fully emptied → rebuild
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, keyTypes[key] ?? "list"));
      } else {
        // Some items remain; push popped items back to the front
        const beforeItems = parseListItems(entry.beforeRaw);
        const afterItems = parseListItems(entry.afterRaw);
        const poppedCount = beforeItems.length - afterItems.length;
        if (poppedCount > 0) {
          const poppedItems = beforeItems.slice(0, poppedCount);
          // LPUSH in reverse order to restore original order
          const pushArgs = poppedItems.reverse().map(q).join(" ");
          inverseCmds.push(`LPUSH ${q(key)} ${pushArgs}`);
        }
      }
      break;
    }

    case "RPOP": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "deleted") {
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, keyTypes[key] ?? "list"));
      } else {
        const beforeItems = parseListItems(entry.beforeRaw);
        const afterItems = parseListItems(entry.afterRaw);
        const poppedCount = beforeItems.length - afterItems.length;
        if (poppedCount > 0) {
          const poppedItems = beforeItems.slice(afterItems.length);
          const pushArgs = poppedItems.map(q).join(" ");
          inverseCmds.push(`RPUSH ${q(key)} ${pushArgs}`);
        }
      }
      break;
    }

    // ── Set commands ─────────────────────────────────────────────
    case "SADD": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        // Only SREM the members that were actually added (not already present)
        const beforeMembers = parseSetMembers(entry.beforeRaw);
        const beforeSet = new Set(beforeMembers);
        const newMembers = args.slice(1).filter((m) => !beforeSet.has(m));
        if (newMembers.length > 0) {
          inverseCmds.push(`SREM ${q(key)} ${newMembers.map(q).join(" ")}`);
        }
      }
      break;
    }

    case "SREM": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "deleted") {
        // All members removed → rebuild
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, keyTypes[key] ?? "set"));
      } else {
        // Restore only the members that were actually removed
        const beforeMembers = parseSetMembers(entry.beforeRaw);
        const afterMembers = parseSetMembers(entry.afterRaw);
        const afterSet = new Set(afterMembers);
        const removedMembers = beforeMembers.filter((m) => !afterSet.has(m));
        if (removedMembers.length > 0) {
          inverseCmds.push(`SADD ${q(key)} ${removedMembers.map(q).join(" ")}`);
        }
      }
      break;
    }

    // ── Sorted set commands ──────────────────────────────────────
    case "ZADD": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "added") {
        inverseCmds.push(`DEL ${q(key)}`);
      } else {
        const beforeMembers = parseZSetMembers(entry.beforeRaw);
        const beforeMap = new Map(beforeMembers.map(([m, s]) => [m, s]));

        // Parse score-member pairs from args
        const scoreArgs = args.slice(1);
        for (let i = 0; i + 1 < scoreArgs.length; i += 2) {
          const member = scoreArgs[i + 1];
          const oldScore = beforeMap.get(member);
          if (oldScore === undefined) {
            // Member was added → ZREM
            inverseCmds.push(`ZREM ${q(key)} ${q(member)}`);
          } else {
            // Member existed → restore old score
            inverseCmds.push(`ZADD ${q(key)} ${oldScore} ${q(member)}`);
          }
        }
      }
      break;
    }

    case "ZREM": {
      const key = args[0];
      const entry = findDiff(key);
      if (!entry) break;

      if (entry.changeType === "deleted") {
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, keyTypes[key] ?? "zset"));
      } else {
        // Restore removed members with their original scores
        const beforeMembers = parseZSetMembers(entry.beforeRaw);
        const afterMembers = parseZSetMembers(entry.afterRaw);
        const afterSet = new Set(afterMembers.map(([m]) => m));
        const removedMembers = beforeMembers.filter(([m]) => !afterSet.has(m));
        for (const [member, score] of removedMembers) {
          inverseCmds.push(`ZADD ${q(key)} ${score} ${q(member)}`);
        }
      }
      break;
    }

    // ── Key-level commands ───────────────────────────────────────
    case "DEL": {
      // Every deleted key needs to be restored
      for (const key of args) {
        const entry = findDiff(key);
        if (!entry || entry.changeType !== "deleted") continue;
        const kt = keyTypes[key] ?? "string";
        inverseCmds.push(...buildRestoreCommands(key, entry.beforeRaw, kt));
      }
      break;
    }

    case "RENAME":
    case "RENAMENX": {
      if (args.length >= 2) {
        const src = args[0];
        const dst = args[1];
        // Reverse: rename dst back to src
        inverseCmds.push(`RENAME ${q(dst)} ${q(src)}`);
      }
      break;
    }

    case "MSET": {
      // Each key-value pair needs independent rollback
      for (let i = 0; i + 1 < args.length; i += 2) {
        const key = args[i];
        const entry = findDiff(key);
        if (!entry) continue;
        if (entry.changeType === "added") {
          inverseCmds.push(`DEL ${q(key)}`);
        } else if (entry.beforeRaw != null) {
          inverseCmds.push(`SET ${q(key)} ${q(entry.beforeRaw)}`);
        }
      }
      break;
    }

    default:
      // Unsupported command — cannot compute inverse
      // Return empty to signal rollback is not possible
      break;
  }

  return inverseCmds;
}

// ── Helpers ──────────────────────────────────────────────────────

/** Parse command string respecting quoted values */
function parseCommandParts(input: string): string[] {
  const parts: string[] = [];
  let current = "";
  let i = 0;

  while (i < input.length) {
    const c = input[i];
    if ((c === " " || c === "\t") && current === "") {
      i++;
    } else if (c === '"') {
      i++;
      while (i < input.length) {
        if (input[i] === '"') { i++; break; }
        if (input[i] === "\\" && i + 1 < input.length) {
          current += input[i + 1];
          i += 2;
        } else {
          current += input[i];
          i++;
        }
      }
    } else if (c === "'") {
      i++;
      while (i < input.length) {
        if (input[i] === "'") { i++; break; }
        current += input[i];
        i++;
      }
    } else if (c === " " || c === "\t") {
      parts.push(current);
      current = "";
      i++;
    } else {
      current += c;
      i++;
    }
  }
  if (current) parts.push(current);
  return parts;
}

type HashFields = [string, string][];

function parseHashFields(raw: string | null): HashFields {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter(
        (item): item is [string, string] =>
          Array.isArray(item) && item.length === 2,
      );
    }
  } catch { /* ignore */ }
  return [];
}

function parseListItems(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.map(String);
  } catch { /* ignore */ }
  return [];
}

function parseSetMembers(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.map(String);
  } catch { /* ignore */ }
  return [];
}

function parseZSetMembers(raw: string | null): [string, number][] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed
        .filter(
          (item): item is [string, number] =>
            Array.isArray(item) && item.length === 2,
        )
        .map(([m, s]) => [m, Number(s)]);
    }
  } catch { /* ignore */ }
  return [];
}

/**
 * Build full restore commands for a key (used when key was fully deleted).
 * This is the only scenario that requires full rebuild.
 */
function buildRestoreCommands(
  key: string,
  beforeRaw: string | null,
  keyType: string,
): string[] {
  if (!beforeRaw) return [];
  const cmds: string[] = [];
  const qk = key.includes(" ") ? `"${key}"` : key;

  switch (keyType) {
    case "string":
      cmds.push(`SET ${qk} ${beforeRaw.includes(" ") ? `"${beforeRaw}"` : beforeRaw}`);
      break;
    case "hash": {
      const fields = parseHashFields(beforeRaw);
      if (fields.length > 0) {
        const args = fields.map(([f, v]) => `${f.includes(" ") ? `"${f}"` : f} ${v.includes(" ") ? `"${v}"` : v}`).join(" ");
        cmds.push(`HSET ${qk} ${args}`);
      }
      break;
    }
    case "list": {
      const items = parseListItems(beforeRaw);
      if (items.length > 0) {
        const args = items.map((v) => v.includes(" ") ? `"${v}"` : v).join(" ");
        cmds.push(`RPUSH ${qk} ${args}`);
      }
      break;
    }
    case "set": {
      const members = parseSetMembers(beforeRaw);
      if (members.length > 0) {
        const args = members.map((v) => v.includes(" ") ? `"${v}"` : v).join(" ");
        cmds.push(`SADD ${qk} ${args}`);
      }
      break;
    }
    case "zset": {
      const members = parseZSetMembers(beforeRaw);
      if (members.length > 0) {
        const args = members.map(([m, s]) => `${s} ${m.includes(" ") ? `"${m}"` : m}`).join(" ");
        cmds.push(`ZADD ${qk} ${args}`);
      }
      break;
    }
    default:
      cmds.push(`SET ${qk} ${beforeRaw.includes(" ") ? `"${beforeRaw}"` : beforeRaw}`);
      break;
  }
  return cmds;
}
