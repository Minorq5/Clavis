<script lang="ts">
  import Modal from "./Modal.svelte";
  import { dialog } from "./dialog.svelte";
  import { t } from "./i18n.svelte";

  let value = $state("");
  $effect(() => {
    const d = dialog.current;
    if (d?.kind === "prompt") value = d.value;
  });

  function close(ok: boolean) {
    const d = dialog.current;
    if (!d) return;
    dialog.current = null;
    if (d.kind === "confirm") d.resolve(ok);
    else d.resolve(ok ? value : null);
  }
</script>

{#if dialog.current}
  {@const d = dialog.current}
  <Modal title={d.title} onclose={() => close(false)} width={420}>
    <form class="dlg" onsubmit={(e) => { e.preventDefault(); close(true); }}>
      {#if d.text}<p>{d.text}</p>{/if}
      {#if d.kind === "prompt"}
        <input class="field" type={d.password ? "password" : "text"} bind:value placeholder={d.placeholder ?? ""} spellcheck="false" autocomplete="off" />
      {/if}
      <div class="acts">
        <button type="button" class="btn ghost" onclick={() => close(false)}>{t("common.cancel")}</button>
        <button class="btn primary" class:danger={d.kind === "confirm" && d.danger}>{d.ok}</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .dlg { display: grid; gap: 14px; }
  p { margin: 0; line-height: 1.55; color: var(--muted); font-size: var(--t-s); }
  .acts { display: flex; justify-content: flex-end; gap: 6px; }
  .btn.primary.danger { background: var(--danger); border-color: var(--danger); color: #fff; }
</style>
