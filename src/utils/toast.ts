// Simple toast event bus + message history
import { ref } from "vue";

type ToastType = "success" | "error" | "warning" | "info";

/** Optional clickable action attached to a toast (e.g. "open file location") */
export interface ToastAction {
  label: string;
  onClick: () => void;
}

interface ToastEvent {
  id: number;
  type: ToastType;
  message: string;
  duration: number;
  action?: ToastAction;
}

/** A single entry in the message history */
export interface MessageEntry {
  id: number;
  type: ToastType;
  message: string;
  timestamp: number;
  connectionName?: string;
  db?: number;
  action?: ToastAction;
}

type Listener = (event: ToastEvent) => void;

let nextId = 0;
const listeners = new Set<Listener>();

/** Lazy getter for active connection name (avoids circular import) */
let getConnectionName: (() => string | undefined) | null = null;
let getConnectionDb: (() => number | undefined) | null = null;
export function setConnectionNameGetter(fn: () => string | undefined) {
  getConnectionName = fn;
}
export function setConnectionDbGetter(fn: () => number | undefined) {
  getConnectionDb = fn;
}

/** Reactive message history for the notification panel */
export const messageHistory = ref<MessageEntry[]>([]);

function emit(type: ToastType, message: string, duration: number, connNameOverride?: string, action?: ToastAction) {
  const id = nextId++;
  const event: ToastEvent = { id, type, message, duration, action };
  listeners.forEach((fn) => fn(event));
  // Record to history with connection context
  const connName = connNameOverride ?? getConnectionName?.();
  const db = getConnectionDb?.();
  messageHistory.value.unshift({ id, type, message, timestamp: Date.now(), connectionName: connName, db, action });
}

/** Clear all message history */
export function clearMessageHistory() {
  messageHistory.value = [];
}

export const toast = {
  on: (fn: Listener) => { listeners.add(fn); return () => { listeners.delete(fn); }; },
  show: (message: string, type: ToastType = "info", duration = 3000) => emit(type, message, duration),
  success: (msg: string, conn?: string, action?: ToastAction) => emit("success", msg, action ? 6000 : 3000, conn, action),
  error: (msg: string, duration = 5000, conn?: string) => emit("error", msg, duration, conn),
  warning: (msg: string, conn?: string) => emit("warning", msg, 3000, conn),
  info: (msg: string, conn?: string, action?: ToastAction) => emit("info", msg, action ? 6000 : 3000, conn, action),
};

