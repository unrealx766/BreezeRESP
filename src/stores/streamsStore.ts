import { defineStore } from "pinia";
import { ref } from "vue";
import type { ConsumerGroup, ConsumerInfo, PendingEntry, StreamEntry, StreamInfo } from "@/types";
import { tauriApi } from "@/services/tauriApi";

/**
 * State for the Streams cockpit page. Keyed operations resolve the active
 * connection themselves via the caller-supplied connectionId so the page
 * can refresh after connection switches.
 */
export const useStreamsStore = defineStore("streams", () => {
  // Stream key list
  const streamKeys = ref<string[]>([]);
  const loadingKeys = ref(false);

  // Selected stream detail
  const selectedKey = ref("");
  const streamInfo = ref<StreamInfo | null>(null);
  const loadingInfo = ref(false);

  // Message browsing
  const entries = ref<StreamEntry[]>([]);
  const loadingEntries = ref(false);

  // Consumer groups & consumers & PEL
  const groups = ref<ConsumerGroup[]>([]);
  const loadingGroups = ref(false);
  const consumers = ref<ConsumerInfo[]>([]);
  const selectedGroup = ref("");
  const loadingConsumers = ref(false);
  const pendingEntries = ref<PendingEntry[]>([]);
  const loadingPending = ref(false);

  async function loadKeys(connectionId: string, pattern?: string) {
    loadingKeys.value = true;
    try {
      streamKeys.value = await tauriApi.streams.list(connectionId, pattern || undefined);
    } finally {
      loadingKeys.value = false;
    }
  }

  async function loadInfo(connectionId: string, key: string) {
    selectedKey.value = key;
    loadingInfo.value = true;
    try {
      streamInfo.value = await tauriApi.streams.getInfo(connectionId, key);
    } finally {
      loadingInfo.value = false;
    }
  }

  async function loadEntries(connectionId: string, key: string, start?: string, end?: string, count = 100) {
    loadingEntries.value = true;
    try {
      entries.value = await tauriApi.streams.getEntries(connectionId, key, start, end, count);
    } finally {
      loadingEntries.value = false;
    }
  }

  async function loadGroups(connectionId: string, key: string) {
    loadingGroups.value = true;
    try {
      groups.value = await tauriApi.streams.getGroups(connectionId, key);
    } finally {
      loadingGroups.value = false;
    }
  }

  async function loadConsumers(connectionId: string, key: string, group: string) {
    selectedGroup.value = group;
    loadingConsumers.value = true;
    try {
      consumers.value = await tauriApi.streams.getConsumers(connectionId, key, group);
    } finally {
      loadingConsumers.value = false;
    }
  }

  async function loadPending(connectionId: string, key: string, group: string, count = 200) {
    loadingPending.value = true;
    try {
      pendingEntries.value = await tauriApi.streams.getPending(connectionId, key, group, count);
    } finally {
      loadingPending.value = false;
    }
  }

  /** Clear everything (connection switch / disconnect). */
  function reset() {
    streamKeys.value = [];
    selectedKey.value = "";
    streamInfo.value = null;
    entries.value = [];
    groups.value = [];
    consumers.value = [];
    selectedGroup.value = "";
    pendingEntries.value = [];
  }

  return {
    streamKeys, loadingKeys,
    selectedKey, streamInfo, loadingInfo,
    entries, loadingEntries,
    groups, loadingGroups,
    consumers, selectedGroup, loadingConsumers,
    pendingEntries, loadingPending,
    loadKeys, loadInfo, loadEntries, loadGroups, loadConsumers, loadPending,
    reset,
  };
});
