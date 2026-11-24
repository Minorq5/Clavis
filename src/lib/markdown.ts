// Документ редактора в Markdown.
import { formatDeadline } from "./deadline";
type N = { type: string; attrs?: Record<string, any>; content?: N[]; text?: string; marks?: { type: string; attrs?: any }[] };

function inline(nodes: N[] = []): string {
  return nodes
    .map((n) => {
      if (n.type === "hardBreak") return "  \n";
      if (n.type === "deadline") return "@" + formatDeadline(n.attrs?.at ?? 0);
      if (n.type !== "text") return "";
      let t = (n.text ?? "").replace(/([*_`~\\[\]])/g, "\\$1");
      const has = (m: string) => n.marks?.some((x) => x.type === m);
      if (has("code")) t = "`" + (n.text ?? "") + "`";
      if (has("bold")) t = `**${t}**`;
      if (has("italic")) t = `*${t}*`;
      if (has("strike")) t = `~~${t}~~`;
      if (has("highlight")) t = `==${t}==`;
      const link = n.marks?.find((m) => m.type === "link");
      if (link) t = `[${t}](${link.attrs?.href ?? ""})`;
      return t;
    })
    .join("");
}

function block(n: N, indent = ""): string {
  switch (n.type) {
    case "heading": return "#".repeat(n.attrs?.level ?? 1) + " " + inline(n.content);
    case "paragraph": return indent + inline(n.content);
    case "blockquote": return (n.content ?? []).map((c) => "> " + block(c)).join("\n>\n");
    case "codeBlock": return "```" + (n.attrs?.language ?? "") + "\n" + (n.content ?? []).map((c) => c.text ?? "").join("") + "\n```";
    case "horizontalRule": return "---";
    case "image": return n.attrs?.src?.startsWith("data:") || n.attrs?.src?.startsWith("clavis-img:") ? "![изображение]()" : `![](${n.attrs?.src ?? ""})`;
    case "bulletList":
    case "orderedList":
    case "taskList":
      return (n.content ?? [])
        .map((li, i) => {
          const mark = n.type === "orderedList" ? `${i + 1}.` : n.type === "taskList" ? (li.attrs?.checked ? "- [x]" : "- [ ]") : "-";
          const [first, ...rest] = li.content ?? [];
          const head = indent + mark + " " + (first ? inline(first.content) : "");
          const tail = rest.map((c) => block(c, indent + "  ")).filter(Boolean).join("\n");
          return tail ? head + "\n" + tail : head;
        })
        .join("\n");
    case "table": {
      const rows = (n.content ?? []).map((r) => (r.content ?? []).map((c) => (c.content ?? []).map((p) => inline(p.content)).join(" ").replace(/\|/g, "\\|")));
      if (!rows.length) return "";
      const head = `| ${rows[0].join(" | ")} |`;
      const sep = `| ${rows[0].map(() => "---").join(" | ")} |`;
      return [head, sep, ...rows.slice(1).map((r) => `| ${r.join(" | ")} |`)].join("\n");
    }
    default:
      return (n.content ?? []).map((c) => block(c, indent)).join("\n\n");
  }
}

export function toMarkdown(doc: unknown): string {
  const d = doc as N;
  return (d?.content ?? []).map((n) => block(n)).filter((s) => s !== undefined).join("\n\n").replace(/\n{3,}/g, "\n\n").trim() + "\n";
}
