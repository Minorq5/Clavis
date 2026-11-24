const EN = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?@#$^&";
const RU = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,\"№;:?";

const toRu = new Map<string, string>();
const toEn = new Map<string, string>();
for (let i = 0; i < EN.length; i++) {
  toRu.set(EN[i], RU[i]);
  toEn.set(RU[i], EN[i]);
}

/** Переключает раскладку: ghbdtn → привет, руддщ → hello. Направление определяется по большинству букв. */
export function switchLayout(s: string): string {
  let latin = 0;
  let cyr = 0;
  for (const ch of s) {
    if (/[a-z]/i.test(ch)) latin++;
    else if (/[а-яё]/i.test(ch)) cyr++;
  }
  const map = latin >= cyr ? toRu : toEn;
  let out = "";
  for (const ch of s) out += map.get(ch) ?? ch;
  return out;
}

export const upper = (s: string) => s.toLocaleUpperCase();
export const lower = (s: string) => s.toLocaleLowerCase();
export const title = (s: string) =>
  s.toLocaleLowerCase().replace(/(^|[\s\-«"(])(\p{L})/gu, (_, a, b) => a + b.toLocaleUpperCase());

/** Убирает лишние пробелы, неразрывные пробелы, табы и пустые строки подряд. */
export function tidy(s: string): string {
  return s
    .replace(/ /g, " ")
    .replace(/[ \t]+/g, " ")
    .replace(/ *\n */g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function formatJson(s: string): string | null {
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return null;
  }
}

export type TransformId = "plain" | "upper" | "lower" | "title" | "tidy" | "layout" | "json";

export function apply(id: TransformId, s: string): string {
  switch (id) {
    case "upper": return upper(s);
    case "lower": return lower(s);
    case "title": return title(s);
    case "tidy": return tidy(s);
    case "layout": return switchLayout(s);
    case "json": return formatJson(s) ?? s;
    default: return s;
  }
}

/** Похоже ли содержимое на код: для моноширинного превью. */
export function looksLikeCode(s: string): boolean {
  if (formatJson(s) !== null && /[{[]/.test(s.trim()[0] ?? "")) return true;
  const signals = [/[{};]\s*$/m, /^\s{2,}\S/m, /=>|::|<\/?\w+>|\bfunction\b|\bconst\b|\bdef\b|\bfn\b/];
  return signals.filter((r) => r.test(s)).length >= 2;
}
