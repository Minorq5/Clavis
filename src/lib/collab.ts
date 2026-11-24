// Мост между редактором (Yjs) и ядром: ядро шифрует правки и доставляет их соавторам.
import * as Y from "yjs";
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate, removeAwarenessStates } from "y-protocols/awareness";
import { api, listen } from "./api";

export const toB64 = (u: Uint8Array) => {
  let s = "";
  for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000));
  return btoa(s);
};
export const fromB64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

/** Приглушённые цвета, различимые на тёмном фоне. */
const COLORS = ["#d9a46c", "#8fb3d9", "#a7c49a", "#d48f9b", "#b6a2d6", "#7cc4bf", "#d6c27a", "#c9a28f"];

export function colorFor(key: string): string {
  let h = 0;
  for (const ch of key) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return COLORS[h % COLORS.length];
}

export interface Peer {
  clientId: number;
  name: string;
  color: string;
  typing: boolean;
}

export class ClavisProvider {
  readonly doc = new Y.Doc();
  readonly awareness: Awareness;
  private offs: Array<() => void> = [];
  private pending: Uint8Array[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private destroyed = false;

  constructor(readonly id: string, initial: Uint8Array, readonly user: { name: string; color: string }) {
    if (initial.length) Y.applyUpdate(this.doc, initial, "remote");
    this.awareness = new Awareness(this.doc);
    this.awareness.setLocalStateField("user", user);

    // Свои правки: объединяем то, что набрано за 40 мс, чтобы не слать по букве.
    this.doc.on("update", (update: Uint8Array, origin: unknown) => {
      if (origin === "remote" || this.destroyed) return;
      this.pending.push(update);
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.sendPending(), 40);
    });

    this.awareness.on("update", ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }, origin: unknown) => {
      if (origin !== "local" || this.destroyed) return;
      const changed = added.concat(updated, removed);
      api.sharedAwareness(id, toB64(encodeAwarenessUpdate(this.awareness, changed))).catch(() => {});
    });

    listen<{ id: string; data: string }>("shared:update", (p) => {
      if (p.id === id && !this.destroyed) Y.applyUpdate(this.doc, fromB64(p.data), "remote");
    }).then((o) => this.offs.push(o));
    listen<{ id: string; data: string }>("shared:awareness", (p) => {
      if (p.id === id && !this.destroyed) applyAwarenessUpdate(this.awareness, fromB64(p.data), "remote");
    }).then((o) => this.offs.push(o));
  }

  private sendPending() {
    if (!this.pending.length) return;
    const merged = this.pending.length === 1 ? this.pending[0] : Y.mergeUpdates(this.pending);
    this.pending = [];
    api.sharedPush(this.id, toB64(merged)).catch(() => {});
  }

  /** Соавторы, которые сейчас в документе (кроме себя). */
  peers(): Peer[] {
    const out: Peer[] = [];
    this.awareness.getStates().forEach((s, clientId) => {
      if (clientId === this.doc.clientID || !s.user) return;
      out.push({ clientId, name: s.user.name, color: s.user.color, typing: !!s.cursor });
    });
    return out;
  }

  destroy() {
    if (this.destroyed) return;
    clearTimeout(this.timer);
    this.sendPending();
    removeAwarenessStates(this.awareness, [this.doc.clientID], "local");
    this.destroyed = true;
    this.offs.forEach((o) => o());
    this.awareness.destroy();
    this.doc.destroy();
  }
}
