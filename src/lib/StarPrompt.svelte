<script lang="ts">
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import { toast } from "./toast.svelte";
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";

  // Один раз после установки просим поставить звезду проекту на GitHub.
  let { onclose }: { onclose: () => void } = $props();
  let opened = $state(false);

  async function finish(thanks: boolean) {
    const s = await api.settings().catch(() => null);
    if (s) await api.settingsSave({ ...s, starAsked: true }).catch(() => {});
    if (thanks) toast(t("star.thanks"));
    onclose();
  }
</script>

<Modal title={t("star.title")} onclose={() => finish(false)} width={420}>
  <div class="star">
    <p>{t("star.text")}</p>
    <button class="btn primary gh" onclick={() => { api.openGithub(); opened = true; }}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 1.6l1.95 4 4.4.6-3.2 3.08.78 4.37L8 11.58l-3.93 2.07.78-4.37L1.65 6.2l4.4-.6z" /></svg>
      {t("star.open")}
    </button>
    <div class="acts">
      <button class="btn" class:primary={opened} onclick={() => finish(true)}><Icon name="check" />{t("star.done")}</button>
      <button class="btn ghost" onclick={() => finish(false)}>{t("star.no")}</button>
    </div>
  </div>
</Modal>

<style>
  .star { display: grid; gap: 14px; }
  p { margin: 0; color: var(--muted); line-height: 1.55; font-size: var(--t-s); }
  .gh { justify-content: center; height: 38px; }
  .gh svg { width: 15px; height: 15px; fill: currentColor; }
  .acts { display: flex; gap: 6px; justify-content: space-between; }
  .acts .btn { flex: 1; justify-content: center; }
</style>
