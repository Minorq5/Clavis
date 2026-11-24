<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, listen } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import Wordmark from "../lib/Wordmark.svelte";

  // Окно быстрой заметки: открывается сочетанием из любой программы.
  let text = $state("");
  let area = $state<HTMLTextAreaElement>();
  let busy = $state(false);
  let error = $state("");

  async function focus() {
    await tick();
    area?.focus();
  }

  onMount(() => {
    focus();
    let off: (() => void) | undefined;
    listen("quick:shown", () => { error = ""; focus(); }).then((o) => (off = o));
    return () => off?.();
  });

  async function save() {
    if (busy) return;
    busy = true;
    try {
      await api.quickNote(text);
      text = "";
      error = "";
    } catch (e) {
      error = String(e) === "locked" ? t("quick.locked") : t("toast.error", { e: String(e) });
    } finally {
      busy = false;
    }
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); save(); }
    else if (e.key === "Escape") { e.preventDefault(); api.quickHide(); }
  }
</script>

<div class="quick">
  <header data-tauri-drag-region><Wordmark size={14} /></header>
  <textarea bind:this={area} bind:value={text} onkeydown={onkey} placeholder={t("quick.ph")} spellcheck="true"></textarea>
  <footer>{error || t("quick.hint")}</footer>
</div>

<style>
  .quick { height: 100%; display: grid; grid-template-rows: auto 1fr auto; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-m); overflow: hidden; animation: in 140ms var(--ease); }
  @keyframes in { from { opacity: 0; transform: translateY(-4px); } }
  header { padding: 10px 14px 0; }
  textarea { resize: none; background: none; border: 0; padding: 10px 14px; font-size: var(--t-l); line-height: 1.5; color: var(--text); user-select: text; }
  textarea:focus-visible { outline: none; }
  textarea::placeholder { color: var(--faint); }
  footer { padding: 7px 14px; border-top: 1px solid var(--line); font-size: var(--t-xs); color: var(--faint); }
</style>
