// Пометка комментария в тексте общей заметки. Сами комментарии лежат в документе Yjs рядом с текстом.
import { Mark } from "@tiptap/core";

export interface CommentData {
  id: string;
  author: string;
  color: string;
  text: string;
  quote: string;
  at: number;
  resolved: boolean;
  /** Устройства упомянутых участников: им придёт уведомление. */
  mentions?: string[];
  replies: { author: string; color: string; text: string; at: number; mentions?: string[] }[];
}

export const CommentMark = Mark.create({
  name: "comment",
  inclusive: false,
  excludes: "",
  addAttributes: () => ({ id: { default: "", parseHTML: (el) => el.getAttribute("data-comment"), renderHTML: (a) => ({ "data-comment": a.id }) } }),
  parseHTML: () => [{ tag: "span[data-comment]" }],
  renderHTML: ({ HTMLAttributes }) => ["span", { ...HTMLAttributes, class: "cmt" }, 0],
});
