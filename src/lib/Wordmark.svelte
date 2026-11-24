<script lang="ts">
  let { size = 18 }: { size?: number } = $props();
</script>

<!-- Знак: ключ. -->
<span class="mark" style="font-size:{size}px">
  <svg viewBox="0 0 24 24" width={size * 1.15} height={size * 1.15} aria-hidden="true">
    <circle cx="7" cy="12" r="4.2" />
    <path d="M11.2 12H22M17.5 12v3.5M20.8 12v2.5" />
  </svg>
  <span>clavis</span>
</span>

<style>
  .mark { display: inline-flex; align-items: center; gap: 0.35em; font-family: var(--font-display); font-weight: 600; letter-spacing: 0.01em; color: var(--text); }
  svg { fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: butt; }
</style>
