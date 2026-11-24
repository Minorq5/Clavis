import { formatDeadline } from "./deadline";
// Экспорт заметки в Word (.docx) прямо из документа редактора.
import {
  AlignmentType, BorderStyle, Document, ExternalHyperlink, HeadingLevel, ImageRun, LevelFormat,
  Packer, Paragraph, ShadingType, Table, TableCell, TableRow, TextRun, WidthType,
} from "docx";

type Mark = { type: string; attrs?: Record<string, any> };
type PMNode = { type: string; attrs?: Record<string, any>; content?: PMNode[]; text?: string; marks?: Mark[] };

const FONT = "Calibri";
const MONO = "Consolas";

function runs(nodes: PMNode[] = [], base: { mono?: boolean; italic?: boolean; color?: string } = {}) {
  const out: (TextRun | ExternalHyperlink)[] = [];
  for (const n of nodes) {
    if (n.type === "hardBreak") { out.push(new TextRun({ break: 1 })); continue; }
    if (n.type === "deadline") { out.push(new TextRun({ text: "@" + formatDeadline(n.attrs?.at ?? 0), bold: true })); continue; }
    if (n.type !== "text") continue;
    const has = (t: string) => n.marks?.some((m) => m.type === t) ?? false;
    const link = n.marks?.find((m) => m.type === "link");
    const run = new TextRun({
      text: n.text ?? "",
      bold: has("bold"),
      italics: has("italic") || base.italic,
      underline: has("underline") || link ? {} : undefined,
      strike: has("strike"),
      highlight: has("highlight") ? "yellow" : undefined,
      font: has("code") || base.mono ? MONO : FONT,
      color: link ? "1F4E79" : base.color,
      size: has("code") ? 20 : undefined,
    });
    out.push(link ? new ExternalHyperlink({ link: link.attrs?.href ?? "", children: [run] }) : run);
  }
  return out;
}

function dataUrlToBytes(src: string): { data: Uint8Array; type: "png" | "jpg" | "gif" | "bmp" } | null {
  const m = src.match(/^data:image\/(png|jpe?g|gif|bmp);base64,(.+)$/);
  if (!m) return null;
  const bin = atob(m[2]);
  const data = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) data[i] = bin.charCodeAt(i);
  return { data, type: (m[1] === "jpeg" ? "jpg" : m[1]) as "png" | "jpg" | "gif" | "bmp" };
}

function imageSize(src: string): Promise<[number, number]> {
  return new Promise((res) => {
    const img = new Image();
    img.onload = () => res([img.naturalWidth, img.naturalHeight]);
    img.onerror = () => res([600, 400]);
    img.src = src;
  });
}

