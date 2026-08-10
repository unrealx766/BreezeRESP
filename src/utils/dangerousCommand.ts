/**
 * Dangerous command detection.
 *
 * Commands that irreversibly delete or overwrite data require a second
 * confirmation before execution (Pipeline / Sandbox apply). Administrative
 * commands (FLUSHALL, SHUTDOWN, CONFIG, ...) are already hard-blocked by the
 * backend (validate.rs); they are kept here as defense-in-depth so the
 * confirmation still fires if the block list ever changes.
 */
const DANGEROUS_COMMANDS = new Set([
  // Key deletion with wildcard patterns (plain explicit DEL is allowed directly)
  "UNLINK",
  // Rename may silently overwrite the destination key
  "RENAME", "RENAMENX",
  // DB-level destructive operations (backend-blocked, defensive)
  "FLUSHDB", "FLUSHALL", "SWAPDB",
  // Server administration (backend-blocked, defensive)
  "SHUTDOWN", "DEBUG", "CONFIG", "SCRIPT",
]);

/**
 * Whether a raw command string (e.g. "DEL user:*") is dangerous.
 * DEL only counts as dangerous when any key argument contains a wildcard
 * (`*` or `?`), e.g. `DEL user:*` — pattern-style mass deletion.
 */
export function isDangerousCommand(command: string): boolean {
  const parts = command.trim().split(/\s+/);
  const name = parts[0]?.toUpperCase() ?? "";
  if (name === "DEL") {
    return parts.slice(1).some((arg) => arg.includes("*") || arg.includes("?"));
  }
  return DANGEROUS_COMMANDS.has(name);
}
