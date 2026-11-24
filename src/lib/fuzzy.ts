import { switchLayout } from "./transforms";

function norm(s: string): string {
  return s.toLocaleLowerCase().replace(/ё/g, "е");
}

function bigrams(s: string): Map<string, number> {
  const m = new Map<string, number>();
  const t = ` ${s} `;
  for (let i = 0; i < t.length - 1; i++) {
    const g = t.slice(i, i + 2);
    m.set(g, (m.get(g) ?? 0) + 1);
  }
  return m;
}

/** Похожесть двух слов по биграммам (коэффициент Сёренсена). Терпит перестановки и опечатки. */
function dice(a: string, b: string): number {
  if (a === b) return 1;
  if (a.length < 2 || b.length < 2) return a[0] === b[0] ? 0.5 : 0;
  const A = bigrams(a);
  const B = bigrams(b);
  let hit = 0;
  for (const [g, n] of A) hit += Math.min(n, B.get(g) ?? 0);
  return (2 * hit) / (a.length + 1 + b.length + 1);
}

/**
 * Оценка совпадения запроса с текстом, от 0 до 1.
 * Точная подстрока ценится выше всего, затем совпадение каждого слова запроса
 * с лучшим словом текста. Учитывается и запрос, набранный в неправильной раскладке.
 */
export function score(query: string, text: string): number {
  const q = norm(query.trim());
  if (!q) return 1;
  const t = norm(text);
  const variants = [q, norm(switchLayout(q))];
  let best = 0;
  for (const v of variants) {
    if (t.includes(v)) return 1;
    const qWords = v.split(/\s+/).filter(Boolean);
    const tWords = t.split(/[^\p{L}\p{N}]+/u).filter(Boolean).slice(0, 400);
    if (!tWords.length) continue;
    let sum = 0;
    for (const qw of qWords) {
      let w = 0;
      for (const tw of tWords) {
        const s = tw.startsWith(qw) ? 0.95 : dice(qw, tw);
        if (s > w) w = s;
        if (w >= 0.95) break;
      }
      sum += w;
    }
    best = Math.max(best, sum / qWords.length);
  }
  return best;
}

export function search<T>(items: T[], query: string, text: (x: T) => string, threshold = 0.55): T[] {
  if (!query.trim()) return items;
  return items
    .map((x, i) => ({ x, i, s: score(query, text(x)) }))
    .filter((r) => r.s >= threshold)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((r) => r.x);
}
