// Подтверждения и ввод строки внутри окна Clavis вместо системных confirm() и prompt().
export type DialogReq =
  | { kind: "confirm"; title: string; text?: string; ok: string; danger?: boolean; resolve: (v: boolean) => void }
  | { kind: "prompt"; title: string; text?: string; ok: string; value: string; placeholder?: string; password?: boolean; resolve: (v: string | null) => void };

export const dialog = $state<{ current: DialogReq | null }>({ current: null });

export function ask(o: { title: string; text?: string; ok: string; danger?: boolean }): Promise<boolean> {
  dialog.current?.kind === "confirm" ? dialog.current.resolve(false) : dialog.current?.resolve(null);
  return new Promise((resolve) => (dialog.current = { kind: "confirm", ...o, resolve }));
}

export function askText(o: { title: string; text?: string; ok: string; value?: string; placeholder?: string; password?: boolean }): Promise<string | null> {
  dialog.current?.kind === "confirm" ? dialog.current.resolve(false) : dialog.current?.resolve(null);
  return new Promise((resolve) => (dialog.current = { kind: "prompt", value: "", ...o, resolve }));
}
