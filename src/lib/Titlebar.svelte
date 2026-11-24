<script lang="ts">
  import { inTauri } from "./api";
  import { t } from "./i18n.svelte";

  let { title = "" }: { title?: string } = $props();

  async function win() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow();
  }
  const minimize = async () => (await win()).minimize();
  const toggleMax = async () => (await win()).toggleMaximize();
  const close = async () => (await win()).hide();
</script>

<div class="bar" data-tauri-drag-region>
  <span class="title" data-tauri-drag-region>{title}</span>
  {#if inTauri}
    <div class="controls">
      <button aria-label={t("win.min")} title={t("win.min")} onclick={minimize}><svg viewBox="0 0 10 10"><path d="M1 5h8" /></svg></button>
      <button aria-label={t("win.max")} title={t("win.max")} onclick={toggleMax}><svg viewBox="0 0 10 10"><path d="M1.5 1.5h7v7h-7z" /></svg></button>
      <button aria-label={t("win.close")} title={t("win.close")} class="x" onclick={close}><svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg></button>
    </div>
  {/if}
</div>

<style>
  .bar {
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 14px;
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  .title { font-size: var(--t-xs); color: var(--faint); }
  .controls { display: flex; height: 100%; }
  .controls button { width: 44px; height: 100%; display: grid; place-items: center; color: var(--muted); }
  .controls button:hover { background: var(--raise); color: var(--text); }
  .controls .x:hover { background: #8f2f22; color: #fff; }
  svg { width: 10px; height: 10px; stroke: currentColor; fill: none; stroke-width: 1; }
</style>
