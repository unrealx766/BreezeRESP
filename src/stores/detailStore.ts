import { defineStore } from "pinia";
import { ref, computed, watch } from "vue";
import type { KeyDetail, KeyValue, RedisDataType } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useCascadeStore } from "./cascadeStore";
import { useConnectionStore } from "./connectionStore";
import { useMetricsStore } from "./metricsStore";
import { useHistoryStore } from "./historyStore";

export const useDetailStore = defineStore("detail", () => {
  const currentDetail = ref<KeyDetail | null>(null);
  const loading = ref(false);
  const editing = ref(false);

  // Pagination state
  const currentPage = ref(0);
  const pageSize = ref(100);
  const filterPattern = ref("");

  const cascade = useCascadeStore();
  const history = useHistoryStore();

  /** Build a readable command string from structured initialData/items */
  function buildCommandStr(cmd: string, keyName: string, keyType: string, data: any): string {
    if (data == null) return `${cmd} ${keyName}`;
    switch (keyType) {
      case "string":
        return `${cmd} ${keyName} ${data}`;
      case "hash":
        if (Array.isArray(data)) {
          const pairs = data.map(([f, v]: [string, string]) => `${f} ${v}`).join(" ");
          return `${cmd} ${keyName} ${pairs}`;
        }
        return `${cmd} ${keyName} ${JSON.stringify(data)}`;
      case "list":
      case "set":
        if (Array.isArray(data)) return `${cmd} ${keyName} ${data.join(" ")}`;
        return `${cmd} ${keyName} ${JSON.stringify(data)}`;
      case "zset":
        if (Array.isArray(data)) {
          const pairs = data.map(([member, score]: [string, number]) => `${score} ${member}`).join(" ");
          return `${cmd} ${keyName} ${pairs}`;
        }
        return `${cmd} ${keyName} ${JSON.stringify(data)}`;
      default:
        return `${cmd} ${keyName}`;
    }
  }

  const currentKey = computed(() => currentDetail.value?.key ?? null);
  const currentValue = computed<KeyValue | null>(() => currentDetail.value?.value ?? null);
  const ttlRemaining = ref(0);
  const ttlTotal = ref(0);
  const ttlPercent = computed(() =>
    ttlTotal.value > 0 ? Math.max(0, (ttlRemaining.value / ttlTotal.value) * 100) : -1
  );
  const isExpired = ref(false);

  /** Convert Rust KeyDetail response to frontend KeyDetail type */
  function mapKeyDetail(rust: any): KeyDetail {
    const rk = rust.key || {};
    const keyType = (rk.keyType || rk.key_type || "string") as RedisDataType;
    // Defensive: if Tauri IPC returns value as a JSON string (possible with serde_json::Value), parse it
    let val = rust.value as unknown;
    if (typeof val === "string") {
      console.warn("[mapKeyDetail] value was a string, parsing:", val.slice(0, 200));
      try { val = JSON.parse(val); } catch { val = {}; }
    }
    const v = val as Record<string, unknown>;
    if (!v || typeof v !== "object" || !v.type) {
      console.warn("[mapKeyDetail] unexpected value format:", typeof val, "keys:", v ? Object.keys(v) : "null");
    }

    let keyValue: KeyValue;

    switch (v.type) {
      case "string":
        keyValue = {
          type: "string",
          value: v.value as string,
          valueHex: (v.valueHex as string) || "",
          encoding: (v.encoding as string) || rust.encoding,
          contentEncoding: v.contentEncoding as string | undefined,
        };
        break;
      case "hash":
        keyValue = {
          type: "hash",
          fields: (v.fields as Array<{ field: string; fieldHex?: string; value: string; valueHex?: string; ttl?: number }>) || [],
          encoding: (v.encoding as string) || rust.encoding,
          contentEncoding: v.contentEncoding as string | undefined,
          totalCount: v.totalCount as number | undefined,
          truncated: v.truncated as boolean | undefined,
          hasFieldTtl: v.hasFieldTtl as boolean | undefined,
        };
        break;
      case "list":
        keyValue = {
          type: "list",
          items: (v.items as string[]) || [],
          itemsHex: (v.itemsHex as string[]) || undefined,
          encoding: (v.encoding as string) || rust.encoding,
          contentEncoding: v.contentEncoding as string | undefined,
          totalCount: v.totalCount as number | undefined,
          truncated: v.truncated as boolean | undefined,
          originalIndices: v.originalIndices as number[] | undefined,
        };
        break;
      case "set":
        keyValue = {
          type: "set",
          members: (v.members as string[]) || [],
          membersHex: (v.membersHex as string[]) || undefined,
          encoding: (v.encoding as string) || rust.encoding,
          contentEncoding: v.contentEncoding as string | undefined,
          totalCount: v.totalCount as number | undefined,
          truncated: v.truncated as boolean | undefined,
        };
        break;
      case "zset":
        keyValue = {
          type: "zset",
          members: (v.members as Array<{ member: string; memberHex?: string; score: number }>) || [],
          encoding: (v.encoding as string) || rust.encoding,
          contentEncoding: v.contentEncoding as string | undefined,
          totalCount: v.totalCount as number | undefined,
          truncated: v.truncated as boolean | undefined,
        };
        break;
      default:
        keyValue = {
          type: "string",
          value: JSON.stringify(v),
          valueHex: "",
          encoding: rust.encoding,
          contentEncoding: undefined,
        };
    }

    return {
      key: {
        key: rk.key || "",
        type: keyType,
        ttl: rk.ttl ?? -1,
        size: rk.size ?? 0,
      },
      value: keyValue,
    };
  }

  // TTL countdown simulation
  let ttlTimer: ReturnType<typeof setInterval> | null = null;
  function startTtlTimer() {
    stopTtlTimer();
    ttlTimer = setInterval(() => {
      if (ttlRemaining.value > 0) {
        ttlRemaining.value--;
        if (ttlRemaining.value <= 0 && ttlTotal.value > 0) {
          isExpired.value = true;
          stopTtlTimer();
        }
      } else {
        stopTtlTimer();
      }
    }, 1000);
  }
  function stopTtlTimer() {
    if (ttlTimer) { clearInterval(ttlTimer); ttlTimer = null; }
  }

  async function loadDetail(key: string, page = 0, filter?: string) {
    const connStore = useConnectionStore();
    const metricsStore = useMetricsStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    loading.value = true;
    currentPage.value = page;
    if (filter !== undefined) filterPattern.value = filter;

    try {
      const offset = page * pageSize.value;
      const redisVersion = metricsStore.version || undefined;
      let rustDetail = await tauriApi.cascade.getKeyDetail(
        connId,
        key,
        offset,
        pageSize.value,
        filterPattern.value || undefined,
        redisVersion
      );
      // Defensive: handle double-serialized IPC response
      if (typeof rustDetail === "string") {
        try { rustDetail = JSON.parse(rustDetail); } catch { /* use as-is */ }
      }
      currentDetail.value = mapKeyDetail(rustDetail);

      if (currentDetail.value.key.ttl > 0) {
        ttlTotal.value = currentDetail.value.key.ttl;
        ttlRemaining.value = currentDetail.value.key.ttl;
        isExpired.value = false;
      } else if (currentDetail.value.key.ttl === 0) {
        // Key has 0 TTL - already expired on server
        ttlTotal.value = 1;
        ttlRemaining.value = 0;
        isExpired.value = true;
      } else {
        // ttl = -1 (no expiry) or -2 (key missing)
        ttlTotal.value = 0;
        ttlRemaining.value = 0;
        isExpired.value = false;
      }
    } catch (e) {
      console.error("Failed to load key detail:", e);
      // If key had a TTL and now fails to load, it likely expired
      if (ttlTotal.value > 0 || isExpired.value) {
        isExpired.value = true;
      }
      currentDetail.value = null;
    } finally {
      loading.value = false;
    }
    // Only start TTL timer on successful load
    if (currentDetail.value) {
      startTtlTimer();
    }
  }

  /** Load a specific page (clamped to valid range) */
  function loadPage(page: number) {
    const key = currentDetail.value?.key.key ?? cascade.selectedKey;
    if (!key) return;
    const total = (currentValue.value as any)?.totalCount ?? 0;
    const maxPage = Math.max(0, Math.ceil(total / pageSize.value) - 1);
    const clamped = Math.max(0, Math.min(page, maxPage));
    loadDetail(key, clamped);
  }

  /** Search/filter within the key */
  function searchFilter(pattern: string) {
    const key = currentDetail.value?.key.key ?? cascade.selectedKey;
    if (key) loadDetail(key, 0, pattern);
  }

  // Watch selected key changes
  watch(
    () => cascade.selectedKey,
    (key) => {
      if (key) {
        // Reset pagination and filter when switching keys
        filterPattern.value = "";
        currentPage.value = 0;
        loadDetail(key);
      } else {
        currentDetail.value = null;
      }
    }
  );

  function setEditing(val: boolean) { editing.value = val; }

  /** Save string value */
  async function saveStringValue(newValue: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`SET ${key} ${newValue}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "string", action: "set", value: newValue })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save string value:", e);
      return false;
    }
  }

  /** Save hash field value */
  async function saveHashField(field: string, newValue: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`HSET ${key} ${field} ${newValue}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "hash", action: "set", field, value: newValue })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save hash field:", e);
      return false;
    }
  }

  /** Set TTL on an individual hash field (Redis >= 7.4.0) */
  async function setHashFieldTtl(field: string, ttl: number) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`HEXPIRE ${key} ${ttl} FIELDS 1 ${field}`, "browser", () =>
        tauriApi.cascade.setHashFieldTtl(connId, key, field, ttl)
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to set hash field TTL:", e);
      return false;
    }
  }

  /** Rename a hash field (preserves its value) */
  async function renameHashField(oldField: string, newField: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key || oldField === newField) return false;
    try {
      await history.execAndRecord(`HREN ${key} ${oldField} ${newField}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "hash", action: "rename_field", field: newField, oldValue: oldField })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to rename hash field:", e);
      return false;
    }
  }

  /** Save list item by index */
  async function saveListItem(index: number, newValue: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`LSET ${key} ${index} ${newValue}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "list", action: "set", index, value: newValue })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save list item:", e);
      return false;
    }
  }

  /** Save set member (rename) */
  async function saveSetMember(oldValue: string, newValue: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`SREM ${key} ${oldValue} + SADD ${key} ${newValue}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "set", action: "set", value: newValue, oldValue })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save set member:", e);
      return false;
    }
  }

  /** Save zset member (update member name and/or score) */
  async function saveZSetMember(oldMember: string, newMember: string, score: number) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`ZREM ${key} ${oldMember} + ZADD ${key} ${score} ${newMember}`, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: "zset", action: "set", value: newMember, score, oldValue: oldMember })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save zset member:", e);
      return false;
    }
  }

  /** Delete a sub-element from hash/list/set/zset */
  async function deleteField(params: { keyType: string; field?: string; value?: string }) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;

    const cmdMap: Record<string, string> = {
      hash: `HDEL ${key} ${params.field ?? ""}`,
      list: `LREM ${key} 1 ${params.value ?? ""}`,
      set: `SREM ${key} ${params.value ?? ""}`,
      zset: `ZREM ${key} ${params.value ?? ""}`,
    };
    const cmdStr = cmdMap[params.keyType] ?? `DEL ${key}`;

    try {
      await history.execAndRecord(cmdStr, "browser", () =>
        tauriApi.cascade.setValue({ connectionId: connId, key, keyType: params.keyType, action: "delete_field", field: params.field, value: params.value })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to delete field:", e);
      return false;
    }
  }

  /** Create a new key (string/hash/list/set/zset) with optional TTL and initial data */
  async function createKey(params: {
    keyName: string;
    keyType: string;
    ttl?: number;
    initialData?: any;
    fieldTtl?: number;
  }) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId || !params.keyName) return false;

    // Build a single combined command string (atomic operation)
    let cmdStr: string;
    const hasTtl = params.ttl != null && params.ttl > 0;
    if (params.keyType === "string" && hasTtl) {
      // String supports SET key value EX ttl natively
      const val = params.initialData != null ? ` ${params.initialData}` : "";
      cmdStr = `SET ${params.keyName}${val} EX ${params.ttl}`;
    } else {
      const baseCmd = { hash: "HSET", list: "RPUSH", set: "SADD", zset: "ZADD" }[params.keyType] ?? "SET";
      cmdStr = buildCommandStr(baseCmd, params.keyName, params.keyType, params.initialData);
      if (hasTtl) cmdStr += ` + EXPIRE ${params.keyName} ${params.ttl}`;
    }

    try {
      await history.execAndRecord(cmdStr, "browser", () =>
        tauriApi.cascade.createKey({ connectionId: connId, key: params.keyName, keyType: params.keyType, ttl: params.ttl, initialData: params.initialData, fieldTtl: params.fieldTtl })
      );
      const cascadeStore = useCascadeStore();
      await cascadeStore.refreshKeys(true);
      cascadeStore.selectKey(params.keyName);
      return true;
    } catch (e) {
      console.error("Failed to create key:", e);
      throw e;
    }
  }

  /** Batch add fields/members/items to hash/list/set/zset */
  async function batchAddFields(params: {
    keyType: string;
    items: any;
    fieldTtl?: number;
  }) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;

    const cmdStr = buildCommandStr(
      { hash: "HMSET", list: "RPUSH", set: "SADD", zset: "ZADD" }[params.keyType] ?? "HMSET",
      key, params.keyType, params.items
    );

    try {
      await history.execAndRecord(cmdStr, "browser", () =>
        tauriApi.cascade.batchAddFields({ connectionId: connId, key, keyType: params.keyType, items: params.items, fieldTtl: params.fieldTtl })
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to batch add fields:", e);
      return false;
    }
  }

  /** Rename key */
  async function renameKey(newKey: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const oldKey = currentDetail.value?.key.key;
    if (!connId || !oldKey || oldKey === newKey) return false;
    try {
      await history.execAndRecord(`RENAME ${oldKey} ${newKey}`, "browser", () =>
        tauriApi.cascade.renameKey(connId, oldKey, newKey)
      );
      const cascadeStore = useCascadeStore();
      const k = cascadeStore.keys.find((k) => k.key === oldKey);
      if (k) k.key = newKey;
      cascade.selectedKey = newKey;
      filterPattern.value = "";
      currentPage.value = 0;
      await loadDetail(newKey);
      return true;
    } catch (e) {
      console.error("Failed to rename key:", e);
      return false;
    }
  }

  /** Set TTL */
  async function setTtl(ttl: number) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    const key = currentDetail.value?.key.key;
    if (!connId || !key) return false;
    try {
      await history.execAndRecord(`EXPIRE ${key} ${ttl}`, "browser", () =>
        tauriApi.cascade.setKeyTtl(connId, key, ttl)
      );
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to set TTL:", e);
      return false;
    }
  }

  function clearDetail() {
    currentDetail.value = null;
    currentPage.value = 0;
    filterPattern.value = "";
    stopTtlTimer();
  }

  function refresh() {
    const key = currentDetail.value?.key.key ?? cascade.selectedKey;
    if (key) loadDetail(key, currentPage.value);
  }

  return {
    currentDetail,
    currentKey,
    currentValue,
    loading,
    editing,
    ttlRemaining,
    ttlTotal,
    ttlPercent,
    isExpired,
    currentPage,
    pageSize,
    filterPattern,
    loadDetail,
    loadPage,
    searchFilter,
    clearDetail,
    setEditing,
    refresh,
    saveStringValue,
    saveHashField,
    setHashFieldTtl,
    renameHashField,
    saveListItem,
    saveSetMember,
    saveZSetMember,
    deleteField,
    createKey,
    batchAddFields,
    renameKey,
    setTtl,
  };
});
