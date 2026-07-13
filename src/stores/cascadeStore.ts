import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { RedisKey, RedisDataType, KeyTreeNode, SortField, SortOrder } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";

export const useCascadeStore = defineStore("cascade", () => {
  const keys = ref<RedisKey[]>([]);
  const searchQuery = ref("");
  const debouncedSearchQuery = ref("");
  const typeFilter = ref<RedisDataType | "all">("all");
  const sortField = ref<SortField>("name");
  const sortOrder = ref<SortOrder>("asc");
  const selectedKey = ref<string | null>(null);
  const loading = ref(false);
  const expandedPaths = ref(new Set<string>());
  const totalKeyCount = ref(0);
  const scanCursor = ref(0);
  let refreshing = false;

  const filteredKeys = computed(() => {
    let result = keys.value;
    // Search is handled server-side via SCAN MATCH pattern
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

  // Flat list of visible tree nodes (expanded children included) for virtual scrolling
  const visibleNodes = computed(() => {
    const nodes: { node: KeyTreeNode; depth: number }[] = [];
    function walk(nodeList: KeyTreeNode[], depth: number) {
      for (const node of nodeList) {
        nodes.push({ node, depth });
        if (!node.key && expandedPaths.value.has(node.fullPath)) {
          walk(node.children, depth + 1);
        }
      }
    }
    walk(keyTree.value, 0);
    return nodes;
  });

  const keyCount = computed(() => keys.value.length);
  const hasMore = computed(() => scanCursor.value !== 0);
  const loadedCount = computed(() => keys.value.length);

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

  async function fetchDbSize(connId: string) {
    try {
      totalKeyCount.value = await tauriApi.cascade.dbSize(connId);
    } catch { /* best-effort */ }
  }

  async function scanBatch(connId: string, cursor: number): Promise<{ keys: RedisKey[]; nextCursor: number }> {
    const maxIterations = 10;
    let iterations = 0;
    const newKeys: RedisKey[] = [];
    const searchPattern = debouncedSearchQuery.value ? `*${debouncedSearchQuery.value}*` : "*";

    do {
      const raw = await tauriApi.cascade.scanKeys(connId, searchPattern, cursor, 2);
      const response = raw as unknown as [number, any[]];
      const nextCursor = response[0];
      const rustKeys = Array.isArray(response[1]) ? response[1] : [];

      newKeys.push(
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

    return { keys: newKeys, nextCursor: cursor };
  }

  async function refreshKeys(force = false) {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) return;
    if (refreshing && !force) return;

    refreshing = true;
    loading.value = true;
    try {
      scanCursor.value = 0;
      const result = await scanBatch(connId, 0);
      keys.value = result.keys;
      scanCursor.value = result.nextCursor;
      await fetchDbSize(connId);
    } catch (e) {
      console.error("Failed to scan keys:", e);
    } finally {
      loading.value = false;
      refreshing = false;
    }
  }

  async function loadMoreKeys() {
    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId || scanCursor.value === 0 || refreshing) return;

    refreshing = true;
    loading.value = true;
    try {
      const result = await scanBatch(connId, scanCursor.value);
      // Deduplicate: filter out keys already present
      const existingKeys = new Set(keys.value.map((k) => k.key));
      const newKeys = result.keys.filter((k) => !existingKeys.has(k.key));
      keys.value = [...keys.value, ...newKeys];
      scanCursor.value = result.nextCursor;
    } catch (e) {
      console.error("Failed to load more keys:", e);
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
    debouncedSearchQuery,
    totalKeyCount,
    filteredKeys,
    keyTree,
    visibleNodes,
    keyCount,
    loadedCount,
    hasMore,
    typeDistribution,
    selectKey,
    toggleNode,
    refreshKeys,
    loadMoreKeys,
    deleteKey,
  };
});
