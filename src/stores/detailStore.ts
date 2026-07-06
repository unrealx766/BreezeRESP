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

  const cascade = useCascadeStore();

  const currentKey = computed(() => currentDetail.value?.key ?? null);
  const currentValue = computed<KeyValue | null>(() => currentDetail.value?.value ?? null);
  const ttlRemaining = ref(0);
  const ttlTotal = ref(0);
  const ttlPercent = computed(() =>
    ttlTotal.value > 0 ? Math.max(0, (ttlRemaining.value / ttlTotal.value) * 100) : -1
  );

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
        };
        break;
      case "hash":
        keyValue = {
          type: "hash",
          fields: (val.fields as Array<{ field: string; value: string }>) || [],
          encoding: (val.encoding as string) || rust.encoding,
        };
        break;
      case "list":
        keyValue = {
          type: "list",
          items: (val.items as string[]) || [],
          encoding: (val.encoding as string) || rust.encoding,
        };
        break;
      case "set":
        keyValue = {
          type: "set",
          members: (val.members as string[]) || [],
          encoding: (val.encoding as string) || rust.encoding,
        };
        break;
      case "zset":
        keyValue = {
          type: "zset",
          members: (val.members as Array<{ member: string; score: number }>) || [],
          encoding: (val.encoding as string) || rust.encoding,
        };
        break;
      default:
        keyValue = {
          type: "string",
          value: JSON.stringify(val),
          encoding: rust.encoding,
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

  async function loadDetail(key: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    loading.value = true;
    try {
      const rustDetail = await tauriApi.cascade.getKeyDetail(connId, key);
      currentDetail.value = mapKeyDetail(rustDetail);

      if (currentDetail.value.key.ttl > 0) {
        ttlTotal.value = currentDetail.value.key.ttl;
        ttlRemaining.value = currentDetail.value.key.ttl;
      } else {
        ttlTotal.value = 0;
        ttlRemaining.value = 0;
      }
    } catch (e) {
      console.error("Failed to load key detail:", e);
      currentDetail.value = null;
    } finally {
      loading.value = false;
    }
  }

  // Watch selected key changes
  watch(
    () => cascade.selectedKey,
    (key) => {
      if (key) loadDetail(key);
      else currentDetail.value = null;
    }
  );

  // TTL countdown simulation
  let ttlTimer: ReturnType<typeof setInterval> | null = null;
  function startTtlTimer() {
    stopTtlTimer();
    ttlTimer = setInterval(() => {
      if (ttlRemaining.value > 0) ttlRemaining.value--;
    }, 1000);
  }
  function stopTtlTimer() {
    if (ttlTimer) { clearInterval(ttlTimer); ttlTimer = null; }
  }
  startTtlTimer();

  function setEditing(val: boolean) { editing.value = val; }

  function clearDetail() {
    currentDetail.value = null;
    stopTtlTimer();
  }

  function refresh() {
    if (currentDetail.value) loadDetail(currentDetail.value.key.key);
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
    loadDetail,
    clearDetail,
    setEditing,
    refresh,
  };
});
