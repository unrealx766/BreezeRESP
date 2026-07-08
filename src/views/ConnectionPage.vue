<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import type { RedisConnection } from "@/types";
import {
  Plus, Server, Wifi, WifiOff, Trash2, Edit3, Zap, AlertCircle,
  X, Loader2, Lock, Unlock,
} from "lucide-vue-next";

const router = useRouter();
const { t } = useI18n();
const connStore = useConnectionStore();

const showForm = ref(false);
const editingId = ref<string | null>(null);
const testing = ref<string | null>(null);
const testResult = ref<{ id: string; ok: boolean } | null>(null);
const formTesting = ref(false);
const formTestResult = ref<{ ok: boolean } | null>(null);
const formError = ref("");
const connectError = ref<string | null>(null);
const usePassword = ref(false);
const hadPassword = ref(false);

const form = ref({
  name: "",
  host: "127.0.0.1",
  port: 6379,
  password: "",
  db: 0,
  ssl: false,
});

function openNew() {
  editingId.value = null;
  form.value = { name: "", host: "127.0.0.1", port: 6379, password: "", db: 0, ssl: false };
  formError.value = "";
  formTestResult.value = null;
  usePassword.value = false;
  hadPassword.value = false;
  showForm.value = true;
}

function openEdit(conn: RedisConnection) {
  editingId.value = conn.id;
  form.value = { name: conn.name, host: conn.host, port: conn.port, password: "", db: conn.db, ssl: conn.ssl };
  formError.value = "";
  formTestResult.value = null;
  usePassword.value = !!conn.password;
  hadPassword.value = !!conn.password;
  showForm.value = true;
}

async function saveForm() {
  if (!form.value.name.trim()) {
    formError.value = t("connection.nameRequired");
    return;
  }
  const data = { ...form.value };
  if (!usePassword.value) {
    data.password = "";
  }
  if (editingId.value) {
    // Preserve old password only if toggle was on AND user didn't type a new one
    if (usePassword.value && !data.password && hadPassword.value) {
      data.password = undefined as any; // signal to keep old password
    }
    const wasConnected = connStore.connections.find((c) => c.id === editingId.value)?.status === "connected";
    await connStore.updateConnection(editingId.value, { ...data });
    showForm.value = false;
    // Reconnect in background if was connected (card shows loading state)
    if (wasConnected) {
      reconnectInBackground(editingId.value);
    }
  } else {
    connStore.addConnection({ ...data, lastUsed: undefined });
    showForm.value = false;
  }
}

function reconnectInBackground(id: string) {
  connStore.disconnect(id).then(() => connStore.connect(id)).then((ok) => {
    if (!ok) {
      connectError.value = connStore.lastError || t("connection.connectFailed");
    }
  });
}

async function handleFormTest() {
  formTesting.value = true;
  formTestResult.value = null;
  const ok = await connStore.testFormConnection(form.value);
  formTesting.value = false;
  formTestResult.value = { ok };
  setTimeout(() => { formTestResult.value = null; }, 3000);
}

async function handleConnect(id: string) {
  connectError.value = null;
  const ok = await connStore.connect(id);
  if (ok) {
    router.push("/browser");
  } else {
    connectError.value = connStore.lastError || t("connection.connectFailed");
  }
}

async function handleTest(conn: RedisConnection) {
  testing.value = conn.id;
  testResult.value = null;
  const ok = await connStore.testConnection(conn.id);
  testing.value = null;
  testResult.value = { id: conn.id, ok };
  setTimeout(() => { testResult.value = null; }, 2000);
}

function handleDelete(conn: RedisConnection) {
  if (confirm(t("connection.confirmDelete", { name: conn.name }))) {
    connStore.removeConnection(conn.id);
  }
}

function statusColor(status: string) {
  return {
    connected: "text-success",
    disconnected: "text-text-muted",
    connecting: "text-warning",
    error: "text-danger",
  }[status] || "text-text-muted";
}
</script>

