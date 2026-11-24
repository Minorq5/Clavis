<script lang="ts">
  import { legalText, renderLegal, type LegalDoc } from "./legal";
  import { i18n } from "./i18n.svelte";
  let { doc }: { doc: LegalDoc } = $props();
  const html = $derived(renderLegal(legalText(doc, i18n.lang)));
</script>

<article class="legal">{@html html}</article>

<style>
  .legal { font-size: var(--t-s); line-height: 1.6; color: var(--muted); user-select: text; }
  .legal :global(h1) { font: 600 var(--t-l)/1.3 var(--font-display); color: var(--text); margin: 0 0 4px; }
  .legal :global(h2) { font: 600 var(--t-m)/1.3 var(--font-display); color: var(--text); margin: 22px 0 6px; }
  .legal :global(p) { margin: 0 0 8px; max-width: 72ch; }
  .legal :global(ul) { margin: 0 0 8px; padding-left: 1.2em; }
  .legal :global(li) { margin: 3px 0; }
  .legal :global(table) { width: 100%; border-collapse: collapse; margin: 6px 0 10px; font-size: var(--t-xs); }
  .legal :global(th), .legal :global(td) { border: 1px solid var(--line); padding: 6px 8px; text-align: left; vertical-align: top; }
  .legal :global(th) { color: var(--text); background: var(--raise); font-weight: 600; }
</style>
