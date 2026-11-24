/**
 * Переменные в сниппетах.
 * {дата} {время} {буфер} подставляются сами; любое другое имя в фигурных скобках
 * спрашивается при вставке. Чтобы написать фигурную скобку буквально, удвойте её: {{ и }}.
 */

const BUILTIN: Record<string, () => string> = {
  дата: () => new Date().toLocaleDateString("ru-RU"),
  date: () => new Date().toLocaleDateString("en-GB"),
  время: () => new Date().toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" }),
  time: () => new Date().toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" }),
};

const RE = /\{\{|\}\}|\{([^{}\n]{1,40})\}/g;

/** Имена, которые нужно спросить у человека, в порядке появления, без повторов. */
export function askedVars(body: string): string[] {
  const out: string[] = [];
  for (const m of body.matchAll(RE)) {
    const name = m[1]?.trim();
    if (!name) continue;
    const key = name.toLocaleLowerCase();
    if (key in BUILTIN || key === "буфер" || key === "clipboard") continue;
    if (!out.includes(name)) out.push(name);
  }
  return out;
}

export function fill(body: string, values: Record<string, string>, clipboard = ""): string {
  return body.replace(RE, (whole, name?: string) => {
    if (whole === "{{") return "{";
    if (whole === "}}") return "}";
    const n = (name ?? "").trim();
    const key = n.toLocaleLowerCase();
    if (key in BUILTIN) return BUILTIN[key]();
    if (key === "буфер" || key === "clipboard") return clipboard;
    return values[n] ?? "";
  });
}
