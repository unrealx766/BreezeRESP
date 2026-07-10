const MAX_DISPLAY_LENGTH = 2000;

/** Truncate long values for display, matching Sandbox & Pipeline behavior */
export function truncateValue(val: string | null | undefined): string {
  if (val == null) return "(empty)";
  if (val.length > MAX_DISPLAY_LENGTH) {
    return val.slice(0, MAX_DISPLAY_LENGTH) + `\n... (${val.length - MAX_DISPLAY_LENGTH} more characters)`;
  }
  return val;
}
