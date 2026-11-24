// Ссылки между заметками и учёт времени внутри заметки.
import { Node } from "@tiptap/core";
import { i18n } from "./i18n.svelte";

export type LinkTarget = { id: string; kind: "note" | "shared"; title: string };

export interface NoteLinkOptions {
  resolve: (id: string) => LinkTarget | null;
  onOpen: (t: { id: string; kind: "note" | "shared" }) => void;
}

export const NoteLink = Node.create<NoteLinkOptions>({
  name: "noteLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addOptions: () => ({ resolve: () => null, onOpen: () => {} }),
  addAttributes: () => ({
    id: { default: "", parseHTML: (el) => el.getAttribute("data-id") ?? "", renderHTML: (a) => ({ "data-id": a.id }) },
    kind: { default: "note", parseHTML: (el) => el.getAttribute("data-kind") ?? "note", renderHTML: (a) => ({ "data-kind": a.kind }) },
    title: { default: "", parseHTML: (el) => el.textContent ?? "", renderHTML: () => ({}) },
  }),
  parseHTML: () => [{ tag: "span[data-note-link]" }],
  renderHTML: ({ node, HTMLAttributes }) => ["span", { ...HTMLAttributes, "data-note-link": "", class: "notelink" }, node.attrs.title],
  renderText: ({ node }) => `[[${node.attrs.title}]]`,
  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement("span");
      dom.className = "notelink";
      dom.contentEditable = "false";
      let attrs = node.attrs;
      const paint = () => {
        const live = this.options.resolve(attrs.id);
        dom.textContent = live?.title || attrs.title || "…";
        dom.classList.toggle("missing", !live);
        dom.title = live ? "" : i18n.lang === "ru" ? "заметка удалена" : "note deleted";
      };
      paint();
      dom.addEventListener("mousedown", (e) => {
        e.preventDefault();
        const live = this.options.resolve(attrs.id);
        if (live) this.options.onOpen({ id: live.id, kind: live.kind });
      });
      return {
        dom,
        update: (n) => {
          if (n.type.name !== "noteLink") return false;
          attrs = n.attrs;
          paint();
          return true;
        },
        ignoreMutation: () => true,
      };
    };
  },
});

export type TimeEntry = { s: number; e: number };

export function formatDuration(ms: number): string {
  const m = Math.round(ms / 60000);
  const h = Math.floor(m / 60);
  const r = m % 60;
  if (i18n.lang === "ru") return h ? `${h} ч ${r} мин` : `${r} мин`;
  return h ? `${h}h ${r}m` : `${r}m`;
}

const total = (es: TimeEntry[], now = Date.now()) => es.reduce((a, x) => a + ((x.e || now) - x.s), 0);

