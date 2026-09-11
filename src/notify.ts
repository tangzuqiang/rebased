import { ref } from "vue";

export type ToastKind = "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

const toasts = ref<Toast[]>([]);
let seq = 0;

export function notify(text: string, kind: ToastKind = "success", ms = 2800) {
  const id = ++seq;
  toasts.value = [...toasts.value.slice(-4), { id, kind, text }];
  window.setTimeout(() => {
    toasts.value = toasts.value.filter((item) => item.id !== id);
  }, ms);
}

export function useToasts() {
  return toasts;
}