<template>
  <div class="h-full p-6 overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-xl font-semibold text-text-primary">{{ t("connection.title") }}</h2>
        <p class="text-sm text-text-muted mt-1">
          {{ t("connection.summary", { total: connStore.connections.length, active: connStore.connectedCount }) }}
        </p>
      </div>
      <button
        @click="openNew"
        class="inline-flex items-center gap-2 px-4 py-2 bg-redis text-white rounded-lg text-sm font-medium hover:bg-redis-dark transition-colors shadow-sm"
      >
        <Plus :size="16" />
        {{ t("connection.newConnection") }}
      </button>
    </div>

    <!-- Connection Error Banner -->
    <div
      v-if="connectError"
      class="mb-4 flex items-start gap-2 px-4 py-3 bg-danger/5 border border-danger/20 rounded-lg"
    >
      <AlertCircle :size="16" class="text-danger mt-0.5 shrink-0" />
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium text-danger">{{ t("connection.connectFailed") }}</p>
        <p class="text-xs text-text-muted mt-0.5 break-all">{{ connectError }}</p>
      </div>
      <button @click="connectError = null" class="shrink-0 text-text-muted hover:text-text-primary">
        <X :size="14" />
      </button>
    </div>

    <!-- Empty State -->
    <div
      v-if="connStore.connections.length === 0"
      class="flex flex-col items-center justify-center py-24"
    >
      <div class="w-16 h-16 rounded-2xl bg-redis-light flex items-center justify-center mb-4">
        <Server :size="32" class="text-redis" />
      </div>
      <h3 class="text-lg font-medium text-text-primary mb-1">{{ t("connection.emptyTitle") }}</h3>
      <p class="text-sm text-text-muted mb-4">{{ t("connection.emptyDesc") }}</p>
      <button
        @click="openNew"
        class="inline-flex items-center gap-2 px-4 py-2 bg-redis text-white rounded-lg text-sm font-medium hover:bg-redis-dark transition-colors"
      >
        <Plus :size="16" />
        {{ t("connection.newConnection") }}
      </button>
    </div>

    <!-- Connection Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
      <div
        v-for="conn in connStore.connections"
        :key="conn.id"
        class="card card-hover p-4 group relative"
      >
        <!-- Status indicator -->
        <div class="flex items-start justify-between mb-3">
          <div class="flex items-center gap-2.5">
            <div
              class="w-9 h-9 rounded-lg flex items-center justify-center"
              :class="conn.status === 'connected' ? 'bg-success/10' : 'bg-bg-primary'"
            >
              <Server :size="18" :class="statusColor(conn.status)" />
            </div>
            <div>
              <h3 class="text-sm font-semibold text-text-primary truncate max-w-[160px]" :title="conn.name">{{ conn.name }}</h3>
              <p class="text-xs text-text-muted">{{ conn.host }}:{{ conn.port }}</p>
            </div>
          </div>
          <span
            class="w-2.5 h-2.5 rounded-full mt-1"
            :class="{
              'bg-success': conn.status === 'connected',
              'bg-gray-300': conn.status === 'disconnected',
              'bg-warning animate-pulse': conn.status === 'connecting',
              'bg-danger': conn.status === 'error',
            }"
          />
        </div>

        <!-- Info -->
        <div class="flex items-center gap-3 mb-4 text-xs text-text-muted">
          <span>DB{{ conn.db }}</span>
          <span v-if="conn.ssl" class="badge bg-info/10 text-info">SSL</span>
          <span v-if="conn.password" class="badge bg-bg-primary text-text-muted">Auth</span>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-2">
          <button
            v-if="conn.status !== 'connected'"
            @click="handleConnect(conn.id)"
            :disabled="conn.status === 'connecting'"
            class="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 bg-redis text-white rounded-lg text-xs font-medium hover:bg-redis-dark transition-colors disabled:opacity-60"
          >
            <Wifi :size="12" />
            {{ conn.status === "connecting" ? t("connection.connecting") : t("connection.connect") }}
          </button>
          <button
            v-else
            @click="connStore.disconnect(conn.id)"
            class="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 bg-bg-primary text-text-secondary rounded-lg text-xs font-medium hover:bg-bg-hover transition-colors"
          >
            <WifiOff :size="12" />
            {{ t("connection.disconnect") }}
          </button>
          <button
            @click="handleTest(conn)"
            :disabled="conn.status === 'connected' || conn.status === 'connecting'"
            class="w-8 h-8 flex items-center justify-center rounded-lg transition-colors relative disabled:opacity-30 disabled:cursor-not-allowed hover:bg-bg-hover"
            :title="conn.status === 'connected' ? t('connection.alreadyConnected') : t('connection.testConnection')"
          >
            <Loader2 v-if="testing === conn.id" :size="14" class="animate-spin text-text-muted" />
            <Zap v-else :size="14" class="text-text-muted" />
            <!-- Test result tooltip -->
            <div
              v-if="testResult?.id === conn.id"
              class="absolute -top-8 left-1/2 -translate-x-1/2 px-2 py-1 rounded text-[10px] font-medium whitespace-nowrap z-10"
              :class="testResult.ok ? 'bg-success text-white' : 'bg-danger text-white'"
            >
              {{ testResult.ok ? t("connection.testSuccess") : t("connection.error") }}
            </div>
          </button>
          <button
            @click="openEdit(conn)"
            class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-bg-hover transition-colors opacity-0 group-hover:opacity-100"
          >
            <Edit3 :size="14" class="text-text-muted" />
          </button>
          <button
            @click="handleDelete(conn)"
            class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-danger/10 transition-colors opacity-0 group-hover:opacity-100"
          >
            <Trash2 :size="14" class="text-danger" />
          </button>
        </div>
      </div>
    </div>

    <!-- Form Modal -->
    <Teleport to="body">
      <div v-if="showForm" class="fixed inset-0 z-50 flex items-center justify-center">
        <div class="absolute inset-0 bg-black/30 backdrop-blur-sm" @click="showForm = false" />
        <div class="relative bg-white rounded-xl shadow-xl w-[440px] max-h-[90vh] overflow-y-auto p-6">
          <div class="flex items-center justify-between mb-5">
            <h3 class="text-base font-semibold text-text-primary">
              {{ editingId ? t("connection.editConnection") : t("connection.newConnection") }}
            </h3>
            <button @click="showForm = false" class="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-bg-hover">
              <X :size="16" class="text-text-muted" />
            </button>
          </div>

          <div class="space-y-4">
            <div>
              <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t("connection.name") }}</label>
              <input v-model="form.name" maxlength="50" class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20" />
            </div>
            <div class="grid grid-cols-3 gap-3">
              <div class="col-span-2">
                <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t("connection.host") }}</label>
                <input v-model="form.host" class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20" />
              </div>
              <div>
                <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t("connection.port") }}</label>
                <input v-model.number="form.port" type="number" class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20" />
              </div>
            </div>
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <label class="text-xs font-medium text-text-secondary">{{ t("connection.password") }}</label>
                <button
                  type="button"
                  @click="usePassword = !usePassword"
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium transition-colors"
                  :class="usePassword
                    ? 'bg-redis-light text-redis hover:bg-redis-light/80'
                    : 'bg-bg-primary text-text-muted hover:bg-bg-hover'"
                >
                  <Lock v-if="usePassword" :size="10" />
                  <Unlock v-else :size="10" />
                  {{ usePassword ? t("connection.requirePassword") : t("connection.noPassword") }}
                </button>
              </div>
              <input
                v-if="usePassword"
                v-model="form.password"
                type="password"
                :placeholder="editingId ? t('connection.passwordUnchanged') : ''"
                class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20"
              />
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="block text-xs font-medium text-text-secondary mb-1.5">{{ t("connection.db") }}</label>
                <input v-model.number="form.db" type="number" min="0" max="15" class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-bg-primary focus:outline-none focus:border-redis focus:ring-1 focus:ring-redis/20" />
              </div>
              <div class="flex items-end pb-1">
                <label class="flex items-center gap-2 cursor-pointer">
                  <input v-model="form.ssl" type="checkbox" class="w-4 h-4 rounded border-border text-redis focus:ring-redis/20" />
                  <span class="text-sm text-text-secondary">{{ t("connection.ssl") }}</span>
                </label>
              </div>
            </div>
          </div>

          <div class="flex justify-between gap-2 mt-6">
            <button
              @click="handleFormTest"
              :disabled="formTesting"
              class="inline-flex items-center gap-1.5 px-3 py-2 border border-border text-sm text-text-secondary rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-60"
            >
              <Loader2 v-if="formTesting" :size="14" class="animate-spin" />
              <Zap v-else :size="14" />
              {{ t("connection.testFromForm") }}
              <span v-if="formTestResult" class="ml-1" :class="formTestResult.ok ? 'text-success' : 'text-danger'">
                {{ formTestResult.ok ? '✓' : '✗' }}
              </span>
            </button>
            <div class="flex gap-2">
              <button @click="showForm = false" class="px-4 py-2 text-sm text-text-secondary hover:bg-bg-hover rounded-lg transition-colors">
                {{ t("common.cancel") }}
              </button>
              <button
                @click="saveForm"
                class="px-4 py-2 bg-redis text-white text-sm font-medium rounded-lg hover:bg-redis-dark transition-colors"
              >
                {{ t("common.save") }}
              </button>
            </div>
          </div>

          <!-- Form error -->
          <div v-if="formError" class="mt-3 flex items-center gap-1.5 text-xs text-danger">
            <AlertCircle :size="12" />
            {{ formError }}
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
