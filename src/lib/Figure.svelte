<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  export type Callout = {
    text: string;
    /** Где стоит подпись (левый верхний угол), в единицах холста. */
    at: [number, number];
    /** Куда указывает стрелка. */
    to: [number, number];
    width?: number;
    /** С какой стороны подписи выходит стрелка. */
    from?: "left" | "right" | "top" | "bottom";
    bend?: number;
  };

  let { w, h, callouts = [], children, label }: { w: number; h: number; callouts?: Callout[]; children: Snippet; label: string } = $props();

  let box: HTMLElement;
  let cw = $state(0);
  let shown = $state(false);
  const scale = $derived(cw ? Math.min(1.25, cw / w) : 1);

  onMount(() => {
    // Стрелки прорисовываются один раз, когда схема впервые попадает на экран.
    const io = new IntersectionObserver(
      (es) => {
        if (es.some((e) => e.isIntersecting)) {
          shown = true;
          io.disconnect();
        }
      },
      { threshold: 0.35 },
    );
    io.observe(box);
    return () => io.disconnect();
  });

  function start(c: Callout): [number, number] {
    const width = c.width ?? 150;
    const [x, y] = c.at;
    switch (c.from ?? (c.to[0] > x + width ? "right" : c.to[0] < x ? "left" : c.to[1] > y ? "bottom" : "top")) {
      case "right": return [x + width + 6, y + 9];
      case "left": return [x - 6, y + 9];
      case "bottom": return [x + 12, y + 26];
      default: return [x + 12, y - 6];
    }
  }

  function path(c: Callout) {
    const [x1, y1] = start(c);
    const [x2, y2] = c.to;
    const mx = (x1 + x2) / 2;
    const my = (y1 + y2) / 2;
    const dx = x2 - x1;
    const dy = y2 - y1;
    const len = Math.hypot(dx, dy) || 1;
    const k = c.bend ?? 0.18;
    // Контрольная точка смещена перпендикулярно: стрелка изгибается мягко.
    const cx = mx - (dy / len) * len * k;
    const cy = my + (dx / len) * len * k;
    return `M${x1},${y1} Q${cx},${cy} ${x2},${y2}`;
  }
</script>

<figure class="fig" class:shown bind:this={box} bind:clientWidth={cw} style="height:{h * scale}px" aria-label={label}>
  <div class="canvas" style="width:{w}px;height:{h}px;transform:scale({scale})">
    {@render children()}
    <svg class="arrows" viewBox="0 0 {w} {h}" width={w} height={h} aria-hidden="true">
      <defs>
        <marker id="ah" viewBox="0 0 10 10" refX="8.5" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M1,1 L9,5 L1,9" />
        </marker>
      </defs>
      {#each callouts as c, i}
        <path class="arrow" d={path(c)} marker-end="url(#ah)" pathLength="1" style="transition-delay:{120 + i * 110}ms" />
        <circle class="dot" cx={c.to[0]} cy={c.to[1]} r="2.6" style="transition-delay:{360 + i * 110}ms" />
      {/each}
    </svg>
    {#each callouts as c, i}
      <span class="callout" style="left:{c.at[0]}px;top:{c.at[1]}px;width:{c.width ?? 150}px;transition-delay:{i * 110}ms">{c.text}</span>
    {/each}
  </div>
</figure>

<style>
  .fig { position: relative; margin: 14px 0 6px; overflow: hidden; border: 1px solid var(--line); border-radius: var(--r-m); background: radial-gradient(circle at 1px 1px, rgba(232, 229, 222, 0.06) 1px, transparent 0) 0 0 / 16px 16px, var(--sunk); max-width: 800px; }
  .canvas { position: absolute; left: 0; top: 0; transform-origin: 0 0; }
  .arrows { position: absolute; inset: 0; pointer-events: none; overflow: visible; }
  .arrow { fill: none; stroke: var(--muted); stroke-width: 1.3; stroke-dasharray: 1; stroke-dashoffset: 1; transition: stroke-dashoffset 520ms var(--ease); }
  marker path { fill: none; stroke: var(--muted); stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
  .dot { fill: var(--text); opacity: 0; transition: opacity 200ms var(--ease); }
  .callout { position: absolute; font-size: 11.5px; line-height: 1.4; color: var(--text); opacity: 0; transform: translateY(3px); transition: opacity 260ms var(--ease), transform 260ms var(--ease); }
  .shown .arrow { stroke-dashoffset: 0; }
  .shown .dot { opacity: 1; }
  .shown .callout { opacity: 1; transform: none; }
  @media (prefers-reduced-motion: reduce) {
    .arrow { stroke-dashoffset: 0; }
    .dot, .callout { opacity: 1; transform: none; }
  }
</style>