async function blocks(nodes: PMNode[] = [], ctx: { list?: { ordered: boolean; level: number } } = {}): Promise<(Paragraph | Table)[]> {
  const out: (Paragraph | Table)[] = [];
  for (const n of nodes) {
    switch (n.type) {
      case "heading": {
        const level = [HeadingLevel.HEADING_1, HeadingLevel.HEADING_2, HeadingLevel.HEADING_3][(n.attrs?.level ?? 1) - 1];
        out.push(new Paragraph({ heading: level, children: runs(n.content) }));
        break;
      }
      case "paragraph":
        out.push(new Paragraph({
          children: runs(n.content),
          ...(ctx.list ? { numbering: { reference: ctx.list.ordered ? "ol" : "ul", level: ctx.list.level } } : {}),
        }));
        break;
      case "bulletList":
      case "orderedList": {
        const level = ctx.list ? ctx.list.level + 1 : 0;
        for (const li of n.content ?? []) {
          const [first, ...rest] = li.content ?? [];
          if (first) out.push(...(await blocks([first], { list: { ordered: n.type === "orderedList", level } })));
          out.push(...(await blocks(rest, { list: { ordered: n.type === "orderedList", level } })));
        }
        break;
      }
      case "taskList":
        for (const li of n.content ?? []) {
          const done = !!li.attrs?.checked;
          const [first, ...rest] = li.content ?? [];
          out.push(new Paragraph({
            indent: { left: 360 * ((ctx.list?.level ?? -1) + 1) },
            children: [new TextRun({ text: done ? "☑ " : "☐ ", font: "Segoe UI Symbol" }), ...runs(first?.content, done ? { color: "808080" } : {})],
          }));
          out.push(...(await blocks(rest, { list: { ordered: false, level: (ctx.list?.level ?? -1) + 1 } })));
        }
        break;
      case "blockquote":
        for (const p of n.content ?? []) {
          out.push(new Paragraph({
            indent: { left: 360 },
            border: { left: { style: BorderStyle.SINGLE, size: 12, color: "BFBFBF", space: 8 } },
            children: runs(p.content, { italic: true, color: "595959" }),
          }));
        }
        break;
      case "codeBlock": {
        const lines = (n.content?.map((c) => c.text ?? "").join("") ?? "").split("\n");
        for (const line of lines) {
          out.push(new Paragraph({
            shading: { type: ShadingType.CLEAR, fill: "F3F3F3", color: "auto" },
            spacing: { before: 0, after: 0 },
            children: [new TextRun({ text: line || " ", font: MONO, size: 19 })],
          }));
        }
        out.push(new Paragraph({ children: [] }));
        break;
      }
      case "horizontalRule":
        out.push(new Paragraph({ border: { bottom: { style: BorderStyle.SINGLE, size: 6, color: "BFBFBF", space: 1 } }, children: [] }));
        break;
      case "image": {
        const src = n.attrs?.src ?? "";
        const img = dataUrlToBytes(src);
        if (!img) break;
        let [w, h] = await imageSize(src);
        const max = 600;
        if (w > max) { h = Math.round((h * max) / w); w = max; }
        out.push(new Paragraph({ alignment: AlignmentType.CENTER, children: [new ImageRun({ type: img.type, data: img.data, transformation: { width: w, height: h } } as any)] }));
        break;
      }
      case "table": {
        const rows: TableRow[] = [];
        for (const r of n.content ?? []) {
          const cells: TableCell[] = [];
          for (const c of r.content ?? []) {
            const kids = (await blocks(c.content)) as Paragraph[];
            cells.push(new TableCell({
              children: kids.length ? kids : [new Paragraph({ children: [] })],
              columnSpan: c.attrs?.colspan ?? 1,
              rowSpan: c.attrs?.rowspan ?? 1,
              shading: c.type === "tableHeader" ? { type: ShadingType.CLEAR, fill: "EDEDED", color: "auto" } : undefined,
              margins: { top: 60, bottom: 60, left: 100, right: 100 },
            }));
          }
          rows.push(new TableRow({ children: cells }));
        }
        out.push(new Table({ rows, width: { size: 100, type: WidthType.PERCENTAGE } }));
        out.push(new Paragraph({ children: [] }));
        break;
      }
      default:
        if (n.content) out.push(...(await blocks(n.content, ctx)));
    }
  }
  return out;
}

const levels = (ordered: boolean) =>
  Array.from({ length: 6 }, (_, level) => ({
    level,
    format: ordered ? LevelFormat.DECIMAL : LevelFormat.BULLET,
    text: ordered ? `%${level + 1}.` : ["•", "◦", "▪"][level % 3],
    alignment: AlignmentType.LEFT,
    style: { paragraph: { indent: { left: 360 * (level + 1), hanging: 260 } } },
  }));

export async function toDocx(doc: unknown, title: string): Promise<Uint8Array> {
  const d = new Document({
    title,
    creator: "Clavis",
    styles: {
      default: { document: { run: { font: FONT, size: 22 }, paragraph: { spacing: { after: 120, line: 300 } } } },
    },
    numbering: { config: [{ reference: "ul", levels: levels(false) }, { reference: "ol", levels: levels(true) }] },
    sections: [{ children: await blocks((doc as PMNode).content) }],
  });
  const blob = await Packer.toBlob(d);
  return new Uint8Array(await blob.arrayBuffer());
}
