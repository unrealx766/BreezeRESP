// Update checker based on GitHub Releases.
// The HTTP request is executed by the Rust backend (get_latest_release command),
// which is not subject to WebView CORS and tries the Atom feed before the REST API.
import { i18n } from "@/i18n";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "@/utils/toast";
import { autoCheckUpdate } from "@/utils/uiSettings";

const RELEASES_URL = "https://github.com/unrealx766/BreezeRESP/releases";
const LAST_CHECK_KEY = "breezeresp-update-last-check";
const NOTIFIED_VERSION_KEY = "breezeresp-update-notified-version";

/** Auto-check at most once every 24 hours */
const AUTO_CHECK_INTERVAL = 24 * 60 * 60 * 1000;

export interface UpdateCheckResult {
  hasUpdate: boolean;
  latestVersion: string;
  releaseUrl: string;
}

/** Strip leading "v" and surrounding whitespace from a version tag */
function normalizeVersion(v: string): string {
  return v.replace(/^v/i, "").trim();
}

/** Semver-ish comparison: returns >0 if a is newer than b */
function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

interface RustLatestRelease {
  latestVersion: string;
  releaseUrl: string;
}

/** Ask the Rust backend for the latest release and compare with the running version. Returns null on failure. */
export async function checkForUpdates(): Promise<UpdateCheckResult | null> {
  try {
    const release = await invoke<RustLatestRelease>("get_latest_release");
    const latestVersion = normalizeVersion(release.latestVersion ?? "");
    if (!latestVersion) return null;
    return {
      hasUpdate: compareVersions(latestVersion, normalizeVersion(__APP_VERSION__)) > 0,
      latestVersion,
      releaseUrl: release.releaseUrl || RELEASES_URL,
    };
  } catch {
    return null;
  }
}

/** Show an interactive toast with a "Download" action and remember the notified version */
export function notifyUpdate(result: UpdateCheckResult) {
  const t = i18n.global.t;
  localStorage.setItem(NOTIFIED_VERSION_KEY, result.latestVersion);
  toast.info(t("updater.updateAvailable", { version: result.latestVersion }), undefined, {
    label: t("updater.download"),
    onClick: () => openUrl(result.releaseUrl),
  });
}

/**
 * Startup auto-check: respects the user toggle, throttles to once per 24h
 * and only notifies once per new version.
 */
export async function autoCheckForUpdates(): Promise<void> {
  if (!autoCheckUpdate.value) return;
  const last = Number(localStorage.getItem(LAST_CHECK_KEY) || "0");
  if (Date.now() - last < AUTO_CHECK_INTERVAL) return;
  localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));

  const result = await checkForUpdates();
  if (!result?.hasUpdate) return;
  if (localStorage.getItem(NOTIFIED_VERSION_KEY) === result.latestVersion) return;
  notifyUpdate(result);
}
