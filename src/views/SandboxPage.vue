<script setup lang="ts">
import { computed, ref as vueRef } from "vue";
import { useI18n } from "vue-i18n";
import { useSandboxStore } from "@/stores/sandboxStore";
import { useConnectionStore } from "@/stores/connectionStore";
import { writeCommandTemplates } from "@/utils/commandTemplates";
import { truncateValue } from "@/utils/format";
import {
  FlaskConical, Play, Check, History,
  Plus, Minus, Edit3, Terminal, AlertTriangle, Hash,
  RotateCcw, ChevronDown, ChevronRight,
} from "lucide-vue-next";

const { t } = useI18n();
const sandbox = useSandboxStore();
const connStore = useConnectionStore();
const isConnected = computed(() => connStore.activeConnection?.status === "connected");

/** Track which history items have their rollback commands expanded */
const expandedRollback = vueRef<Set<string>>(new Set());

function toggleRollbackDetail(id: string) {
  const s = new Set(expandedRollback.value);
  if (s.has(id)) { s.delete(id); } else { s.add(id); }
  expandedRollback.value = s;
}

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

const commandTemplates = writeCommandTemplates;

function useTemplate(tpl: typeof commandTemplates[0]) {
  sandbox.commandInput = [tpl.cmd, ...tpl.args].join(" ");
}
</script>

