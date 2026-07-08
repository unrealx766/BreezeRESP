// Simple toast event bus — import and call directly from anywhere
type ToastType = "success" | "error" | "warning" | "info";

interface ToastEvent {
  id: number;
  type: ToastType;
  message: string;
  duration: number;
}

type Listener = (event: ToastEvent) => void;

let nextId = 0;
const listeners = new Set<Listener>();

function emit(type: ToastType, message: string, duration: number) {
  const event: ToastEvent = { id: nextId++, type, message, duration };
  listeners.forEach((fn) => fn(event));
}

export const toast = {
  on: (fn: Listener) => { listeners.add(fn); return () => { listeners.delete(fn); }; },
  show: (message: string, type: ToastType = "info", duration = 3000) => emit(type, message, duration),
  success: (msg: string) => emit("success", msg, 3000),
  error: (msg: string, duration = 5000) => emit("error", msg, duration),
  warning: (msg: string) => emit("warning", msg, 3000),
  info: (msg: string) => emit("info", msg, 3000),
};
