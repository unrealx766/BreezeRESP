// Simple toast event bus + message history
import { ref } from "vue";

type ToastType = "success" | "error" | "warning" | "info";

interface ToastEvent {
  id: number;
  type: ToastType;
  message: string;
  duration: number;
}

/** A single entry in the message history */
export interface MessageEntry {
  id: number;
  type: ToastType;
  message: string;
  timestamp: number;
  connectionName?: string;
}

type Listener = (event: ToastEvent) => void;

let nextId = 0;
const listeners = new Set<Listener>();

/** Lazy getter for active connection name (avoids circular import) */
let getConnectionName: (() => string | undefined) | null = null;
export function setConnectionNameGetter(fn: () => string | undefined) {
  getConnectionName = fn;
}

/** Reactive message history for the notification panel */
export const messageHistory = ref<MessageEntry[]>([]);

function emit(type: ToastType, message: string, duration: number, connNameOverride?: string) {
  const id = nextId++;
  const event: ToastEvent = { id, type, message, duration };
  listeners.forEach((fn) => fn(event));
  // Record to history with connection context (override takes precedence)
  const connName = connNameOverride ?? getConnectionName?.();
  messageHistory.value.unshift({ id, type, message, timestamp: Date.now(), connectionName: connName });
}

/** Clear all message history */
export function clearMessageHistory() {
  messageHistory.value = [];
}

export const toast = {
  on: (fn: Listener) => { listeners.add(fn); return () => { listeners.delete(fn); }; },
  show: (message: string, type: ToastType = "info", duration = 3000) => emit(type, message, duration),
  success: (msg: string, conn?: string) => emit("success", msg, 3000, conn),
  error: (msg: string, duration = 5000, conn?: string) => emit("error", msg, duration, conn),
  warning: (msg: string, conn?: string) => emit("warning", msg, 3000, conn),
  info: (msg: string, conn?: string) => emit("info", msg, 3000, conn),
};

