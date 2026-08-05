import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { ServerCapability } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

/**
 * Caches server capability profiles (Redis version + module support) per
 * connection. Profiles are probed lazily on first access and dropped when
 * the backend clears them on disconnect (a fresh probe happens on demand).
 */
export const useCapabilityStore = defineStore("capability", () => {
  const profiles = ref<Record<string, ServerCapability>>({});
  const probing = ref<Record<string, boolean>>({});

  /** Capability of the active connection (null until probed). */
  const activeCapability = computed<ServerCapability | null>(() => {
    const connStore = useConnectionStore();
    const id = connStore.activeConnectionId;
    return id ? profiles.value[id] ?? null : null;
  });

  function capabilityOf(connectionId: string): ServerCapability | null {
    return profiles.value[connectionId] ?? null;
  }

  /** Fetch (or reuse cached) capability profile for a connection. */
  async function fetchCapability(connectionId: string, force = false): Promise<ServerCapability | null> {
    if (!connectionId) return null;
    if (!force && profiles.value[connectionId]) return profiles.value[connectionId];
    if (probing.value[connectionId]) return profiles.value[connectionId] ?? null;
    probing.value[connectionId] = true;
    try {
      const cap = await tauriApi.capability.get(connectionId, force);
      profiles.value = { ...profiles.value, [connectionId]: cap };
      return cap;
    } catch {
      return null;
    } finally {
      probing.value = { ...probing.value, [connectionId]: false };
    }
  }

  /** Drop a cached profile (e.g. on disconnect). */
  function invalidate(connectionId: string) {
    if (profiles.value[connectionId]) {
      const next = { ...profiles.value };
      delete next[connectionId];
      profiles.value = next;
    }
  }

  return { profiles, activeCapability, capabilityOf, fetchCapability, invalidate };
});
