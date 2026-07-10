import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { PipelineCommand, SavedPipeline } from "@/types";
import { tauriApi } from "@/services/tauriApi";
import { useConnectionStore } from "./connectionStore";
import { useCascadeStore } from "./cascadeStore";
import { toast } from "@/utils/toast";

export const usePipelineStore = defineStore("pipeline", () => {
  const commands = ref<PipelineCommand[]>([]);
  const executing = ref(false);
  const totalLatency = ref<number | null>(null);
  const individualLatencySum = ref<number | null>(null);
  const lastError = ref<string | null>(null);
  const savedPipelines = ref<SavedPipeline[]>([]);

  const commandCount = computed(() => commands.value.length);
  const executedCount = computed(() => commands.value.filter((c) => c.result !== undefined).length);
  const hasResults = computed(() => commands.value.some((c) => c.result !== undefined));

  const rttSaving = computed(() => {
    if (totalLatency.value === null || individualLatencySum.value === null) return null;
    if (individualLatencySum.value === 0) return 0;
    return Math.round(((individualLatencySum.value - totalLatency.value) / individualLatencySum.value) * 100);
  });

  function addCommand(command = "", args: string[] = []) {
    commands.value.push({
      id: `cmd-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      command,
      args,
      result: undefined,
    });
  }

  function removeCommand(id: string) {
    commands.value = commands.value.filter((c) => c.id !== id);
  }

  function updateCommand(id: string, patch: Partial<PipelineCommand>) {
    const idx = commands.value.findIndex((c) => c.id === id);
    if (idx !== -1) commands.value[idx] = { ...commands.value[idx], ...patch };
  }

  function reorderCommands(fromIdx: number, toIdx: number) {
    const arr = [...commands.value];
    const [moved] = arr.splice(fromIdx, 1);
    arr.splice(toIdx, 0, moved);
    commands.value = arr;
  }

  async function executeAll() {
    lastError.value = null;

    const connStore = useConnectionStore();
    const connId = connStore.activeConnectionId;
    if (!connId) {
      toast.error("Not connected. Please connect first.");
      return;
    }

    // Filter out commands with empty command names
    const validCommands = commands.value.filter(
      (cmd) => cmd.command.trim().length > 0
    );
    if (validCommands.length === 0) {
      toast.error("No valid commands to execute.");
      return;
    }

    executing.value = true;
    totalLatency.value = null;
    individualLatencySum.value = null;

    try {
      // Map frontend commands to Rust format
      const rustCommands = validCommands.map((cmd) => ({
        command: cmd.command.trim().toUpperCase(),
        args: cmd.args,
      }));

      const response = await tauriApi.pipeline.execute(connId, rustCommands);

      // Map results back to commands
      for (let i = 0; i < validCommands.length && i < response.results.length; i++) {
        const r = response.results[i];
        validCommands[i].result = {
          success: r.success,
          value: r.value,
          error: r.error ?? undefined,
          latencyMs: r.latencyMs,
        };
      }

      totalLatency.value = response.totalLatencyMs;
      individualLatencySum.value = response.individualSumMs;

      // Refresh browser keys after pipeline execution (data may have changed)
      try {
        const cascade = useCascadeStore();
        await cascade.refreshKeys(true);
      } catch {
        // cascade refresh is best-effort
      }
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message || String(e);
      toast.error(msg);
      console.error("Pipeline execution failed:", e);
    } finally {
      executing.value = false;
    }
  }

  function clearResults() {
    for (const cmd of commands.value) cmd.result = undefined;
    totalLatency.value = null;
    individualLatencySum.value = null;
    lastError.value = null;
  }

  function clearAll() {
    commands.value = [];
    clearResults();
  }

  async function saveCurrentPipeline(name: string) {
    const validCommands = commands.value.filter((cmd) => cmd.command.trim().length > 0);
    if (validCommands.length === 0) return;

    const id = `pipe-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const rustCommands = validCommands.map((c) => ({ command: c.command, args: c.args }));
    const createdAt = Math.floor(Date.now() / 1000);

    await tauriApi.pipeline.save(id, name, rustCommands, createdAt);
    await loadSavedPipelines();
  }

  async function updatePipeline(pipelineId: string, name: string) {
    const validCommands = commands.value.filter((cmd) => cmd.command.trim().length > 0);
    if (validCommands.length === 0) return;

    const rustCommands = validCommands.map((c) => ({ command: c.command, args: c.args }));
    const existing = savedPipelines.value.find((p) => p.id === pipelineId);
    const createdAt = existing?.createdAt ?? Math.floor(Date.now() / 1000);

    await tauriApi.pipeline.save(pipelineId, name, rustCommands, createdAt);
    await loadSavedPipelines();
  }

  async function loadSavedPipelines() {
    try {
      savedPipelines.value = await tauriApi.pipeline.list();
    } catch (e) {
      console.error("Failed to load saved pipelines:", e);
    }
  }

  async function deletePipeline(id: string) {
    await tauriApi.pipeline.delete(id);
    await loadSavedPipelines();
  }

  function loadPipeline(saved: SavedPipeline) {
    clearAll();
    for (const cmd of saved.commands) {
      addCommand(cmd.command, cmd.args);
    }
  }

  return {
    commands,
    executing,
    totalLatency,
    individualLatencySum,
    lastError,
    savedPipelines,
    commandCount,
    executedCount,
    hasResults,
    rttSaving,
    addCommand,
    removeCommand,
    updateCommand,
    reorderCommands,
    executeAll,
    clearResults,
    clearAll,
    saveCurrentPipeline,
    updatePipeline,
    loadSavedPipelines,
    deletePipeline,
    loadPipeline,
  };
});
