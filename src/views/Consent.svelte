<script lang="ts">
  import { fade } from "svelte/transition";
  import { t, i18n } from "../lib/i18n.svelte";
  import { LEGAL_VERSION, type LegalDoc } from "../lib/legal";
  import LegalView from "../lib/LegalView.svelte";
  import Modal from "../lib/Modal.svelte";
  import Wordmark from "../lib/Wordmark.svelte";

  // Короткий экран согласия: одна кнопка, а полный текст открывается по серым ссылкам.
  let { onaccept, updated = false }: { onaccept: (version: string) => void; updated?: boolean } = $props();
  let open = $state<LegalDoc | null>(null);
  let busy = $state(false);

  async function accept() {
    busy = true;
    try { await onaccept(LEGAL_VERSION); } finally { busy = false; }
  }
</script>

<div class="wrap" in:fade={{ duration: 180 }}>
  <div class="card">
    <Wordmark size={20} />
    <h1>{updated ? t("legal.updatedTitle") : t("legal.title")}</h1>
    <p class="lead">{t("legal.lead")}</p>
    <button class="btn primary big" onclick={accept} disabled={busy}>{t("legal.accept")}</button>
    <p class="fine">
      {t("legal.fine1")}
      <button class="link" onclick={() => (open = "terms")}>{t("legal.terms")}</button>
      {t("legal.and")}
      <button class="link" onclick={() => (open = "privacy")}>{t("legal.privacy")}</button>{t("legal.fine2")}
    </p>
    <div class="lang" role="radiogroup" aria-label="Language">
      <button role="radio" aria-checked={i18n.lang === "ru"} onclick={() => (i18n.lang = "ru")}>RU</button>
      <button role="radio" aria-checked={i18n.lang === "en"} onclick={() => (i18n.lang = "en")}>EN</button>
    </div>
  </div>
</div>

{#if open}
  <Modal title={open === "terms" ? t("legal.terms") : t("legal.privacy")} onclose={() => (open = null)} width={720}>
    <LegalView doc={open} />
  </Modal>
{/if}

<style>
  .wrap { height: 100%; display: grid; place-items: center; padding: 24px; }
  .card { width: min(420px, 100%); display: grid; gap: 14px; justify-items: start; }
  h1 { margin: 10px 0 0; font: 600 var(--t-xl)/1.25 var(--font-display); }
  .lead { margin: 0; color: var(--muted); line-height: 1.55; }
  .big { width: 100%; justify-content: center; height: 40px; margin-top: 6px; }
  .fine { margin: 0; font-size: var(--t-xs); color: var(--faint); line-height: 1.55; }
  .link { color: var(--faint); text-decoration: underline; text-underline-offset: 3px; font-size: inherit; padding: 0; }
  .link:hover { color: var(--text); }
  .lang { display: flex; gap: 2px; margin-top: 4px; }
  .lang button { font-size: var(--t-xs); color: var(--faint); padding: 3px 8px; border-radius: var(--r-s); }
  .lang button[aria-checked="true"] { color: var(--text); background: var(--raise); }
</style>
