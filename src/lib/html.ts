// Документ редактора в HTML для предпросмотра версий. Только безопасная разметка.
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

function marks(text: string, ms: any[] = []): string {
  let out = esc(text);
  for (const m of ms) {
    if (m.type === "bold") out = `<strong>${out}</strong>`;
    else if (m.type === "italic") out = `<em>${out}</em>`;
    else if (m.type === "underline") out = `<u>${out}</u>`;
    else if (m.type === "strike") out = `<s>${out}</s>`;
    else if (m.type === "code") out = `<code>${out}</code>`;
    else if (m.type === "highlight") out = `<mark>${out}</mark>`;
  }
  return out;
}

export function docToHtml(n: any, fmtDeadline: (at: number) => string): string {
  if (!n) return "";
  const kids = () => (n.content ?? []).map((c: any) => docToHtml(c, fmtDeadline)).join("");
  switch (n.type) {
    case "doc": return kids();
    case "text": return marks(n.text ?? "", n.marks);
    case "paragraph": return `<p>${kids() || "<br>"}</p>`;
    case "heading": { const l = Math.min(3, Math.max(1, n.attrs?.level ?? 1)); return `<h${l}>${kids()}</h${l}>`; }
    case "bulletList": return `<ul>${kids()}</ul>`;
    case "orderedList": return `<ol>${kids()}</ol>`;
    case "listItem": return `<li>${kids()}</li>`;
    case "taskList": return `<ul class="tasks">${kids()}</ul>`;
    case "taskItem": return `<li class="${n.attrs?.checked ? "done" : ""}"><span class="box"></span><div>${kids()}</div></li>`;
    case "blockquote": return `<blockquote>${kids()}</blockquote>`;
    case "codeBlock": return `<pre><code>${esc((n.content ?? []).map((c: any) => c.text ?? "").join(""))}</code></pre>`;
    case "horizontalRule": return "<hr>";
    case "hardBreak": return "<br>";
    case "image": return typeof n.attrs?.src === "string" && n.attrs.src.startsWith("data:image/") ? `<img src="${esc(n.attrs.src)}" alt="">` : "";
    case "table": return `<table>${kids()}</table>`;
    case "tableRow": return `<tr>${kids()}</tr>`;
    case "tableHeader": return `<th>${kids()}</th>`;
    case "tableCell": return `<td>${kids()}</td>`;
    case "deadline": return `<span class="deadline">${esc(fmtDeadline(n.attrs?.at ?? 0))}</span>`;
    default: return kids();
  }
}
