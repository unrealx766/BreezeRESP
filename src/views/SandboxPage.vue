<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useSandboxStore } from "@/stores/sandboxStore";
import {
  FlaskConical, Play, Check, RotateCcw, History,
  Plus, Minus, Edit3, Terminal, AlertTriangle,
} from "lucide-vue-next";

const { t } = useI18n();
const sandbox = useSandboxStore();

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    sandbox.executePreview();
  }
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d.getSeconds().toString().padStart(2, "0")}`;
}
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between mb-4">
      <div>
        <h2 class="text-xl font-semibold text-text-primary flex items-center gap-2">
          <FlaskConical :size="20" class="text-redis" />
          {{ t("sandbox.title") }}
        </h2>
        <p class="text-sm text-text-muted mt-1 flex items-center gap-1.5">
          <AlertTriangle :size="12" class="text-warning" />
          {{ t("sandbox.hint") }}
        </p>
      </div>
    </div>

    <!-- Command Input Terminal -->
    <div class="card overflow-hidden mb-4">
      <div class="bg-gray-900 px-4 py-2 flex items-center gap-2">
        <Terminal :size="14" class="text-green-400" />
        <span class="text-xs text-green-400 font-mono">redis&gt;</span>
        <span class="text-xs text-gray-500 font-mono ml-1">sandbox mode</span>
      </div>
      <div class="p-4 bg-bg-primary">
        <div class="flex gap-3">
          <div class="flex-1">
            <textarea
              v-model="sandbox.commandInput"
              @keydown="handleKeydown"
              :placeholder="t('sandbox.placeholder')"
              rows="2"
              class="w-full px-3 py-2 text-sm font-mono bg-white border border-border rounded-lg focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20 resize-none"
            />
          </div>
          <div class="flex flex-col gap-2 shrink-0">
            <button
              @click="sandbox.executePreview()"
              :disabled="!sandbox.commandInput.trim() || sandbox.executing"
              class="inline-flex items-center gap-1.5 px-4 py-2 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50"
            >
              <Play :size="14" /> {{ sandbox.executing ? t("sandbox.executing") : t("sandbox.preview") }}
            </button>
            <button
              v-if="sandbox.showPreview"
              @click="sandbox.resetPreview()"
              class="inline-flex items-center gap-1.5 px-4 py-1.5 text-xs text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Diff Preview -->
    <div v-if="sandbox.showPreview" class="card overflow-hidden mb-4">
      <!-- Diff Header -->
      <div class="px-4 py-3 bg-bg-primary border-b border-border flex items-center justify-between">
        <div class="flex items-center gap-3">
          <h3 class="text-sm font-semibold text-text-primary">{{ t("sandbox.diff") }}</h3>
          <div class="flex items-center gap-2 text-xs">
            <span class="flex items-center gap-1 text-success" v-if="sandbox.addedCount">
              <Plus :size="12" /> {{ sandbox.addedCount }}
            </span>
            <span class="flex items-center gap-1 text-warning" v-if="sandbox.modifiedCount">
              <Edit3 :size="12" /> {{ sandbox.modifiedCount }}
            </span>
            <span class="flex items-center gap-1 text-danger" v-if="sandbox.deletedCount">
              <Minus :size="12" /> {{ sandbox.deletedCount }}
            </span>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <button @click="sandbox.applyChange()"
            class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-white bg-success rounded-lg hover:bg-success/90 transition-colors">
            <Check :size="13" /> {{ t("sandbox.apply") }}
          </button>
          <button @click="sandbox.rollbackChange()"
            class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors">
            <RotateCcw :size="13" /> {{ t("sandbox.rollback") }}
          </button>
        </div>
      </div>

      <!-- Diff Entries -->
      <div class="divide-y divide-border-light">
        <div v-for="(entry, i) in sandbox.currentDiff" :key="i" class="p-4">
          <div class="flex items-center gap-2 mb-2">
            <span class="badge text-[10px]"
              :class="{
                'bg-success/10 text-success': entry.changeType === 'added',
                'bg-warning/10 text-warning': entry.changeType === 'modified',
                'bg-danger/10 text-danger': entry.changeType === 'deleted',
              }">
              {{ t(`sandbox.${entry.changeType}`) }}
            </span>
            <span class="text-xs font-mono text-text-secondary">{{ entry.path }}</span>
          </div>

          <div class="grid grid-cols-2 gap-3">
            <!-- Before -->
            <div>
              <p class="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">{{ t("sandbox.before") }}</p>
              <div class="px-3 py-2 text-xs font-mono rounded-lg border min-h-[40px]"
                :class="entry.before ? 'bg-danger/5 border-danger/20 text-text-primary' : 'bg-bg-primary border-border-light text-text-muted'">
                {{ entry.before ?? '(empty)' }}
              </div>
            </div>
            <!-- After -->
            <div>
              <p class="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">{{ t("sandbox.after") }}</p>
              <div class="px-3 py-2 text-xs font-mono rounded-lg border min-h-[40px]"
                :class="entry.after ? 'bg-success/5 border-success/20 text-text-primary' : 'bg-bg-primary border-border-light text-text-muted'">
                {{ entry.after ?? '(empty)' }}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- History -->
    <div class="card">
      <div class="px-4 py-3 border-b border-border-light flex items-center gap-2">
        <History :size="14" class="text-text-muted" />
        <h3 class="text-sm font-semibold text-text-primary">{{ t("sandbox.history") }}</h3>
      </div>
      <div v-if="sandbox.history.length === 0" class="p-8 text-center text-text-muted text-sm">
        {{ t("sandbox.noHistory") }}
      </div>
      <div v-else class="divide-y divide-border-light">
        <div v-for="item in sandbox.history" :key="item.id"
          class="px-4 py-3 flex items-center gap-3 hover:bg-bg-hover/50 transition-colors">
          <span class="badge text-[10px] shrink-0"
            :class="{
              'bg-success/10 text-success': item.status === 'applied',
              'bg-warning/10 text-warning': item.status === 'preview',
              'bg-danger/10 text-danger': item.status === 'rolled-back',
            }">
            {{ t(`sandbox.${item.status === 'rolled-back' ? 'rolledBack' : item.status}`) }}
          </span>
          <span class="text-xs font-mono text-text-primary truncate flex-1">{{ item.command }}</span>
          <span class="text-[10px] text-text-muted shrink-0">{{ item.diffCount }} changes</span>
          <span class="text-[10px] text-text-muted shrink-0">{{ formatTime(item.timestamp) }}</span>
          <button
            v-if="item.status === 'applied'"
            @click="sandbox.rollbackHistoryItem(item.id)"
            class="text-[10px] text-danger hover:underline shrink-0"
          >
            {{ t("sandbox.rollback") }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