<template>
  <div class="h-full flex flex-col p-6 overflow-hidden min-w-[600px]">
    <!-- Header -->
    <div class="flex items-center justify-between mb-4 shrink-0">
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
    <div class="card overflow-hidden mb-4 shrink-0">
      <div class="bg-gray-900 px-4 py-2 flex items-center gap-2">
        <Terminal :size="14" class="text-green-400" />
        <span class="text-xs text-green-400 font-mono">redis&gt;</span>
        <span class="text-xs text-gray-500 font-mono ml-1">sandbox mode</span>
      </div>
      <!-- Command Templates -->
      <div class="flex flex-wrap gap-1.5 px-4 pt-3 pb-1 bg-bg-primary border-t border-border-light">
        <span class="text-[11px] text-text-muted self-center mr-1">{{ t("sandbox.templates") }}:</span>
        <button v-for="tpl in commandTemplates" :key="tpl.label"
          @click="useTemplate(tpl)"
          :disabled="!isConnected"
          class="px-2 py-0.5 text-[11px] font-mono bg-bg-secondary border border-border rounded text-text-secondary hover:border-redis hover:text-redis transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
          {{ tpl.label }}
        </button>
      </div>
      <div class="p-4 bg-bg-primary">
        <div class="flex flex-col sm:flex-row gap-3">
          <div class="flex-1 min-w-0">
            <textarea
              v-model="sandbox.commandInput"
              @keydown="handleKeydown"
              @input="sandbox.commandError = null"
              :placeholder="t('sandbox.placeholder')"
              :disabled="!isConnected"
              rows="2"
              :class="[
                'w-full px-3 py-2 text-sm font-mono bg-bg-secondary border rounded-lg focus:outline-none focus:ring-1 resize-none disabled:opacity-50 disabled:cursor-not-allowed transition-colors',
                sandbox.commandError
                  ? 'border-danger focus:border-danger focus:ring-danger/20'
                  : 'border-border focus:border-redis focus:ring-redis/20'
              ]"
            />
            <p v-if="sandbox.commandError" class="mt-1.5 text-xs text-danger flex items-center gap-1">
              <AlertTriangle :size="12" />
              {{ sandbox.commandError }}
            </p>
          </div>
          <div class="flex flex-row sm:flex-col gap-2 shrink-0">
            <button
              @click="sandbox.executePreview()"
              :disabled="!sandbox.commandInput.trim() || sandbox.executing || !isConnected"
              class="inline-flex items-center justify-center gap-1.5 w-full sm:w-36 h-10 text-sm font-medium text-white bg-redis rounded-lg hover:bg-redis-dark transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              :title="!isConnected ? t('status.noConnection') : ''"
            >
              <Play :size="14" />
              <span>{{ sandbox.executing ? t("sandbox.executing") : t("sandbox.preview") }}</span>
            </button>
            <button
              v-show="sandbox.showPreview"
              @click="sandbox.resetPreview()"
              class="inline-flex items-center justify-center gap-1.5 w-full sm:w-36 h-9 text-xs text-text-secondary bg-bg-primary border border-border rounded-lg hover:bg-bg-hover transition-colors"
            >
              {{ t("common.cancel") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Diff Preview -->
    <transition name="slide-down">
    <div v-if="sandbox.showPreview" class="card overflow-hidden mb-4 shrink-0">
      <!-- Diff Header -->
      <div class="px-4 py-3 bg-bg-primary border-b border-border flex items-center justify-between flex-wrap gap-2">
        <div class="flex items-center gap-3 flex-wrap">
          <h3 class="text-sm font-semibold text-text-primary">{{ t("sandbox.diff") }}</h3>
          <div class="flex items-center gap-2 text-xs flex-wrap">
            <span class="flex items-center gap-1 text-success" v-if="sandbox.addedCount">
              <Plus :size="12" /> {{ sandbox.addedCount }}
            </span>
            <span class="flex items-center gap-1 text-warning" v-if="sandbox.modifiedCount">
              <Edit3 :size="12" /> {{ sandbox.modifiedCount }}
            </span>
            <span class="flex items-center gap-1 text-danger" v-if="sandbox.deletedCount">
              <Minus :size="12" /> {{ sandbox.deletedCount }}
            </span>
            <span class="flex items-center gap-1 text-text-muted" v-if="sandbox.unchangedCount">
              <Hash :size="12" /> {{ sandbox.unchangedCount }}
            </span>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <button v-if="!sandbox.isReadOnly"
            @click="sandbox.applyChange()"
            :disabled="sandbox.applying || !isConnected"
            class="inline-flex items-center justify-center gap-1.5 w-28 h-8 text-xs font-medium text-white bg-success rounded-lg hover:bg-success/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
            <Check :size="13" />
            <span>{{ sandbox.applying ? t("sandbox.applying") : t("sandbox.apply") }}</span>
          </button>
          <span v-else class="badge bg-info/10 text-info text-[10px]">{{ t("sandbox.readOnly") }}</span>
        </div>
      </div>

      <!-- Read-only Command Result -->
      <div v-if="sandbox.commandResult != null" class="p-4">
        <p class="text-[10px] font-semibold text-info uppercase tracking-wider mb-2 flex items-center gap-1">
          <Terminal :size="10" />
          {{ t("sandbox.result") }}
        </p>
        <div class="px-3 py-2 text-xs font-mono bg-info/5 border border-info/20 rounded-lg max-h-64 overflow-y-auto break-all whitespace-pre-wrap min-w-0">
          {{ truncateValue(sandbox.commandResult) }}
        </div>
      </div>

      <!-- Diff Entries -->
      <div v-if="sandbox.currentDiff.length > 0" class="divide-y divide-border-light overflow-y-auto max-h-[400px]">
        <div v-for="(entry, i) in sandbox.currentDiff" :key="i" class="p-4">
          <div class="flex items-center gap-2 mb-2 min-w-0">
            <span class="badge text-[10px] shrink-0"
              :class="{
                'bg-success/10 text-success': entry.changeType === 'added',
                'bg-warning/10 text-warning': entry.changeType === 'modified',
                'bg-danger/10 text-danger': entry.changeType === 'deleted',
                'bg-gray-100 text-text-muted': entry.changeType === 'unchanged',
              }">
              {{ t(`sandbox.${entry.changeType}`) }}
            </span>
            <span v-if="entry.keyType" class="badge text-[9px] bg-purple-50 text-purple-600 shrink-0 font-mono">
              {{ entry.keyType }}
            </span>
            <span class="text-xs font-mono text-text-secondary truncate" :title="entry.path">{{ entry.path }}</span>
          </div>

          <div class="grid grid-cols-1 md:grid-cols-2 gap-3 min-w-0">
            <!-- Before -->
            <div class="min-w-0">
              <p class="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">{{ t("sandbox.before") }}</p>
              <div class="px-3 py-2 text-xs font-mono rounded-lg border min-h-[40px] max-h-48 overflow-y-auto break-all whitespace-pre-wrap"
                :class="entry.changeType === 'unchanged' ? 'bg-gray-50 border-border-light text-text-muted' : (entry.before ? 'bg-danger/5 border-danger/20 text-text-primary' : 'bg-bg-primary border-border-light text-text-muted')">
                {{ truncateValue(entry.before) }}
              </div>
            </div>
            <!-- After -->
            <div class="min-w-0">
              <p class="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1">{{ t("sandbox.after") }}</p>
              <div class="px-3 py-2 text-xs font-mono rounded-lg border min-h-[40px] max-h-48 overflow-y-auto break-all whitespace-pre-wrap"
                :class="entry.changeType === 'unchanged' ? 'bg-gray-50 border-border-light text-text-muted' : (entry.after ? 'bg-success/5 border-success/20 text-text-primary' : 'bg-bg-primary border-border-light text-text-muted')">
                {{ truncateValue(entry.after) }}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
    </transition>

    <!-- History -->
    <div class="card flex-1 min-h-0 flex flex-col">
      <div class="px-4 py-3 border-b border-border-light flex items-center gap-2 shrink-0">
        <History :size="14" class="text-text-muted" />
        <h3 class="text-sm font-semibold text-text-primary">{{ t("sandbox.history") }}</h3>
      </div>
      <div v-if="sandbox.history.length === 0" class="p-8 text-center text-text-muted text-sm">
        {{ t("sandbox.noHistory") }}
      </div>
      <div v-else class="flex-1 min-h-0 overflow-y-auto">
        <div v-for="item in sandbox.history" :key="item.id"
          class="hover:bg-bg-hover/50 transition-colors">
          <!-- Main row -->
          <div class="px-4 py-3 flex items-center gap-3 flex-wrap border-b border-border-light">
            <span class="badge text-[10px] shrink-0"
              :class="{
                'bg-success/10 text-success': item.status === 'applied',
                'bg-warning/10 text-warning': item.status === 'preview',
                'bg-danger/10 text-danger': item.status === 'rolled-back',
              }">
              {{ t(`sandbox.${item.status === 'rolled-back' ? 'rolledBack' : item.status}`) }}
            </span>
            <span class="text-xs font-mono text-text-primary truncate flex-1 min-w-[120px]">{{ item.command }}</span>
            <span class="text-[10px] text-text-muted shrink-0">{{ item.diffCount }} changes</span>
            <span class="text-[10px] text-text-muted shrink-0">{{ formatTime(item.timestamp) }}</span>
            <!-- Rollback / toggle detail -->
            <button
              v-if="item.status === 'applied' && item.rollbackCommands.length > 0"
              @click="toggleRollbackDetail(item.id)"
              class="text-[10px] text-text-muted hover:text-text-secondary shrink-0 flex items-center gap-0.5"
              :title="t('sandbox.previewRollback')"
            >
              <component :is="expandedRollback.has(item.id) ? ChevronDown : ChevronRight" :size="11" />
            </button>
            <button
              v-if="item.status === 'applied'"
              @click="sandbox.rollbackHistoryItem(item.id)"
              :disabled="sandbox.rollingBack || !isConnected"
              class="text-[10px] text-danger hover:underline shrink-0 disabled:opacity-50 disabled:no-underline disabled:cursor-not-allowed min-w-14 text-center"
            >
              {{ sandbox.rollingBack ? t("sandbox.rollingBack") : t("sandbox.rollback") }}
            </button>
            <!-- Toggle detail for rolled-back items -->
            <button
              v-if="item.status === 'rolled-back' && item.rollbackCommands.length > 0"
              @click="toggleRollbackDetail(item.id)"
              class="text-[10px] text-text-muted hover:text-text-secondary shrink-0 flex items-center gap-0.5"
            >
              <component :is="expandedRollback.has(item.id) ? ChevronDown : ChevronRight" :size="11" />
              <span>{{ t("sandbox.rollbackDetail") }}</span>
            </button>
          </div>
          <!-- Rollback commands detail -->
          <transition name="slide-down">
          <div v-if="expandedRollback.has(item.id) && item.rollbackCommands.length > 0"
            class="px-4 py-2 bg-bg-primary border-b border-border-light">
            <p class="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1.5 flex items-center gap-1">
              <RotateCcw :size="10" />
              {{ item.status === 'rolled-back' ? t("sandbox.executedRollback") : t("sandbox.willExecuteRollback") }}
            </p>
            <div class="space-y-1">
              <div v-for="(cmd, idx) in item.rollbackCommands" :key="idx"
                class="flex items-center gap-2 text-xs font-mono">
                <span class="text-[10px] text-text-muted w-4 text-right shrink-0">{{ idx + 1 }}</span>
                <span class="px-2 py-1 bg-bg-secondary border border-border-light rounded text-text-secondary break-all">{{ cmd }}</span>
              </div>
            </div>
          </div>
          </transition>
        </div>
      </div>
    </div>
  </div>
</template>
