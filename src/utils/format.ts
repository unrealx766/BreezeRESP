const MAX_DISPLAY_LENGTH = 2000;

/** Truncate long values for display, matching Sandbox & Pipeline behavior */
export function truncateValue(val: string | null | undefined): string {
  if (val == null) return "(empty)";
  if (val.length > MAX_DISPLAY_LENGTH) {
    return val.slice(0, MAX_DISPLAY_LENGTH) + `\n... (${val.length - MAX_DISPLAY_LENGTH} more characters)`;
  }
  return val;
}

/** Format bytes into human-readable string (e.g. 1024 → "1.00 KB") */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(Math.abs(bytes)) / Math.log(1024));
  const idx = Math.min(i, units.length - 1);
  if (idx === 0) return `${bytes.toLocaleString()} B`;
  return `${(bytes / Math.pow(1024, idx)).toFixed(2)} ${units[idx]}`;
}
