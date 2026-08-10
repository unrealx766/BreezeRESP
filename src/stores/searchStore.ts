import { defineStore } from "pinia";
import { ref } from "vue";
import type { FtIndexInfo, FtSearchResult } from "@/types";
import { tauriApi } from "@/services/tauriApi";

/**
 * State for the RedisJSON & RediSearch page: index list, the selected
 * index's FT.INFO detail and the latest search results.
 */
export const useSearchStore = defineStore("search", () => {
  const indexes = ref<string[]>([]);
  const loadingIndexes = ref(false);

  const selectedIndex = ref("");
  const indexInfo = ref<FtIndexInfo | null>(null);
  const loadingInfo = ref(false);

  const searchResult = ref<FtSearchResult | null>(null);
  const searching = ref(false);

  async function loadIndexes(connectionId: string) {
    loadingIndexes.value = true;
    try {
      indexes.value = await tauriApi.jsonsearch.ftList(connectionId);
    } finally {
      loadingIndexes.value = false;
    }
  }

  async function loadIndexInfo(connectionId: string, index: string) {
    selectedIndex.value = index;
    loadingInfo.value = true;
    try {
      indexInfo.value = await tauriApi.jsonsearch.ftInfo(connectionId, index);
    } finally {
      loadingInfo.value = false;
    }
  }

  async function search(
    connectionId: string,
    index: string,
    query: string,
    options?: { offset?: number; limit?: number; params?: Array<[string, number[]]>; withScores?: boolean },
  ) {
    searching.value = true;
    try {
      searchResult.value = await tauriApi.jsonsearch.ftSearch({
        connectionId,
        index,
        query,
        ...options,
      });
    } finally {
      searching.value = false;
    }
  }

  function reset() {
    indexes.value = [];
    selectedIndex.value = "";
    indexInfo.value = null;
    searchResult.value = null;
  }

  return {
    indexes, loadingIndexes,
    selectedIndex, indexInfo, loadingInfo,
    searchResult, searching,
    loadIndexes, loadIndexInfo, search,
    reset,
  };
});
