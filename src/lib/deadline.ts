// Дедлайн внутри текста заметки: «@30.11», «@30.11-18:00», «@18:00», «@завтра».
import { Node, InputRule } from "@tiptap/core";
import { i18n } from "./i18n.svelte";

const DAY = 86_400_000;

/** Разбирает запись срока. Без времени срок ставится на 9:00. */
export function parseDeadline(src: string, now = new Date()): number | null {
  const s = src.trim().toLowerCase().replace(/[.,;:!?)]+$/, (m) => (/^\d/.test(m) ? m : ""));
  const m = /^(?:(завтра|сегодня|послезавтра|tomorrow|today)|(\d{1,2})\.(\d{1,2})(?:\.(\d{2}|\d{4}))?)?(?:(?:^|[-/в t])(\d{1,2}):(\d{2}))?$/u.exec(s);
  if (!m || (!m[1] && !m[2] && !m[5])) return null;
  let d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (m[1]) {
    const add = m[1] === "завтра" || m[1] === "tomorrow" ? 1 : m[1] === "послезавтра" ? 2 : 0;
    d = new Date(d.getTime() + add * DAY);
  } else if (m[2]) {
    const day = +m[2], mon = +m[3] - 1;
    let year = m[4] ? (m[4].length === 2 ? 2000 + +m[4] : +m[4]) : now.getFullYear();
    if (mon < 0 || mon > 11 || day < 1 || day > 31) return null;
    d = new Date(year, mon, day);
    if (d.getMonth() !== mon) return null;
    // Без года: если дата уже прошла, имеется в виду следующий год.
    if (!m[4] && d.getTime() < now.getTime() - DAY) d = new Date(++year, mon, day);
  }
  let h = 9, min = 0;
  if (m[5]) {
    h = +m[5]; min = +m[6];
    if (h > 23 || min > 59) return null;
  }
  d.setHours(h, min, 0, 0);
  // Одно время без даты: сегодня, а если уже прошло, то завтра.
  if (!m[1] && !m[2] && d.getTime() < now.getTime()) d = new Date(d.getTime() + DAY);
  return d.getTime();
}

export function formatDeadline(at: number, now = Date.now()): string {
  const d = new Date(at);
  const loc = i18n.lang === "ru" ? "ru-RU" : "en-GB";
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  const date = d.toLocaleDateString(loc, { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) }).replace(/\.$/, "");
  const time = d.getHours() === 9 && d.getMinutes() === 0 ? "" : " " + d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" });
  return date + time;
}

/** Значение для поля datetime-local. */
export const toLocalInput = (at: number) => {
  const d = new Date(at);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
};

export interface DeadlineOptions { onClick: (pos: number, rect: DOMRect) => void }

export const Deadline = Node.create<DeadlineOptions>({
  name: "deadline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addOptions: () => ({ onClick: () => {} }),
  addAttributes: () => ({
    at: { default: 0, parseHTML: (el) => Number(el.getAttribute("data-at")) || 0, renderHTML: (a) => ({ "data-at": String(a.at) }) },
  }),
  parseHTML: () => [{ tag: "span[data-deadline]" }],
  renderHTML: ({ node, HTMLAttributes }) => ["span", { ...HTMLAttributes, "data-deadline": "", class: "deadline" }, formatDeadline(node.attrs.at)],
  renderText: ({ node }) => "@" + formatDeadline(node.attrs.at),
  addNodeView() {
    return ({ node, getPos }) => {
      const dom = document.createElement("span");
      dom.className = "deadline";
      dom.contentEditable = "false";
      const paint = (at: number) => {
        dom.textContent = formatDeadline(at);
        dom.classList.toggle("overdue", at < Date.now());
        dom.title = new Date(at).toLocaleString(i18n.lang === "ru" ? "ru-RU" : "en-GB");
      };
      paint(node.attrs.at);
      dom.addEventListener("mousedown", (e) => {
        e.preventDefault();
        const pos = typeof getPos === "function" ? getPos() : undefined;
        if (typeof pos === "number") this.options.onClick(pos, dom.getBoundingClientRect());
      });
      return {
        dom,
        update: (n) => {
          if (n.type.name !== "deadline") return false;
          paint(n.attrs.at);
          return true;
        },
        ignoreMutation: () => true,
      };
    };
  },
  addInputRules() {
    return [
      new InputRule({
        find: /(?:^|\s)(@([\p{L}\d.:\-/]+))\s$/u,
        handler: ({ state, range, match, chain }) => {
          const at = parseDeadline(match[2]);
          if (at === null) return null;
          if (state.selection.$from.parent.type.name === "codeBlock") return null;
          const start = range.from + match[0].indexOf("@");
          chain().deleteRange({ from: start, to: range.to }).insertContentAt(start, [{ type: this.name, attrs: { at } }, { type: "text", text: " " }]).run();
        },
      }),
    ];
  },
});

/** Все дедлайны документа: текст строки, в которой стоит срок, и сам срок. Выполненные задачи пропускаются. */
export function collectDeadlines(doc: unknown, fallback: string): { text: string; at: number }[] {
  const out: { text: string; at: number }[] = [];
  const text = (n: any): string => (n?.type === "deadline" ? "" : n?.text ?? (n?.content ?? []).map(text).join(""));
  const walk = (n: any, done: boolean) => {
    if (!n) return;
    const isDone = done || (n.type === "taskItem" && n.attrs?.checked);
    if (n.type === "paragraph" || n.type === "heading") {
      for (const c of n.content ?? [])
        if (c.type === "deadline" && c.attrs?.at && !isDone) out.push({ text: text(n).replace(/\s+/g, " ").trim() || fallback, at: c.attrs.at });
      return;
    }
    (n.content ?? []).forEach((c: any) => walk(c, isDone));
  };
  walk(doc, false);
  return out;
}
