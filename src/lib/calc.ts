// Безопасный калькулятор для быстрых действий: без eval, только арифметика.
// Поддерживает + - * / ^ %, скобки, десятичную запятую, пробелы в числах и функции sqrt, abs, round.

type Tok = { t: "num"; v: number } | { t: "op"; v: string } | { t: "fn"; v: string };

const FNS: Record<string, (x: number) => number> = {
  sqrt: Math.sqrt, корень: Math.sqrt, abs: Math.abs, round: Math.round, sin: Math.sin, cos: Math.cos, ln: Math.log, log: Math.log10,
};

function tokenize(src: string): Tok[] | null {
  const s = src.replace(/×/g, "*").replace(/÷/g, "/").replace(/−/g, "-").replace(/,/g, ".").trim();
  const out: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (/\s/.test(c)) { i++; continue; }
    const num = /^\d[\d\s]*(\.\d+)?|^\.\d+/.exec(s.slice(i));
    if (num) {
      out.push({ t: "num", v: parseFloat(num[0].replace(/\s/g, "")) });
      i += num[0].length;
      continue;
    }
    const fn = /^[a-zа-я]+/i.exec(s.slice(i));
    if (fn) {
      const name = fn[0].toLowerCase();
      if (name === "pi" || name === "пи") out.push({ t: "num", v: Math.PI });
      else if (FNS[name]) out.push({ t: "fn", v: name });
      else return null;
      i += fn[0].length;
      continue;
    }
    if ("+-*/^%()".includes(c)) { out.push({ t: "op", v: c }); i++; continue; }
    if (c === "=" && i === s.length - 1) { i++; continue; }
    return null;
  }
  return out;
}

/** Вычисляет выражение. Возвращает null, если это не выражение (например, просто число или текст). */
export function calc(src: string): number | null {
  if (src.length > 200) return null;
  const parsed = tokenize(src);
  if (!parsed) return null;
  const toks: Tok[] = parsed;
  if (toks.length < 3 || !toks.some((t) => t.t === "op" && t.v !== "(" && t.v !== ")")) return null;
  let i = 0;
  const peek = () => toks[i];
  const take = () => toks[i++];
  function primary(): number {
    const t = take();
    if (!t) throw 0;
    if (t.t === "num") return t.v;
    if (t.t === "fn") return FNS[t.v](unary());
    if (t.v === "(") { const v = expr(); if (take()?.v !== ")") throw 0; return v; }
    throw 0;
  }
  function postfix(): number {
    let v = primary();
    // «200 + 15%» и «15% от»: процент как сотая часть.
    while (peek()?.t === "op" && peek()!.v === "%" && (toks[i + 1] === undefined || (toks[i + 1].t === "op" && toks[i + 1].v !== "("))) { take(); v = v / 100; }
    return v;
  }
  function power(): number { const b = postfix(); if (peek()?.v === "^") { take(); return Math.pow(b, unary()); } return b; }
  function unary(): number { if (peek()?.v === "-") { take(); return -unary(); } if (peek()?.v === "+") { take(); return unary(); } return power(); }
  function term(): number {
    let v = unary();
    while (peek()?.t === "op" && "*/%".includes(String(peek()!.v))) {
      const op = take().v; const r = unary();
      v = op === "*" ? v * r : op === "/" ? v / r : v % r;
    }
    return v;
  }
  function expr(): number {
    let v = term();
    while (peek()?.t === "op" && "+-".includes(String(peek()!.v))) {
      const op = take().v;
      const start = i;
      let r = term();
      // «200 + 15%» означает 200 + 15% от 200.
      if (toks[i - 1]?.t === "op" && toks[i - 1].v === "%" && i - start === 2) r = v * r;
      v = op === "+" ? v + r : v - r;
    }
    return v;
  }
  try {
    const v = expr();
    if (i !== toks.length || !Number.isFinite(v)) return null;
    return Math.round(v * 1e10) / 1e10;
  } catch {
    return null;
  }
}

export function formatNumber(n: number, lang: "ru" | "en"): string {
  return n.toLocaleString(lang === "ru" ? "ru-RU" : "en-US", { maximumFractionDigits: 10 }).replace(/\u00a0/g, " ");
}