export const TimeLog = Node.create({
  name: "timeLog",
  group: "block",
  atom: true,
  selectable: true,
  draggable: true,
  addAttributes: () => ({
    entries: {
      default: [],
      parseHTML: (el) => { try { return JSON.parse(el.getAttribute("data-entries") ?? "[]"); } catch { return []; } },
      renderHTML: (a) => ({ "data-entries": JSON.stringify(a.entries ?? []) }),
    },
  }),
  parseHTML: () => [{ tag: "div[data-time-log]" }],
  renderHTML: ({ node, HTMLAttributes }) => ["div", { ...HTMLAttributes, "data-time-log": "", class: "timelog" }, timeLogText(node.attrs.entries ?? [])],
  addNodeView() {
    return ({ node, getPos, editor }) => {
      const ru = () => i18n.lang === "ru";
      const dom = document.createElement("div");
      dom.className = "timelog";
      dom.contentEditable = "false";
      let entries: TimeEntry[] = node.attrs.entries ?? [];
      let timer: ReturnType<typeof setInterval> | undefined;
      const set = (next: TimeEntry[]) => {
        const pos = typeof getPos === "function" ? getPos() : undefined;
        if (typeof pos !== "number") return;
        editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { entries: next }));
      };
      const fmtTime = (t: number) => new Date(t).toLocaleTimeString(ru() ? "ru-RU" : "en-GB", { hour: "2-digit", minute: "2-digit" });
      const fmtDate = (t: number) => new Date(t).toLocaleDateString(ru() ? "ru-RU" : "en-GB", { day: "numeric", month: "short" });
      const paint = () => {
        const running = entries.some((x) => !x.e);
        dom.replaceChildren();
        const head = document.createElement("div");
        head.className = "tl-head";
        const name = document.createElement("span");
        name.className = "tl-name";
        name.textContent = ru() ? "Учёт времени" : "Time tracking";
        const sum = document.createElement("span");
        sum.className = "tl-total";
        sum.textContent = (ru() ? "Итого " : "Total ") + formatDuration(total(entries));
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "tl-btn" + (running ? " on" : "");
        btn.textContent = running ? (ru() ? "Закончить" : "Stop") : ru() ? "Начать" : "Start";
        btn.onmousedown = (e) => {
          e.preventDefault();
          if (!editor.isEditable) return;
          set(running ? entries.map((x) => (x.e ? x : { ...x, e: Date.now() })) : [...entries, { s: Date.now(), e: 0 }]);
        };
        head.append(name, sum, btn);
        dom.append(head);
        const list = document.createElement("div");
        list.className = "tl-list";
        if (!entries.length) {
          const p = document.createElement("div");
          p.className = "tl-empty";
          p.textContent = ru() ? "Нажмите «Начать», когда приступите к работе." : "Press Start when you begin working.";
          list.append(p);
        }
        entries.forEach((x, i) => {
          const row = document.createElement("div");
          row.className = "tl-row" + (x.e ? "" : " live");
          const a = document.createElement("span");
          a.textContent = `${fmtDate(x.s)}, ${fmtTime(x.s)} — ${x.e ? fmtTime(x.e) : "…"}`;
          const d = document.createElement("span");
          d.className = "tl-dur";
          d.textContent = formatDuration((x.e || Date.now()) - x.s);
          const del = document.createElement("button");
          del.type = "button";
          del.className = "tl-del";
          del.title = ru() ? "Удалить запись" : "Delete entry";
          del.textContent = "×";
          del.onmousedown = (e) => {
            e.preventDefault();
            if (editor.isEditable) set(entries.filter((_, j) => j !== i));
          };
          row.append(a, d, del);
          list.append(row);
        });
        dom.append(list);
        clearInterval(timer);
        if (running) timer = setInterval(paint, 30_000);
      };
      paint();
      return {
        dom,
        update: (n) => {
          if (n.type.name !== "timeLog") return false;
          entries = n.attrs.entries ?? [];
          paint();
          return true;
        },
        destroy: () => clearInterval(timer),
        stopEvent: () => true,
        ignoreMutation: () => true,
      };
    };
  },
});

export function timeLogText(entries: TimeEntry[]): string {
  const ru = i18n.lang === "ru";
  const loc = ru ? "ru-RU" : "en-GB";
  const lines = entries.map((x) => {
    const d = new Date(x.s).toLocaleString(loc, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
    const e = x.e ? new Date(x.e).toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" }) : "…";
    return `${d} — ${e} (${formatDuration((x.e || Date.now()) - x.s)})`;
  });
  return [(ru ? "Учёт времени, итого " : "Time tracking, total ") + formatDuration(total(entries)), ...lines].join("\n");
}

/** Перед экспортом превращает ссылки и учёт времени в обычный текст. */
export function lowerCustom(n: any): any {
  if (!n || typeof n !== "object") return n;
  if (n.type === "noteLink") return { type: "text", text: n.attrs?.title || "…" };
  if (n.type === "timeLog") {
    const [first, ...rest] = timeLogText(n.attrs?.entries ?? []).split("\n");
    return { type: "paragraph", content: [{ type: "text", marks: [{ type: "bold" }], text: first }, ...rest.flatMap((l) => [{ type: "hardBreak" }, { type: "text", text: l }])] };
  }
  return n.content ? { ...n, content: n.content.map(lowerCustom) } : n;
}
