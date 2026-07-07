<script setup lang="ts">
import { computed } from "vue";
import type { KeyTreeNode, RedisDataType } from "@/types";
import { useCascadeStore } from "@/stores/cascadeStore";
import {
  ChevronRight, ChevronDown, FolderOpen, Folder,
  Type, Hash, List, CircleDot, BarChart3,
} from "lucide-vue-next";

const props = defineProps<{
  node: KeyTreeNode;
  depth: number;
}>();

const emit = defineEmits<{
  select: [node: KeyTreeNode];
}>();

const cascade = useCascadeStore();

const typeIcons: Record<RedisDataType, any> = {
  string: Type,
  hash: Hash,
  list: List,
  set: CircleDot,
  zset: BarChart3,
};

function formatTtl(ttl: number): string {
  if (ttl === -1) return "∞";
  if (ttl < 60) return `${ttl}s`;
  if (ttl < 3600) return `${Math.floor(ttl / 60)}m`;
  return `${Math.floor(ttl / 3600)}h`;
}

const isLeaf = computed(() => !!props.node.key);
const isSelected = computed(() => !!props.node.key && cascade.selectedKey === props.node.key.key);
const isExpanded = computed(() => cascade.expandedPaths.has(props.node.fullPath));
</script>

<template>
  <div
    class="flex items-center gap-1.5 px-2 py-1 cursor-pointer text-xs transition-colors rounded-md mx-1"
    :class="isSelected ? 'bg-redis/8 text-redis' : 'hover:bg-bg-hover text-text-secondary'"
    :style="{ paddingLeft: `${depth * 16 + 8}px` }"
    @click="emit('select', node)"
  >
    <template v-if="!isLeaf">
      <ChevronDown v-if="isExpanded" :size="12" class="text-text-muted shrink-0" />
      <ChevronRight v-else :size="12" class="text-text-muted shrink-0" />
      <FolderOpen v-if="isExpanded" :size="12" class="text-warning shrink-0" />
      <Folder v-else :size="12" class="text-warning shrink-0" />
    </template>
    <template v-else>
      <component :is="typeIcons[node.key!.type]" :size="12" :class="`text-type-${node.key!.type} shrink-0`" />
    </template>

    <span class="truncate">{{ node.label }}</span>

    <span v-if="isLeaf && node.key && node.key.ttl > 0" class="ml-auto text-[10px] text-text-muted shrink-0">
      {{ formatTtl(node.key.ttl) }}
    </span>
  </div>
</template>
