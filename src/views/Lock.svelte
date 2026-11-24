<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Stage } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import Wordmark from "../lib/Wordmark.svelte";

  let { stage, ondone }: { stage: Stage; ondone: () => void } = $props();

  let pin = $state("");
  let err = $state("");
  let recover = $state(false);
  let phrase = $state("");
  let newPin = $state("");
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  let hello = $state(false);

  onMount(async () => {
    input?.focus();
    if (stage !== "locked") return;
    const st = await api.helloStatus().catch(() => null);
    hello = !!st?.available && !!st?.enabled;
    // Если включён Windows Hello, сразу предлагаем войти через него.
    if (hello) helloGo();
  });

  async function helloGo() {
    err = "";
    busy = true;
    try {
      if (await api.unlockHello()) ondone();
    } catch (e) {
      const m = String(e);
      if (m !== "cancelled") err = t("lock.helloFailed");
    } finally {
      busy = false;
      input?.focus();
    }
  }

  async function unlock(e: Event) {
    e.preventDefault();
    busy = true;
    try {
      const r = await api.unlock(pin);
      pin = "";
      if (r.ok) ondone();
      else if (r.reason === "tpm_locked") err = t("lock.tpmLocked");
      else if (r.reason === "guard_missing") { err = t("lock.guardMissing"); recover = true; }
      else if (r.attemptsLeft <= 0) { err = t("lock.disabled"); recover = true; }
      else err = t("lock.wrong", { n: r.attemptsLeft });
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  async function doRecover(e: Event) {
    e.preventDefault();
    const words = phrase.trim().toLocaleLowerCase().split(/\s+/);
    busy = true;
    try {
      const r = await api.recover(words, newPin);
      if (r.ok) ondone();
      else err = t("lock.recoverBad");
    } finally {
      busy = false;
    }
  }
</script>

<div class="lock">
  <Wordmark size={20} />
  {#if stage === "blocked"}
    <h1>{t("blocked.title")}</h1>
    <p class="muted">{t("blocked.text")}</p>
  {:else if !recover}
    <form onsubmit={unlock}>
      <label for="pin">{t("lock.pin")}</label>
      <input id="pin" bind:this={input} class="field pin" type="password" inputmode="numeric" maxlength="12" bind:value={pin} oninput={() => (err = "")} />
      {#if err}<p class="err">{err}</p>{/if}
      <button class="btn primary" disabled={pin.length < 4 || busy}>{t("lock.go")}</button>
      {#if hello}<button type="button" class="btn" onclick={helloGo} disabled={busy}>{t("lock.hello")}</button>{/if}
      <button type="button" class="btn ghost" onclick={() => { recover = true; err = ""; }}>{t("lock.forgot")}</button>
    </form>
  {:else}
    <form onsubmit={doRecover} class="wide">
      {#if err && !phrase}<p class="err">{err}</p>{/if}
      <p class="muted">{t("lock.recoverHelp")}</p>
      <textarea class="field phrase" rows="4" bind:value={phrase} spellcheck="false"></textarea>
      <label for="np">{t("setup.pinLabel")}</label>
      <input id="np" class="field pin" type="password" inputmode="numeric" maxlength="12" bind:value={newPin} />
      {#if err}<p class="err">{err}</p>{/if}
      <div class="row">
        <button type="button" class="btn ghost" onclick={() => (recover = false)}>{t("setup.back")}</button>
        <button class="btn primary" disabled={phrase.trim().split(/\s+/).length !== 24 || !/^\d{4,12}$/.test(newPin)}>{t("lock.recoverGo")}</button>
      </div>
    </form>
  {/if}
</div>

<style>
  .lock { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 28px; padding: 40px; text-align: center; }
  h1 { font: 600 var(--t-xl)/1.3 var(--font-display); margin: 0; }
  p { margin: 0; max-width: 44ch; line-height: 1.6; }
  form { display: grid; gap: 10px; width: 240px; justify-items: stretch; }
  form.wide { width: 420px; text-align: left; }
  label { font-size: var(--t-s); color: var(--muted); text-align: left; }
  .pin { font-size: 18px; letter-spacing: 0.3em; text-align: center; }
  form.wide .pin { text-align: left; width: 200px; }
  .phrase { font-family: var(--mono); font-size: 12.5px; resize: none; line-height: 1.6; }
  .err { color: var(--danger); font-size: var(--t-s); }
  .btn { justify-content: center; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
</style>
