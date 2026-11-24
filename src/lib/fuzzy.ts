import { switchLayout } from "./transforms";

function norm(s: string): string {
  return s.toLocaleLowerCase().replace(/ё/g, "е");
}

// Подготовленный текст кэшируется: при наборе запроса тексты не разбираются заново.
type Prep = { t: string; words: string[] };
const prepCache = new Map<string, Prep>();
function prep(text: string): Prep {
  let p = prepCache.get(text);
  if (p) return p;
  if (prepCache.size > 5000) prepCache.clear();
  const t = norm(text.length > 200_000 ? text.slice(0, 200_000) : text);
  const words = [...new Set(t.slice(0, 20_000).split(/[^\p{L}\p{N}]+/u).filter(Boolean))].slice(0, 400);
  p = { t, words };
  prepCache.set(text, p);
  return p;
}

const gramCache = new Map<string, Map<string, number>>();
function bigrams(s: string): Map<string, number> {
  let m = gramCache.get(s);
  if (m) return m;
  if (gramCache.size > 50_000) gramCache.clear();
  m = new Map<string, number>();
  const t = ` ${s} `;
  for (let i = 0; i < t.length - 1; i++) {
    const g = t.slice(i, i + 2);
    m.set(g, (m.get(g) ?? 0) + 1);
  }
  gramCache.set(s, m);
  return m;
}

/** Похожесть двух слов по биграммам (коэффициент Сёренсена). Терпит перестановки и опечатки. */
function dice(a: string, b: string): number {
  if (a === b) return 1;
  if (a.length < 2 || b.length < 2) return a[0] === b[0] ? 0.5 : 0;
  // Слова сильно разной длины похожими не бывают.
  if (b.length > a.length * 3 + 2 || a.length > b.length * 3 + 2) return 0;
  const A = bigrams(a);
  const B = bigrams(b);
  let hit = 0;
  for (const [g, n] of A) hit += Math.min(n, B.get(g) ?? 0);
  return (2 * hit) / (a.length + 1 + b.length + 1);
}

function scorer(query: string): (text: string) => number {
  const q = norm(query.trim());
  if (!q) return () => 1;
  const variants = [...new Set([q, norm(switchLayout(q))])].map((v) => ({ v, words: v.split(/\s+/).filter(Boolean) }));
  return (text: string) => {
    const p = prep(text);
    let best = 0;
    for (const { v, words } of variants) {
      if (p.t.includes(v)) return 1;
      if (!p.words.length) continue;
      let sum = 0;
      for (const qw of words) {
        let w = 0;
        for (const tw of p.words) {
          const s = tw.startsWith(qw) ? 0.95 : dice(qw, tw);
          if (s > w) w = s;
          if (w >= 0.95) break;
        }
        sum += w;
      }
      best = Math.max(best, sum / words.length);
    }
    return best;
  };
}

/**
 * Оценка совпадения запроса с текстом, от 0 до 1.
 * Точная подстрока ценится выше всего, затем совпадение каждого слова запроса
 * с лучшим словом текста. Учитывается и запрос, набранный в неправильной раскладке.
 */
export function score(query: string, text: string): number {
  return scorer(query)(text);
}

export function search<T>(items: T[], query: string, text: (x: T) => string, threshold = 0.55): T[] {
  if (!query.trim()) return items;
  const sc = scorer(query);
  return items
    .map((x, i) => ({ x, i, s: sc(text(x)) }))
    .filter((r) => r.s >= threshold)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((r) => r.x);
}
