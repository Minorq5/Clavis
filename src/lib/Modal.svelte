<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";

  let { title, onclose, children, width = 460 }: { title: string; onclose: () => void; children: Snippet; width?: number } = $props();
  let box: HTMLDivElement;

  $effect(() => {
    box?.querySelector<HTMLElement>("input, textarea, button.primary")?.focus();
  });
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); onclose(); } }} />

<div class="backdrop" transition:fade={{ duration: 120 }} onclick={onclose} role="presentation"></div>
<div class="modal" role="dialog" aria-modal="true" aria-label={title} bind:this={box} style="width:min({width}px, calc(100vw - 32px))" transition:scale={{ start: 0.97, duration: 160, easing: cubicOut }}>
  <header>
    <h2>{title}</h2>
    <button class="x" onclick={onclose} aria-label={t("win.close")}><Icon name="close" /></button>
  </header>
  <div class="body">{@render children()}</div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(8, 8, 8, 0.6); z-index: 50; }
  .modal { position: fixed; left: 50%; top: 14vh; translate: -50% 0; z-index: 51; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-m); box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55); max-height: 76vh; display: grid; grid-template-rows: auto 1fr; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px 10px 20px; }
  h2 { margin: 0; font: 600 var(--t-l)/1.3 var(--font-display); }
  .x { color: var(--faint); padding: 6px; border-radius: var(--r-s); display: grid; }
  .x:hover { color: var(--text); background: var(--raise); }
  .body { padding: 0 20px 20px; overflow-y: auto; }
</style>
