// Политика конфиденциальности и условия использования. Тексты лежат в docs/legal и попадают в программу при сборке.
import privacyRu from "../../docs/legal/privacy.ru.md?raw";
import privacyEn from "../../docs/legal/privacy.en.md?raw";
import termsRu from "../../docs/legal/terms.ru.md?raw";
import termsEn from "../../docs/legal/terms.en.md?raw";

/** Меняется вместе с редакцией документов: новая версия снова спрашивает согласие. */
export { LEGAL_VERSION } from "./version";

export type LegalDoc = "privacy" | "terms";

export function legalText(doc: LegalDoc, lang: "ru" | "en"): string {
  if (doc === "privacy") return lang === "ru" ? privacyRu : privacyEn;
  return lang === "ru" ? termsRu : termsEn;
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const inline = (s: string) => esc(s).replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");

/** Небольшой разбор Markdown: заголовки, абзацы, списки и таблицы. */
export function renderLegal(md: string): string {
  const out: string[] = [];
  const lines = md.replace(/\r/g, "").split("\n");
  let i = 0;
  while (i < lines.length) {
    const l = lines[i];
    if (!l.trim()) { i++; continue; }
    const h = /^(#{1,3})\s+(.*)$/.exec(l);
    if (h) { out.push(`<h${h[1].length}>${inline(h[2])}</h${h[1].length}>`); i++; continue; }
    if (l.startsWith("|")) {
      const rows: string[][] = [];
      while (i < lines.length && lines[i].startsWith("|")) {
        const cells = lines[i].slice(1, -1).split("|").map((c) => c.trim());
        if (!cells.every((c) => /^-+$/.test(c))) rows.push(cells);
        i++;
      }
      const [head, ...body] = rows;
      out.push(`<table><thead><tr>${head.map((c) => `<th>${inline(c)}</th>`).join("")}</tr></thead><tbody>${body.map((r) => `<tr>${r.map((c) => `<td>${inline(c)}</td>`).join("")}</tr>`).join("")}</tbody></table>`);
      continue;
    }
    if (l.startsWith("- ")) {
      const items: string[] = [];
      while (i < lines.length && lines[i].startsWith("- ")) items.push(`<li>${inline(lines[i++].slice(2))}</li>`);
      out.push(`<ul>${items.join("")}</ul>`);
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#|\||- )/.test(lines[i])) para.push(lines[i++]);
    out.push(`<p>${inline(para.join(" "))}</p>`);
  }
  return out.join("\n");
}
