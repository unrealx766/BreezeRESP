import { defineStore } from "pinia";
import { ref, computed, watch } from "vue";
import type { KeyDetail, KeyValue, RedisDataType } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useCascadeStore } from "./cascadeStore";
import { useConnectionStore } from "./connectionStore";

export const useDetailStore = defineStore("detail", () => {
  const currentDetail = ref<KeyDetail | null>(null);
  const loading = ref(false);
  const editing = ref(false);

  // Pagination state
  const currentPage = ref(0);
  const pageSize = ref(100);
  const filterPattern = ref("");

  const cascade = useCascadeStore();

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
    const val = rust.value as Record<string, unknown>;

    let keyValue: KeyValue;

    switch (val.type) {
      case "string":
        keyValue = {
          type: "string",
          value: val.value as string,
          encoding: (val.encoding as string) || rust.encoding,
          contentEncoding: val.contentEncoding as string | undefined,
        };
        break;
      case "hash":
        keyValue = {
          type: "hash",
          fields: (val.fields as Array<{ field: string; value: string }>) || [],
          encoding: (val.encoding as string) || rust.encoding,
          contentEncoding: val.contentEncoding as string | undefined,
          totalCount: val.totalCount as number | undefined,
          truncated: val.truncated as boolean | undefined,
        };
        break;
      case "list":
        keyValue = {
          type: "list",
          items: (val.items as string[]) || [],
          encoding: (val.encoding as string) || rust.encoding,
          contentEncoding: val.contentEncoding as string | undefined,
          totalCount: val.totalCount as number | undefined,
          truncated: val.truncated as boolean | undefined,
          originalIndices: val.originalIndices as number[] | undefined,
        };
        break;
      case "set":
        keyValue = {
          type: "set",
          members: (val.members as string[]) || [],
          encoding: (val.encoding as string) || rust.encoding,
          contentEncoding: val.contentEncoding as string | undefined,
          totalCount: val.totalCount as number | undefined,
          truncated: val.truncated as boolean | undefined,
        };
        break;
      case "zset":
        keyValue = {
          type: "zset",
          members: (val.members as Array<{ member: string; score: number }>) || [],
          encoding: (val.encoding as string) || rust.encoding,
          contentEncoding: val.contentEncoding as string | undefined,
          totalCount: val.totalCount as number | undefined,
          truncated: val.truncated as boolean | undefined,
        };
        break;
      default:
        keyValue = {
          type: "string",
          value: JSON.stringify(val),
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
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    loading.value = true;
    currentPage.value = page;
    if (filter !== undefined) filterPattern.value = filter;

    try {
      const offset = page * pageSize.value;
      const rustDetail = await tauriApi.cascade.getKeyDetail(
        connId,
        key,
        offset,
        pageSize.value,
        filterPattern.value || undefined
      );
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
      await tauriApi.cascade.setValue({
        connectionId: connId,
        key,
        keyType: "string",
        action: "set",
        value: newValue,
      });
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
      await tauriApi.cascade.setValue({
        connectionId: connId,
        key,
        keyType: "hash",
        action: "set",
        field,
        value: newValue,
      });
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save hash field:", e);
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
      await tauriApi.cascade.setValue({
        connectionId: connId,
        key,
        keyType: "list",
        action: "set",
        index,
        value: newValue,
      });
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
      await tauriApi.cascade.setValue({
        connectionId: connId,
        key,
        keyType: "set",
        action: "set",
        value: newValue,
        oldValue,
      });
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
      await tauriApi.cascade.setValue({
        connectionId: connId,
        key,
        keyType: "zset",
        action: "set",
        value: newMember,
        score,
        oldValue: oldMember,
      });
      await loadDetail(key, currentPage.value);
      return true;
    } catch (e) {
      console.error("Failed to save zset member:", e);
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
      await tauriApi.cascade.renameKey(connId, oldKey, newKey);
      // Update cascade store
      const cascadeStore = useCascadeStore();
      const k = cascadeStore.keys.find((k) => k.key === oldKey);
      if (k) k.key = newKey;
      cascade.selectedKey = newKey;
      // Reset pagination for renamed key
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
      await tauriApi.cascade.setKeyTtl(connId, key, ttl);
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
    saveListItem,
    saveSetMember,
    saveZSetMember,
    renameKey,
    setTtl,
  };
});
