export type ToastAction = { label: string; run: () => void };
export const toasts = $state<{ id: number; text: string; action?: ToastAction }[]>([]);
let n = 0;

/** Короткое сообщение внизу окна. С действием (например «Отменить») живёт дольше. */
export function toast(text: string, ms = 2200, action?: ToastAction) {
  const id = ++n;
  toasts.push({ id, text, action });
  setTimeout(() => dismiss(id), action ? Math.max(ms, 6000) : ms);
}

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
