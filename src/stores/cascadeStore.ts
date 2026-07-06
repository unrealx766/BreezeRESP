import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { RedisKey, RedisDataType, KeyTreeNode, SortField, SortOrder } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

export const useCascadeStore = defineStore("cascade", () => {
  const keys = ref<RedisKey[]>([]);
  const searchQuery = ref("");
  const typeFilter = ref<RedisDataType | "all">("all");
  const sortField = ref<SortField>("name");
  const sortOrder = ref<SortOrder>("asc");
  const selectedKey = ref<string | null>(null);
  const loading = ref(false);
  const expandedPaths = ref(new Set<string>());
  let refreshing = false;

  const filteredKeys = computed(() => {
    let result = keys.value;
    if (searchQuery.value) {
      const q = searchQuery.value.toLowerCase();
      result = result.filter((k) => k.key.toLowerCase().includes(q));
    }
    if (typeFilter.value !== "all") {
      result = result.filter((k) => k.type === typeFilter.value);
    }
    result = [...result].sort((a, b) => {
      let cmp = 0;
      if (sortField.value === "name") cmp = a.key.localeCompare(b.key);
      else if (sortField.value === "type") cmp = a.type.localeCompare(b.type);
      else if (sortField.value === "ttl") cmp = a.ttl - b.ttl;
      else if (sortField.value === "size") cmp = a.size - b.size;
      return sortOrder.value === "asc" ? cmp : -cmp;
    });
    return result;
  });

  const keyTree = computed<KeyTreeNode[]>(() => {
    const root: KeyTreeNode[] = [];
    const nodeMap = new Map<string, KeyTreeNode>();
    const expanded = expandedPaths.value;

    for (const key of filteredKeys.value) {
      const parts = key.key.split(":");
      let currentPath = "";

      for (let i = 0; i < parts.length; i++) {
        const parentPath = currentPath;
        currentPath = currentPath ? `${currentPath}:${parts[i]}` : parts[i];
        const isLeaf = i === parts.length - 1;

        if (!nodeMap.has(currentPath)) {
          const node: KeyTreeNode = {
            label: parts[i],
            fullPath: currentPath,
            children: [],
            expanded: expanded.has(currentPath),
            key: isLeaf ? key : undefined,
          };
          nodeMap.set(currentPath, node);

          if (parentPath && nodeMap.has(parentPath)) {
            nodeMap.get(parentPath)!.children.push(node);
          } else if (!parentPath) {
            root.push(node);
          }
        }
      }
    }
    return root;
  });

  const keyCount = computed(() => keys.value.length);

  const typeDistribution = computed(() => {
    const dist: Record<RedisDataType, number> = { string: 0, hash: 0, list: 0, set: 0, zset: 0 };
    for (const k of keys.value) {
      if (k.type in dist) dist[k.type]++;
    }
    return dist;
  });

  function selectKey(key: string) {
    selectedKey.value = key;
  }

  function toggleNode(node: KeyTreeNode) {
    const s = new Set(expandedPaths.value);
    if (s.has(node.fullPath)) {
      s.delete(node.fullPath);
    } else {
      s.add(node.fullPath);
    }
    expandedPaths.value = s;
  }

  async function refreshKeys(force = false) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;
    // Skip if already refreshing, unless forced (e.g. DB switch)
    if (refreshing && !force) return;

    refreshing = true;
    loading.value = true;
    try {
      let allKeys: RedisKey[] = [];
      let cursor = 0;
      const maxIterations = 100;
      let iterations = 0;

      do {
        const raw = await tauriApi.cascade.scanKeys(
          connId,
          searchQuery.value || "*",
          cursor,
          200
        );

        // Tauri returns Rust tuple as JSON array: [cursor_number, keys_array]
        const response = raw as unknown as [number, any[]];
        const nextCursor = response[0];
        const rustKeys = Array.isArray(response[1]) ? response[1] : [];

        allKeys.push(
          ...rustKeys.map((rk: any) => ({
            key: rk.key,
            type: (rk.keyType || rk.key_type || "string") as RedisDataType,
            ttl: rk.ttl ?? -1,
            size: rk.size ?? 0,
          }))
        );
        cursor = nextCursor;
        iterations++;
      } while (cursor !== 0 && iterations < maxIterations);

      keys.value = allKeys;
    } catch (e) {
      console.error("Failed to scan keys:", e);
    } finally {
      loading.value = false;
      refreshing = false;
    }
  }

  async function deleteKey(key: string) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;

    try {
      await tauriApi.cascade.deleteKey(connId, key);
      keys.value = keys.value.filter((k) => k.key !== key);
      if (selectedKey.value === key) selectedKey.value = null;
    } catch (e) {
      console.error("Failed to delete key:", e);
    }
  }

  return {
    keys,
    searchQuery,
    typeFilter,
    sortField,
    sortOrder,
    selectedKey,
    loading,
    expandedPaths,
    filteredKeys,
    keyTree,
    keyCount,
    typeDistribution,
    selectKey,
    toggleNode,
    refreshKeys,
    deleteKey,
  };
});
